# exit-copies implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The 11 operator exit copies outside `join.cpp` (#154) are gone or kept with a reason:
`expr.cpp`'s `ColumnRef` copy, the filter's and project's and window's input-column copies, and
the aggregate's key copies; the four Welford struct members and one merged-state child stay.

**Architecture:** `expr.cpp` gains `evaluate_column`, which borrows an input column for a bare
`ColumnRef` and owns everything else; `build_column` becomes its owning wrapper and the
view-only callers switch to it. Filter, project and window build their output through
refcounted-scatter's `TableResult` (`owning`, `select`, public `owners`/`columns`), sharing the
input's column owners instead of copying. The aggregate releases groupby's fresh key tables.
Assembling a handle through the public fields is fine and is why they are public, but
`register_handle` — the only path to a handle number — refuses one of no columns or whose names
or owners do not number its columns, so a hand-built handle has to be complete before it is
registered (added by refcounted-scatter's completeness pass, which is where this plan's
`push_back` steps would otherwise have reopened [#164](../tickets/corpus-coverage.md#t164)).

**Tech stack:** C++ against libcudf 25.02 and 26.02/25.10a; gtest on a device with the RMM
statistics adaptor `main()` installs.

**Spec:** [`exit-copies.md`](exit-copies.md) — frozen (9e563348). Depends on refcounted-scatter's
`TableResult` (`refcounted-scatter-impl.md` Task 1) and its `allocated_by` test helper (Task 4).

## Global constraints

- `TableResult` is used, not changed: its shape is refcounted-scatter's.
- `join.cpp` is not touched: join-session-cpp takes its ten sites.
- No operator's output changes: every existing gtest, harness case and corpus cell stays green
  unchanged. No ABI, wire, Rust or golden change.
- A copy kept is kept with a one-line comment saying why it must own.
- Commits at most 10 lines; device cycles foreground; never build in the primary checkout.

## Review focus

1. **A column projected twice** (tpcds q84 `c_customer_id@0` twice, q85 two avgs twice; device
   cells on at tp1-single). Expected: two output columns sharing one owner, the same values.
   Pinned in Task 3.
2. **A filter whose projection repeats an ordinal.** The spec asked for a refusal, written when an
   exit copy was a move; with per-column owners a repeat is two entries sharing one owner, so it
   is answered, not refused, as the spec now says. Pinned in Task 2.
3. **A borrowed view outliving its table.** `evaluate_column`'s `ColumnRef` view is valid only
   while the input table lives; every converted caller uses it before returning. Expected: no
   site stores a borrowed view past its input — checked by Task 1's rule and the full tiers.
4. **A filter on a bare boolean column** (`WHERE b`): the mask is the input's own column,
   borrowed. Expected: the rows where `b` is true, NULL dropping the row. Pinned in Task 1.
5. **A window over a wide input.** Expected: every input column passed through without a copy,
   names and order unchanged. Pinned in Task 3.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/peacock/expr.h:46-47`, `cpp/src/expr.cpp` | `EvaluatedColumn`, `evaluate_column`; the view-only callers |
| `cpp/src/operators/filter.cpp` | mask by `evaluate_column`; projection by `select` |
| `cpp/src/operators/project.cpp`, `window.cpp` | output sharing the input's owners |
| `cpp/src/operators/sort.cpp`, `aggregate.cpp` | `evaluate_column` for keys and arguments; groupby keys released |
| `cpp/tests/gpu/test_plan_executor.cpp` | the allocation cases |
| `llm-wiki/tickets/corpus-coverage.md`, `build-test.md` | #154 reworded to `join.cpp`'s sites; counts |

---

### Task 1: A bare `ColumnRef` evaluates to a borrowed view

**Files:**
- Modify: `cpp/src/peacock/expr.h:45-47`, `cpp/src/expr.cpp:505` and `:801-904`
- Modify: `cpp/src/operators/filter.cpp:27`, `sort.cpp:30-42`, `aggregate.cpp:203,354,452`,
  `window.cpp:58-74`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Produces:

```cpp
// expr.h
/// What an expression evaluated to: a column the evaluation made, or one of the input table's
/// own columns, borrowed — a bare ColumnRef is the second. Copying it was #154's costliest
/// site: a whole column per batch on every predicate the AST refuses.
struct EvaluatedColumn {
  std::unique_ptr<cudf::column> owned;  // null when borrowed
  cudf::column_view borrowed;           // valid while the input table lives
  [[nodiscard]] cudf::column_view view() const { return owned ? owned->view() : borrowed; }
  /// Ownership, for a caller that keeps the column past the input: the made column, or a copy
  /// of the borrowed one — the one place a borrowed column is copied.
  [[nodiscard]] std::unique_ptr<cudf::column> take() && {
    return owned ? std::move(owned) : std::make_unique<cudf::column>(borrowed);
  }
};

EvaluatedColumn evaluate_column(const fb::Expr* expr, cudf::table_view const& table);

// Unchanged signature: `return evaluate_column(expr, table).take();`
std::unique_ptr<cudf::column> build_column(const fb::Expr* expr, cudf::table_view const& table);
```

- [ ] **Step 1: The cases, red.**

```cpp
/// A one-expression buffer: the Expr table finished as the root, so a test can hand an
/// fb::Expr* to the evaluator without a plan around it.
static const fb::Expr* finished_expr(flatbuffers::FlatBufferBuilder& fbb,
                                     flatbuffers::Offset<fb::Expr> e) {
  fbb.Finish(e);
  return flatbuffers::GetRoot<fb::Expr>(fbb.GetBufferPointer());
}

TEST(ExitCopies, AColumnRefIsBorrowedNotCopied) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  cols.push_back(int64_column({1, 2, 3}));
  cudf::table table(std::move(cols));
  flatbuffers::FlatBufferBuilder fbb;
  const fb::Expr* ref = finished_expr(fbb, make_col_ref(fbb, 0, "a"));
  peacock::EvaluatedColumn e;
  auto got = allocated_by([&] { e = peacock::evaluate_column(ref, table.view()); });
  EXPECT_EQ(got.total, 0) << "a bare column reference allocates nothing";
  EXPECT_EQ(e.owned, nullptr);
  EXPECT_EQ(e.view().head<int64_t>(), table.view().column(0).head<int64_t>());
}

TEST(ExitCopies, ANonAstPredicateDoesNotCopyTheColumnsItReads) {
  // c_acctbal > 0.00 is a decimal comparison, which cudf's AST refuses, so it runs through
  // build_column_binary — whose ColumnRef operand used to be a whole-column copy per batch.
  flatbuffers::FlatBufferBuilder fbb;
  auto path = fbb.CreateString(parquet_path("customer"));
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {{"c_acctbal", fb::DataType_Decimal128}});
  // projection {0}: an ordinal into this one-field schema, which the scan resolves to the file's
  // column by name — `{5}` (the file's position) is out of this schema's range, and scan.cpp
  // drops it and reads no column.
  auto scan = fb::CreateCudfScan(fbb, paths, schema, fbb.CreateVector(std::vector<uint32_t>{0}));
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
  auto pred = make_binary_expr(fbb, make_col_ref(fbb, 0, "c_acctbal"), fb::BinaryOp_Gt,
                               make_decimal_literal(fbb, /*hi=*/0, /*lo=*/0, /*precision=*/15,
                                                    /*scale=*/2));
  auto filter = fb::CreateCudfFilter(fbb, pred, scan_node);
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfFilter, filter.Union()));
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{0};
  uint64_t in = session.execute_scan_rowgroups(0, g, nullptr);
  const int64_t rows = session.table_for(in).num_rows();
  uint64_t counts[1] = {1}, out = 0;
  size_t produced = 0;
  auto got = allocated_by([&] { session.execute_node(1, &in, counts, 1, &out, 1, &produced, nullptr); });
  // The mask (a BOOL8 per row, and the comparison's own) and the filtered DECIMAL128 rows: under
  // 24 bytes a row. A copied operand adds 16 more.
  EXPECT_LT(got.total, rows * 24) << got.total << " bytes for " << rows << " rows";
}

