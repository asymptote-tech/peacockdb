# date_part return type implementation plan

**Goal:** `expr.cpp`'s `date_part` returns the type the wire's `return_type` names; three
plan-executor cases, one harness case (pin first, green after), `tpch/q7`, `q8`, `q9` rolled out
— closing #191.

**Architecture:** One `cudf::cast` at the end of one arm. The tests are written before it, red,
at the two levels below the corpus; the corpus rows are the third.

**Tech stack:** C++ and gtest on a device; one Rust harness case; the registry.

**Spec:** [`date-part-return-type.md`](date-part-return-type.md) — frozen.

## Global constraints

- `date_part` alone. Every other scalar arm is read and reported, not fixed.
- No Rust production change, no wire change, no golden moves.
- Pin first, fix second: the harness case is committed red-as-`bug_` before the C++ changes.
- Commits at most 10 lines; device cycles foreground.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/expr.cpp:641-661` | the cast |
| `cpp/tests/gpu/test_plan_executor.cpp` | `ProjectDatePartYearIsInt32`, `…Month…`, `…Day…` |
| `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` | the pin, then `a_date_part_answers_in_its_declared_type` |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | `q7`, `q9` gain `191`; three rows' cells |

---

### Task 1: The pin

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs` (beside `a_scalar_function_agrees`, `:355`)

- [ ] **Step 1: The case.** `exec_cases.rs`'s `input()` schema has no `Date32` column — add a
  `Given` leaf local to the case (`given::columns(&[("d", DataType::Date32)])` over four dates
  spanning two years, the file's `Given::of` shape), then:

```rust
// #191 — cuDF's extract_datetime_component answers INT16 for every field and the device hands
// it up as is, where the plan (and the cpu) declare Int32.
operator_case! {
    GpuProject,
    fn bug_a_date_part_year_comes_back_int16_from_the_device() {
        let year = Expr::ScalarFunction {
            name: "date_part".into(),
            args: vec![Expr::Literal(ScalarValue::Utf8(Some("YEAR".into()))), Expr::column(0, "d")],
            return_type: DataType::Int32,
            nullable: true,
        };
        let node = project_over(dates(), vec![(year, "y", DataType::Int32)]);
        let why = run_both(&node, Script::Exec(vec![dates_batch()])).gpu_refuses();
        assert!(why.contains("y: Int32 vs Int16"), "{why}");
    }
}
```

  Adapt names to the file's builders (`project` takes exprs over `input()`; give it a sibling
  `project_over(leaf, exprs)` if it lacks one — a builder, not a mechanism).

- [ ] **Step 2: Device cycle**, `PCK_TEST_FILTER='exec_cases'`: the pin green (it asserts the
  refusal), everything else untouched.
- [ ] **Step 3: Commit.** `git commit -m "#191 pinned: date_part's year comes back Int16 from the device"`.

### Task 2: The C++ tests, red

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp` (after `ProjectSqrtThroughTheColumnPath`, `:718`)

- [ ] **Step 1:** Three cases sharing a helper `date_part_over_region_dates(field)`: the file's
  scan of a parquet with a date column (`orders`' `o_orderdate` at `parquet_path("orders")` if
  `region` has none), a `CudfProject` with one `ScalarFunctionExprNode` — `name "date_part"`,
  args `[string literal field, col_ref]`, `return_type Int32` — executed through the file's
  plan runner; assert `result.column(0).type().id() == cudf::type_id::INT32` and the first
  value (`1996` for orders' first row, or whatever the file's fixture says — read it, do not
  guess). Fields `YEAR`, `MONTH`, `DAY`.
- [ ] **Step 2:** Build and run on the device: all three red on `INT16 != INT32`.
- [ ] **Step 3: Commit.** `git commit -m "date_part's declared type, asserted red at the plan executor"`.

### Task 3: The fix

**Files:**
- Modify: `cpp/src/expr.cpp:660`

- [ ] **Step 1:**

```cpp
    auto ts = build_column(args->Get(1), table);
    auto component = cudf::datetime::extract_datetime_component(ts->view(), comp);
    // cuDF answers INT16 for every field; the wire names the type DataFusion declared.
    auto want = cudf::data_type{fb_to_type_id(fn->return_type())};
    if (!cudf::is_integral(want))
      throw std::runtime_error("date_part: return_type " + std::to_string(fn->return_type()) + " is not an integer type");
    return component->type() == want ? std::move(component) : cudf::cast(component->view(), want);
```

  (`fn` is the `ScalarFunctionExprNode` the arm already holds; use its variable name.)
- [ ] **Step 2:** Build; the three gtest cases green; `PCK_TEST_FILTER='exec_cases'`: the pin
  is now red — rewrite it as the green case `a_date_part_answers_in_its_declared_type`
  (`.same(Order::AsEmitted)`, no `bug_`, no ticket line); rerun green.
- [ ] **Step 3: Commit.** `git commit -m "date_part answers in the type the wire names (#191)"`.

### Task 4: The neighbours, read

- [ ] For every arm of `expr.cpp`'s scalar dispatch (`substr`, `upper`/`lower`, `concat`,
  `coalesce`, `round`, … — list them from the file), write in the detail file: the arm, the
  cuDF type it returns, the `return_type` DataFusion declares for it (from a plan golden or
  the DataFusion function's `return_type`), verdict. A mismatch is a ticket in
  `active-tickets.md` naming a query or a harness case that would reach it; no fix.
- [ ] `git commit -m "the scalar arms read against their declared types; findings ticketed"`.

### Task 5: The rows

- [ ] `cost-registry.csv`: `tpch/q7` and `q9` gain `191` in `tickets` (the survey saw the class
  on them). `corpus_cases.inc`: the three rows' `gpu_modes` to `tp1_single`; device corpus run;
  enable green cells with `191` struck, ticket value failures, then the other modes for what
  passed.
- [ ] Close #191 in `active-tickets.md`; `build-test.md` counts and `bug_` table.
- [ ] `git commit -m "#191 closed: q7, q8, q9 on the device"`.

### Task 6: The record

- [ ] Detail file: the gtest output, the harness lines, the neighbours table, the rollout.
  `git commit -m "date-part-return-type: the record"`.
