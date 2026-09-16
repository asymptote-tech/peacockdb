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

- [ ] **Step 1: The case.** `synthetic::schema()` already carries `d: Date32` at index 6, so
  `input()` serves as the leaf — no builder needed:

```rust
// #191 — cuDF's extract_datetime_component answers INT16 for every field and the device hands
// it up as is, where the plan (and the cpu) declare Int32.
operator_case! {
    GpuProject,
    fn bug_a_date_part_year_comes_back_int16_from_the_device() {
        let year = Expr::ScalarFunction {
            name: "date_part".into(),
            args: vec![Expr::Literal(ScalarValue::Utf8(Some("YEAR".into()))), Expr::column(6, "d")],
            return_type: DataType::Int32,
            nullable: true,
        };
        let node = project(vec![keep_id(), (year, "y", DataType::Int32)]);
        let why = run_both(&node, Script::Exec(vec![input()])).gpu_refuses();
        assert!(why.contains("y: Int32 vs Int16"), "{why}");
    }
}
```

  `project` and `keep_id` are the file's own builders (`:160`, and the one
  `a_scalar_function_agrees` uses); nothing new is added.

- [ ] **Step 2: Device cycle**, `PCK_TEST_FILTER='exec_cases'`: the pin green (it asserts the
  refusal), everything else untouched.
- [ ] **Step 3: Commit.** `git commit -m "#191 pinned: date_part's year comes back Int16 from the device"`.

### Task 2: The C++ tests, red

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp` (after `ProjectSqrtThroughTheColumnPath`, `:718`)

- [ ] **Step 1:** `tpch.minimal` has no date column (customer, nation, part, region, supplier
  only), so the input is made: three cases sharing a helper `date_part_over_a_made_date(field)`
  — a scan of `nation`, a `CudfProject` whose first expression is `CastExprNode(n_nationkey →
  Date32)` (a day number becomes a date; the loader's `TIMESTAMP_DAYS`), and above it a second
  `CudfProject` with one `ScalarFunctionExprNode` — `name "date_part"`, args `[string literal
  field, col_ref 0]`, `return_type Int32` — executed through the file's plan runner. Assert
  `result.column(0).type().id() == cudf::type_id::INT32` and the first value (`1970` for
  `YEAR` over day 0; `1` for `MONTH` and `DAY`). Fields `YEAR`, `MONTH`, `DAY`. The wire's
  `return_type` is `gpu_plan.fbs:204`; the arm reads it off the node it already holds (`sf`,
  `expr.cpp:634-661`).
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

  (`sf` is the `ScalarFunctionExprNode` variable the arm already holds; the snippet's `fn`
  is that.)
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
  enable green cells, ticket value failures, then the other modes for what passed. `191` is
  struck from a row only when every cell still disabled in it carries another ticket
  (`registry.rs:229-240`); the registry test green before the push.
- [ ] Close #191 in `active-tickets.md`; `build-test.md` counts and `bug_` table.
- [ ] `git commit -m "#191 closed: q7, q8, q9 on the device"`.

### Task 6: The record

- [ ] Detail file: the gtest output, the harness lines, the neighbours table, the rollout.
  `git commit -m "date-part-return-type: the record"`.