TEST(ExitCopies, AFilterOnABareBooleanColumnBorrowsItsMask) {
  std::vector<std::unique_ptr<cudf::column>> cols;
  cols.push_back(int64_column({1, 2, 3}));
  auto b = cudf::make_numeric_column(cudf::data_type{cudf::type_id::BOOL8}, 3);
  std::vector<int8_t> bits{1, 0, 1};
  cudaMemcpy(b->mutable_view().data<int8_t>(), bits.data(), 3, cudaMemcpyHostToDevice);
  cols.push_back(std::move(b));
  cudf::table table(std::move(cols));
  flatbuffers::FlatBufferBuilder fbb;
  const fb::Expr* ref = finished_expr(fbb, make_col_ref(fbb, 1, "b"));
  auto mask = peacock::evaluate_column(ref, table.view());
  auto kept = cudf::apply_boolean_mask(table.view(), mask.view());
  EXPECT_EQ(host_int64_column(kept->view().column(0)), (std::vector<int64_t>{1, 3}));
}
```

  (`make_decimal_literal`'s argument order is the file's own, `:103`; read it before writing the
  literal. `int64_column`, `host_int64_column` and `allocated_by` are refcounted-scatter's.)
- [ ] **Step 2: Device cycle:** the first and third do not compile (no `evaluate_column`); with
  a stub returning `{build_column(...), {}}` the first and second are red.
- [ ] **Step 3: The evaluator.** In `expr.cpp`, rename today's `build_column` (`:801`) to
  `static std::unique_ptr<cudf::column> make_column(...)`, delete its `ColumnRef` arm
  (`:822-839`), and add after it:

```cpp
EvaluatedColumn evaluate_column(const fb::Expr* expr, cudf::table_view const& table) {
  if (expr->node_type() == fb::ExprNode_ColumnRef) {
    auto idx = static_cast<cudf::size_type>(expr->node_as_ColumnRef()->index());
    if (idx < 0 || idx >= table.num_columns())
      throw std::runtime_error("ColumnRef index " + std::to_string(idx) +
                               " out of range (cols=" + std::to_string(table.num_columns()) + ")");
    return {nullptr, table.column(idx)};
  }
  return {make_column(expr, table), {}};
}

std::unique_ptr<cudf::column> build_column(const fb::Expr* expr, cudf::table_view const& table) {
  return evaluate_column(expr, table).take();
}
```

  The forward declaration at `:505` becomes `EvaluatedColumn evaluate_column(...)` beside
  `build_column`'s.
- [ ] **Step 4: Convert the view-only callers.** The rule: a variable whose every later use is
  `->view()`, `->type()`, `->size()` or `->null_count()` becomes `auto x = evaluate_column(e, t);`
  with `x->view()` → `x.view()` and `x->type()` → `x.view().type()`; a variable that is returned,
  moved or reassigned keeps `build_column`. Applied:
  - `expr.cpp` `build_column_binary` (`:571-610`): all six `lcol`/`rcol`. Example, the
    both-columns tail:

```cpp
  auto lcol = evaluate_column(lhs, table);
  auto rcol = evaluate_column(rhs, table);
  auto out = binop_output_type(bin->op(), lcol.view().type(), rcol.view().type());
  return cudf::binary_operation(lcol.view(), rcol.view(), op, out);
```

  - `build_column_scalar_fn`: `date_part`'s `ts` (`:640`), `substr`'s `strcol` (`:648`), `abs`
    (`:680`), `lower` (`:710`), `upper` (`:718`), `concat`'s args (`owned` becomes
    `std::vector<EvaluatedColumn>`, `views.push_back(owned.back().view())`), `coalesce`'s loop
    `col` (`:744`; its `result` keeps `build_column`). `round` (`:691`):

```cpp
    auto col = evaluate_column(args->Get(0), table);
    ...
    std::unique_ptr<cudf::column> widened;
    if (col.view().type().id() != cudf::type_id::FLOAT64)
      widened = cudf::cast(col.view(), cudf::data_type{cudf::type_id::FLOAT64});
    return cudf::round(widened ? widened->view() : col.view(), places, cudf::rounding_method::HALF_UP);
```

  - `build_column_case`: `last_then` (`:773`), `cond` and `then` (`:780-781`); `result` keeps
    `build_column`.
  - `make_column`'s arms: unary `arg` (`:854`), LIKE `strcol` (`:872`); the cast's `inner`
    (`:897`) converts, and its string-to-string no-op returns `std::move(inner).take()` (a copy
    only for a bare `ColumnRef`, as today).
  - `filter.cpp:27`: `auto mask = evaluate_column(...)`, used as `mask.view()`; the AST branch
    keeps its owned column: hold both as `EvaluatedColumn mask = cudf_ast_can_evaluate(...) ?
    EvaluatedColumn{cudf::compute_column(...), {}} : evaluate_column(...);`.
  - `sort.cpp:30-42`: `owned_keys` becomes `std::vector<EvaluatedColumn>`; the `ColumnRef`
    special case there can go, since `evaluate_column` borrows it now.
  - `aggregate.cpp:203,452`: `computed_args` becomes `std::vector<EvaluatedColumn>` (its
    `reserve` stays — views into its elements must not move); `:354` `null_placeholders` likewise.
  - `window.cpp:58-74`: `key_owned` → `std::vector<EvaluatedColumn>`; `arg_owned` stays a
    `unique_ptr` for the decimal cast and the argument is `EvaluatedColumn arg = evaluate_column(...)`
    with `arg_view = arg.view()`.
- [ ] **Step 5: Device cycle:** the three green; the full gpu tier and `peacock_plan_tests`
  unchanged.
- [ ] **Step 6: Commit.** `git commit -m "a bare ColumnRef is borrowed, not copied (#154)"`.

### Task 2: The filter's projection shares its columns

**Files:**
- Modify: `cpp/src/operators/filter.cpp:31-48`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Consumes: `TableResult::owning`, `select` (refcounted-scatter Task 1).

- [ ] **Step 1: The cases.**

```cpp
/// nation filtered on n_regionkey > 2, with `projection` (empty for none).
static std::vector<uint8_t> nation_filter_plan(flatbuffers::FlatBufferBuilder& fbb,
                                               std::vector<uint32_t> projection) {
  auto pred = make_binary_expr(fbb, make_cast_expr(fbb, make_col_ref(fbb, 2, "n_regionkey"),
                                                   fb::DataType_Int64),
                               fb::BinaryOp_Gt, make_int64_literal(fbb, 2));
  auto scan = nation_scan_node(fbb);
  auto proj = projection.empty() ? flatbuffers::Offset<flatbuffers::Vector<uint32_t>>{}
                                 : fbb.CreateVector(projection);
  auto filter = fb::CreateCudfFilter(fbb, pred, scan, proj);
  return finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfFilter, filter.Union()));
}

static Allocated filter_call(std::vector<uint32_t> projection, peacock::TableResult* out_table) {
  flatbuffers::FlatBufferBuilder fbb;
  auto buf = nation_filter_plan(fbb, std::move(projection));
  peacock::NodeSession session(buf.data(), buf.size());
  uint64_t counts[1] = {1}, in = 0, out = 0;
  size_t produced = 0;
  peacock::NodeStats st{};
  session.execute_node(0, nullptr, nullptr, 0, &in, 1, &produced, &st);
  auto got = allocated_by([&] { session.execute_node(1, &in, counts, 1, &out, 1, &produced, nullptr); });
  if (out_table) *out_table = session.table_for(out);
  return got;
}

TEST(ExitCopies, AFiltersProjectionAllocatesNothingOfItsOwn) {
  auto whole = filter_call({}, nullptr);
  auto narrowed = filter_call({1, 0}, nullptr);
  EXPECT_LE(narrowed.total, whole.total) << "projecting kept columns must not copy them";
}

TEST(ExitCopies, AFiltersRepeatedOrdinalIsTwoColumnsOverOneOwner) {
  peacock::TableResult got;
  filter_call({1, 1}, &got);
  ASSERT_EQ(got.num_columns(), 2);
  EXPECT_EQ(got.owners[0], got.owners[1]);
  EXPECT_EQ(get_string_value(got.columns[0], 0), get_string_value(got.columns[1], 0));
}
```

- [ ] **Step 2: Device cycle:** the first red (the projection copies), the second green or red by
  chance — it must be green after.
- [ ] **Step 3: The fix.**

```cpp
  auto filtered = cudf::apply_boolean_mask(input.view(), mask.view());
  auto result = TableResult::owning(std::move(filtered), std::move(input.column_names));
  // The planner's fused projection, as a selection over the filtered table: kept columns are
  // shared, dropped ones freed with it, and a repeated ordinal is two entries over one owner.
  if (filter->projection() && filter->projection()->size() > 0) {
    std::vector<cudf::size_type> ordinals(filter->projection()->begin(), filter->projection()->end());
    return result.select(ordinals);
  }
  return result;
```

- [ ] **Step 4: Device cycle:** both green; `PlanExecutor.FilterNation` unchanged.
- [ ] **Step 5: Commit.** `git commit -m "a filter's projection selects, it does not copy (#154)"`.

### Task 3: Project and window pass input columns through

**Files:**
- Modify: `cpp/src/operators/project.cpp:33-63`, `window.cpp:37-116`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: The cases.**

```cpp
TEST(ExitCopies, AProjectOfColumnRefsAllocatesNothing) {
  flatbuffers::FlatBufferBuilder fbb;
  auto exprs = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{
      make_col_ref(fbb, 1), make_col_ref(fbb, 0), make_col_ref(fbb, 1)});
  auto aliases = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{
      fbb.CreateString("name"), fbb.CreateString("key"), fbb.CreateString("name_again")});
  auto proj = fb::CreateCudfProject(fbb, exprs, aliases, nation_scan_node(fbb));
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfProject, proj.Union()));
  peacock::NodeSession session(buf.data(), buf.size());
  uint64_t counts[1] = {1}, in = 0, out = 0;
  size_t produced = 0;
  session.execute_node(0, nullptr, nullptr, 0, &in, 1, &produced, nullptr);
  auto got = allocated_by([&] { session.execute_node(1, &in, counts, 1, &out, 1, &produced, nullptr); });
  EXPECT_EQ(got.total, 0);
  const auto& t = session.table_for(out);
  ASSERT_EQ(t.num_columns(), 3);
  EXPECT_EQ(t.owners[0], t.owners[2]) << "q84's shape: one column projected twice, one owner";
  EXPECT_EQ(t.column_names, (std::vector<std::string>{"name", "key", "name_again"}));
}

TEST(ExitCopies, AWindowPassesItsInputThroughUncopied) {
  // count(*) OVER (PARTITION BY n_regionkey): one INT32/64 column computed, four passed through.
  flatbuffers::FlatBufferBuilder fbb;
  auto func = fbb.CreateString("count");
  auto parts = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 2)});
  auto alias = fbb.CreateString("n");
  fb::WindowExprNodeBuilder wb(fbb);
  wb.add_func_name(func);
  wb.add_partition_by(parts);
  wb.add_frame_end(fb::WindowFrameBound_UnboundedFollowing);
  wb.add_alias(alias);
  auto we = wb.Finish();
  auto win = fb::CreateCudfWindow(
      fbb, fbb.CreateVector(std::vector<flatbuffers::Offset<fb::WindowExprNode>>{we}),
      nation_scan_node(fbb));
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfWindow, win.Union()));
  peacock::NodeSession session(buf.data(), buf.size());
  uint64_t counts[1] = {1}, in = 0, out = 0;
  size_t produced = 0;
  auto scan = allocated_by([&] { session.execute_node(0, nullptr, nullptr, 0, &in, 1, &produced, nullptr); });
  auto got = allocated_by([&] { session.execute_node(1, &in, counts, 1, &out, 1, &produced, nullptr); });
  EXPECT_LT(got.total, scan.net) << "the window allocated more than its result column";
  const auto& t = session.table_for(out);
  EXPECT_EQ(t.num_columns(), 5);
  EXPECT_EQ(t.column_names.back(), "n");
}
```

  (The `WindowExprNode` fields are `gpu_plan.fbs:541-565`; if `alias` is named differently there,
  use that name. nation's 25 rows are a small input: `scan.net` is its bytes, and the window
  result is 25 integers, so the bound holds by a wide margin after and fails before.)
- [ ] **Step 2: Device cycle:** both red.
- [ ] **Step 3: Project.** The `ColumnRef` arm shares the input's owner; computed columns are
  owned:

```cpp
  auto tv = input.view();
  TableResult out;
  for (flatbuffers::uoffset_t i = 0; i < proj->exprs()->size(); ++i) {
    auto* expr = proj->exprs()->Get(i);
    if (expr->node_type() == fb::ExprNode_ColumnRef) {
      // The input's own column, shared: no copy, and a column projected twice is one owner.
      auto idx = static_cast<cudf::size_type>(expr->node_as_ColumnRef()->index());
      out.owners.push_back(input.owners.at(idx));
      out.columns.push_back(input.columns.at(idx));
    } else {
      std::unique_ptr<cudf::column> made;
      if (cudf_ast_can_evaluate(expr, tv)) {
        ExprContext ctx;
        made = cudf::compute_column(tv, build_expr(expr, ctx));
      } else {
        made = build_column(expr, tv);
      }
      std::shared_ptr<cudf::column const> owner{std::move(made)};
      out.columns.push_back(owner->view());
      out.owners.push_back(std::move(owner));
    }
    out.column_names.push_back(proj->aliases() && i < proj->aliases()->size()
                                   ? proj->aliases()->Get(i)->str()
                                   : "col" + std::to_string(i));
  }
  return out;
```

  The empty-projection placeholder arm (`:20-31`) stays as it is (join-backend replaces it).
- [ ] **Step 4: Window.** `TableResult out = input;` (shares every owner) replaces the copy loop
  `:43-48`; each window column is appended as `out.owners.push_back(...)`,
  `out.columns.push_back(owner->view())`, `out.column_names.push_back(...)`; `return out;`.
  `tv` stays `input.view()` for the evaluation.
- [ ] **Step 5: Device cycle:** both green; `PlanExecutor.ProjectRename`, the Sqrt cases and the
  harness `exec_cases` unchanged.
- [ ] **Step 6: Commit.** `git commit -m "project and window share their input's columns (#154)"`.

### Task 4: The aggregate releases groupby's keys

**Files:**
- Modify: `cpp/src/operators/aggregate.cpp:411-415`, `:758-761`; comments at `:642-645`,
  `:678-681`, `:771`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: The case.** A group-by of customer on `c_custkey` (every key distinct, so the
  output's key column is as large as the input's) with one count, against cuDF's own groupby of
  the same input as the baseline:

```cpp
TEST(ExitCopies, AnAggregateHandsGroupbysKeysOverUncopied) {
  flatbuffers::FlatBufferBuilder fbb;
  auto path = fbb.CreateString(parquet_path("customer"));
  auto paths = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{path});
  auto schema = make_schema(fbb, {{"c_custkey", fb::DataType_Int64}});
  auto scan = fb::CreateCudfScan(fbb, paths, schema, fbb.CreateVector(std::vector<uint32_t>{0}));
  auto scan_node = make_plan_node(fbb, fb::PlanNodeKind_CudfScan, scan.Union());
  auto groups = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 0, "c_custkey")});
  auto group_names = fbb.CreateVector(std::vector<flatbuffers::Offset<flatbuffers::String>>{fbb.CreateString("c_custkey")});
  auto args = fbb.CreateVector(std::vector<flatbuffers::Offset<fb::Expr>>{make_col_ref(fbb, 0, "c_custkey")});
  auto f = fb::CreateAggregateFuncNode(fbb, fbb.CreateString("count"), args, false, fbb.CreateString("n"));
  fb::CudfAggregateBuilder ab(fbb);
  ab.add_mode(fb::AggregateMode_Partial);
  ab.add_group_exprs(groups);
  ab.add_group_names(group_names);
  ab.add_aggr_funcs(fbb.CreateVector(std::vector<flatbuffers::Offset<fb::AggregateFuncNode>>{f}));
  ab.add_input(scan_node);
  auto agg = ab.Finish();
  auto buf = finish_plan(fbb, make_plan_node(fbb, fb::PlanNodeKind_CudfAggregate, agg.Union()));
  peacock::NodeSession session(buf.data(), buf.size());
  std::vector<uint32_t> g{0};
  uint64_t in = session.execute_scan_rowgroups(0, g, nullptr);
  auto input = session.table_for(in);  // shares the owners, so the view outlives the call
  const int64_t key_bytes = int64_t{input.num_rows()} * 8;
  auto baseline = allocated_by([&] {
    cudf::groupby::groupby gb(cudf::table_view{{input.columns[0]}});
    std::vector<cudf::groupby::aggregation_request> reqs(1);
    reqs[0].values = input.columns[0];
    reqs[0].aggregations.push_back(cudf::make_count_aggregation<cudf::groupby_aggregation>());
    auto [keys, results] = gb.aggregate(reqs);
    auto widened = cudf::cast(results[0].results[0]->view(), cudf::data_type{cudf::type_id::INT64});
  });
  uint64_t counts[1] = {1}, out = 0;
  size_t produced = 0;
  auto got = allocated_by([&] { session.execute_node(1, &in, counts, 1, &out, 1, &produced, nullptr); });
  EXPECT_LT(got.total, baseline.total + key_bytes / 2)
      << "the operator allocated about one more key column than groupby itself";
}
```

  (`#include <cudf/groupby.hpp>`, `<cudf/aggregation.hpp>`, `<cudf/unary.hpp>` at the top of the
  file if absent. The cast mirrors the operator's count widening, so the baseline is groupby
  plus what the operator must add.)
- [ ] **Step 2: Device cycle:** red, by about one key column.
- [ ] **Step 3: The fix.** `:758-761`:

```cpp
  // Groupby's key table is fresh and ours: hand its columns over rather than copy them.
  auto key_columns = group_keys->release();
  for (size_t i = 0; i < key_columns.size(); ++i) {
    out_cols.push_back(std::move(key_columns[i]));
    out_names.push_back(key_names[i]);
  }
```

  and the grouping-sets loop `:411-415`, reading the row count before the release:

```cpp
      auto const set_rows = gk->num_rows();
      for (auto& key : gk->release()) cols.push_back(std::move(key));
      cudf::numeric_scalar<int32_t> gid_s(gid, true);
      cols.push_back(cudf::make_column_from_scalar(gid_s, set_rows));
```

  The five kept copies get their reason in place: `:642-645` and `:678-681` — "MERGE_M2 takes a
  struct, and a struct owns its children; these are the input's columns, so they are copied";
  `:771` — "the merged struct is read by up to three builds, one child each; releasing it for one
  would strand the others, and a per-group state column is small".
- [ ] **Step 4: Device cycle:** green; `AggregateMerge.*`, `PlanExecutor.AggregateGroupBy` and
  `AggregateCount` unchanged; the harness `aggregate_cases` unchanged.
- [ ] **Step 5: Commit.** `git commit -m "the aggregate hands groupby's keys over (#154)"`.

### Task 5: The tiers, the benchmark, the wiki

- [ ] **Step 1: Device cycle, the full gpu tier and the corpus device cells on today.** Every
  answer unchanged; tpcds q84 and q85 at tp1-single named in the record (Review focus 1).
- [ ] **Step 2: The benchmark.** The sf40 q6 and q19 numbers before Task 1 and after Task 4
  (`build-test-shadgpu.sh`'s benchmark run on shad-gpu), into the detail file; q19's lineitem
  filter is where the `ColumnRef` copy cost most (#154's HBM reading, ~46 of 107 GB).
- [ ] **Step 3: The wiki.** `tickets/corpus-coverage.md`'s #154 reworded to `join.cpp`'s ten
  sites alone (join-session-cpp's), with this task's outcome per site; `build-test.md`:
  `peacock_plan_tests` count (+8) and the row text.
- [ ] **Step 4: Commit.** `git commit -m "#154 narrowed to join.cpp: the other exit copies are gone"`.

### Task 6: The record

- [ ] Detail file: each case red then green; the per-site table (11 sites: 6 gone — expr.cpp's
  `ColumnRef`, filter, project, window, aggregate 413 and 759 — and 5 kept with their reasons);
  the benchmark. `git commit -m "exit-copies: the record"`.
