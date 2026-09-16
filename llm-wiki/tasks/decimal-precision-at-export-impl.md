# Decimal precision at export implementation plan

**Goal:** The export writes the precision the plan declared, refuses any fixed_point that is not
DECIMAL128, the plan refuses any decimal that is not `Decimal128`, and the wire and header move
once for the whole chain — `output_schema` and the view values gone, `peacock_handle_schema`
in — closing #187.

**Architecture:** `peacock_result_from_handle` gains a per-column precision array; `unload`
and `Device::fetch` fill it from the declared schema; `export_table_to_ipc` sets each declared
precision on the imported Arrow schema before the batch is imported against it — the one
mechanism both cuDF 25.02 (no `column_metadata::precision`) and 26.02 share. The widening arm
becomes two hard failures. No Rust-side conversion: `concat_batches` at the sink is unchanged
and starts passing. This is the chain's one wire/header rebuild: the view enum values and
their C++ arms, the dead `output_schema` fields, and `peacock_handle_schema` all land here.

**Tech stack:** C++ (`gpu_executor.cpp`, `expr.cpp`, `union.cpp`, gtest), the ABI header,
Rust FFI and the GPU backend, `gpu_plan.fbs`, `shad-gpu`.

**Spec:** [`decimal-precision-at-export.md`](decimal-precision-at-export.md) — frozen.

## Global constraints

- Precision alone crosses the ABI; scale stays the column's. No cast, no range check, no
  relabel after decode. `column_metadata::precision` is not used: 25.02 lacks it.
- Every non-DECIMAL128 fixed_point at export is an error naming the column; the only widening
  left in `cpp/src` is the loader's (`scan.cpp:98-108`).
- The `.fbs` and the header move once, here, for everything the chain needs of them; both
  sides rebuild together and staged binaries from before are stale.
- `rustfmt`; C++ formatted as its neighbours; commits at most 10 lines.
- Device cycles foreground, as in the previous plan.

## File structure

| file | responsibility |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `cpp/src/plan_executor_internal.h` | the signature; `export_table_to_ipc` with precisions and the two failures, exposed to tests; `peacock_handle_schema` |
| `peacockdb-ffi/src/lib.rs` | both externs |
| `peacockdb-core/src/common.rs`, `executor/gpu_backend/mod.rs` | `declared_precisions` (not feature-gated, so its unit test runs rust-only); `unload` passes the array |
| `peacockdb-core/src/tests/gpu_tests/device.rs`, `wire/gpu_tests/mod.rs` | `fetch` takes a declaration; the walk's `export` passes zeros |
| `peacockdb-core/src/plan/common.rs`, `validate.rs`, `validate/tests.rs` | `is_decimal128_only`, the rule, its test |
| `flatbuffers/gpu_plan.fbs`, `wire/writer.rs`, `cpp/src/operators/union.cpp`, `cpp/src/node_session.cpp`, `cpp/src/expr.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `output_schema` removed; `Utf8View`/`BinaryView` removed with their C++ arms and the gtests' 45 uses |
| `llm-wiki/architecture.md` | union row; the device-type sentence |

---

### Task 1: The ABI and the export

**Files:**
- Modify: `cpp/include/peacock_gpu.h:203`, `cpp/src/gpu_executor.cpp:48-80,292`
- Modify: `peacockdb-ffi/src/lib.rs:151`
- Test: `cpp/tests/gpu/test_plan_executor.cpp` (beside `:1262`, the existing export tests)

**Interfaces:**
- Produces: `int peacock_result_from_handle(peacock_executor_t*, uint64_t handle, uint64_t offset, uint64_t length, const int32_t* decimal_precisions, uint64_t n_columns, uint8_t** out_ipc, uint64_t* out_ipc_len)`. `decimal_precisions` may be null with `n_columns == 0` (no declarations); otherwise `n_columns` must equal the table's column count.

- [ ] **Step 1: The failing C++ tests.** In `test_plan_executor.cpp`, next to the export tests
  (`:1262`), over a planned handle — a scan of a decimal-bearing fixture through the file's
  plan runner (`tpch.minimal`'s `part` has `p_retailprice`, `supplier` has `s_acctbal`):

```cpp
TEST(Export, DeclaredPrecisionIsWrittenToTheSchema) {
  // a handle whose column 0 is DECIMAL128 scale -2 (a CudfProject keeping s_acctbal alone)
  const int32_t precisions[1] = {15};
  uint8_t* ipc = nullptr; uint64_t len = 0;
  ASSERT_EQ(peacock_result_from_handle(plan.get(), handle, 0, UINT64_MAX, precisions, 1, &ipc, &len), 0);
  auto schema = read_ipc_schema(ipc, len);          // arrow::ipc::RecordBatchStreamReader over the buffer
  auto& t = static_cast<const arrow::Decimal128Type&>(*schema->field(0)->type());
  EXPECT_EQ(t.precision(), 15); EXPECT_EQ(t.scale(), 2);
  peacock_result_free(ipc);
}
TEST(Export, NoDeclarationExportsAtMaximum) {
  // same handle, precisions nullptr, n_columns 0 → precision 38, scale 2
}
TEST(Export, APrecisionForANonDecimalColumnIsRefused) {
  // a handle whose column 0 is INT64 (s_suppkey), precisions {10}
  EXPECT_NE(peacock_result_from_handle(plan.get(), handle, 0, UINT64_MAX, precisions, 1, &ipc, &len), 0);
  EXPECT_NE(std::string(peacock_last_error(plan.get())).find("s_suppkey"), std::string::npos);
}
```

  And in `test_cudf_nodes.cpp`, which tests internals, the refusal no C-API path can reach —
  no handle can hold a DECIMAL64 (the scan widens, `adopt` is not exported):

```cpp
TEST(ExportInternal, ADecimal64ColumnIsRefusedByName) {
  cudf::test::fixed_point_column_wrapper<int64_t> amount({100, 250}, numeric::scale_type{-2});
  cudf::table_view tv{{amount}};
  uint8_t* out = nullptr; uint64_t len = 0;
  EXPECT_THROW(peacock::export_table_to_ipc(tv, {"amount"}, nullptr, &out, &len), std::runtime_error);
  try { peacock::export_table_to_ipc(tv, {"amount"}, nullptr, &out, &len); }
  catch (const std::runtime_error& e) { EXPECT_NE(std::string(e.what()).find("amount"), std::string::npos); }
}
```

- [ ] **Step 2: Compile red.** The signature does not exist: the tests fail to build.

- [ ] **Step 3: The header and the function.** `peacock_gpu.h:203`: the new signature, doc
  sentence: "`decimal_precisions[i]` is the declared precision of column i, 0 for none; scale
  is the column's." `gpu_executor.cpp`: `export_table_to_ipc(tview, names, precisions, out,
  len)`, no longer `static`, declared in `cpp/src/plan_executor_internal.h` in namespace
  `peacock` —

```cpp
  std::vector<cudf::column_metadata> col_meta;
  for (cudf::size_type i = 0; i < tview.num_columns(); ++i) {
    auto t = tview.column(i).type();
    bool fixed = t.id() == cudf::type_id::DECIMAL32 || t.id() == cudf::type_id::DECIMAL64
              || t.id() == cudf::type_id::DECIMAL128;
    if (fixed && t.id() != cudf::type_id::DECIMAL128)
      throw std::runtime_error("export: column " + names[i] + " is a narrow fixed_point; the loader widens every decimal to DECIMAL128 and nothing may narrow one");
    if (precisions && precisions[i] != 0 && !fixed)
      throw std::runtime_error("export: column " + names[i] + " is not a decimal but was given precision " + std::to_string(precisions[i]));
    col_meta.push_back({names[i]});
  }
  auto c_schema = cudf::to_arrow_schema(tview, col_meta);
  auto schema = arrow::ImportSchema(c_schema.get()).ValueOrDie();        // decimal128(38, s) today
  // The label the plan declared. cuDF stores no precision (25.02 cannot even be told one), so
  // the schema message is where it is written; the buffers below are untouched.
  for (int i = 0; i < schema->num_fields(); ++i) {
    if (!precisions || precisions[i] == 0) continue;
    auto scale = -tview.column(i).type().scale();
    schema = schema->SetField(i, schema->field(i)->WithType(arrow::decimal128(precisions[i], scale))).ValueOrDie();
  }
  auto c_array = cudf::to_arrow_host(tview);
  auto batch = arrow::ImportRecordBatch(&c_array->array, schema).ValueOrDie();
```

  The `widened`/`widened_views` vectors, the cast loop and the two comments above them go.
  `peacock_result_from_handle` checks `n_columns == 0 || n_columns == num_columns` (else
  `last_error`, return 1) and forwards `precisions`. (`WithType` on an imported decimal field
  changes the type only; `ImportRecordBatch` validates the buffers against it and accepts,
  since a decimal128's layout does not depend on precision.)

- [ ] **Step 4: The extern.** `peacockdb-ffi/src/lib.rs:151`: add `decimal_precisions: *const
  i32, n_columns: u64` before `out_ipc`; doc line as the header's.

- [ ] **Step 5: Build C++ and run the four tests on a device** (`scripts/build-test.sh --build`
  for the C++; the export cases through `build-test-shadgpu.sh --run` with
  `PCK_TEST_FILTER='Export'`). Green on 25.02; the 26.02 leg compiles in CI.

- [ ] **Step 6: Commit.**
```bash
git add cpp/include/peacock_gpu.h cpp/src/gpu_executor.cpp peacockdb-ffi/src/lib.rs cpp/tests
git commit -m "the export is told each decimal's precision and refuses a narrow fixed_point"
```

### Task 2: The Rust callers

**Files:**
- Modify: `peacockdb-core/src/executor/gpu_backend/mod.rs:207-250`, `tests/gpu_tests/device.rs:85-112`, `wire/gpu_tests/mod.rs:163-170`

**Interfaces:**
- Produces: `pub(crate) fn declared_precisions(schema: &Schema) -> Vec<i32>` in
  `peacockdb-core/src/common.rs` — not in `gpu_backend/`, which is compiled out under
  `rust-only` (`executor/mod.rs:22-24`) and would take its unit test with it.

- [ ] **Step 1:** `declared_precisions`: `schema.fields().iter().map(|f| match f.data_type() {
  DataType::Decimal128(p, _) => *p as i32, _ => 0 }).collect()`. Unit test beside it in
  `common.rs`'s test module: `[Decimal128(15,2), Int64, Decimal128(38,6)] → [15, 0, 38]`; runs
  rust-only.
- [ ] **Step 2:** `unload`: `let precisions = declared_precisions(&self.schema);` and pass
  `precisions.as_ptr(), precisions.len() as u64`. Nothing else in the function moves.
- [ ] **Step 3:** `Device::fetch(&self, batch, rows, declared: Option<&Schema>)`: build the
  array from `declared` or pass `(null, 0)`. Every caller in `tests/gpu_tests/` passes the
  schema it declares for the node's output (they hold it — `compare.rs` reads it); grep
  `\.fetch(` and update each. The walk's `Session::export` passes `(null, 0)`: intermediates
  declare nothing.
- [ ] **Step 4:** `cargo test --features rust-only -p peacockdb-core --lib` green (the FFI is
  compiled out; the unit test runs). `cargo test --features gpu … --no-run` through
  `scripts/cargo-cudf.sh` compiles.
- [ ] **Step 5: Commit.** `git commit -m "unload and the harness hand the export the declared precisions"`.

### Task 3: The plan rule

**Files:**
- Modify: `peacockdb-core/src/plan/common.rs` (`check_expr_types` from task 1 of chain B), `plan/validate.rs` (`no_view_types` → `device_holdable_types`)
- Test: `plan/validate/tests.rs`

- [ ] **Step 1: Failing test.** `a_decimal256_column_is_refused`: a leaf declaring
  `Decimal256(50, 2)`; `validate` errors naming the column and "Decimal128".
- [ ] **Step 2: The rule.** In `common.rs`, `is_view_type` gains a sibling
  `is_narrow_or_wide_decimal(t)`: `matches!(t, DataType::Decimal256(..))` — written so a
  future `Decimal32/64` variant is added to the same match. `check_expr_types` and the node
  rule (renamed `device_holdable_types`) refuse it with "only Decimal128 crosses to the device".
- [ ] **Step 3: Green.** `-- plan::validate::tests`.
- [ ] **Step 4: Commit.** `git commit -m "validation: every decimal in a plan is Decimal128"`.

### Task 4: `output_schema` leaves the wire

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs:520-526,604-610`, `peacockdb-core/src/wire/writer.rs:114,143`, `cpp/src/operators/union.cpp:30-50`, `cpp/tests/gpu/test_plan_executor.cpp:85-91`, `llm-wiki/architecture.md:829` and the "Every cast is explicit" section (`:350`)

- [ ] **Step 1:** Remove the two fields and their doc comments from the `.fbs`, and the
  `Utf8View` and `BinaryView` values from `enum DataType` (`:28-36`). Regenerate (rust-only
  build for Rust; cmake for C++). Fix what fails: the two `output_schema: None` args;
  `union.cpp`'s block (delete from `if (u->output_schema()` to its closing brace, keeping the
  single-input early return above it); `make_plan_node`'s `schema` parameter and its one
  forwarding; `expr.cpp`'s five view arms (`:89`, `:228`, `:333`, `:336`, `:478`);
  `test_plan_executor.cpp`'s 45 `DataType_Utf8View` → `DataType_Utf8` (`sed` on the file, then
  read the diff). `node_session.cpp:275`: the comment says "the node declares no schema of its
  own" instead of naming `output_schema`.
- [ ] **Step 2:** `architecture.md`: the `CudfUnion` row lists `inputs`, `interleave` only; in
  the union's paragraph add: "Branches are planned independently, so one column can land a
  different cuDF type per branch; the planner's per-branch cast projects are what align them
  before `cudf::concatenate`, which refuses mixed types." In "Every cast is explicit" add the
  sentence: "The device type is `{type_id, scale}`; precision, timezone and string flavour are
  labels the export is told, not facts the device holds."
- [ ] **Step 3:** `grep -rn output_schema cpp peacockdb-core flatbuffers` → nothing; `grep -rn
  "Utf8View\|BinaryView" cpp flatbuffers` → nothing. C++ builds; `ctest -L cpu` green;
  rust-only `--lib -- wire` green.
- [ ] **Step 4: Commit.** `git commit -m "the wire loses output_schema and the view types; one rebuild"`.

### Task 4b: `peacock_handle_schema`, for the harness two tasks on

**Files:**
- Modify: `cpp/include/peacock_gpu.h` (after `peacock_result_from_handle`), `cpp/src/gpu_executor.cpp`, `peacockdb-ffi/src/lib.rs`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

- [ ] **Step 1: Failing gtests.** `TEST(HandleSchema, ReadsTheTablesTypesWithoutRows)`: a scan
  of `nation` as a handle; call the function; `arrow::ipc::RecordBatchStreamReader` over the
  buffer; assert four fields, `n_nationkey` `int64`, `n_name` `utf8`, and `ReadNext` yields no
  batch. `TEST(HandleSchema, UnknownHandleFails)` with handle 999.
- [ ] **Step 2: Implement.** `int peacock_handle_schema(peacock_executor_t*, uint64_t handle,
  uint8_t** out_ipc, uint64_t* out_len)`: `table_for(handle)` as the export does; `col_meta`
  from `column_names`; `to_arrow_schema` → `ImportSchema` → `MakeStreamWriter` + `Close()`
  with no batch written → `malloc`'d buffer; the export's error handling. Header doc: "the
  schema alone; for reading what the device holds at a handle without moving rows." Extern
  in `ffi/src/lib.rs` beside the export's; nothing in Rust calls it yet.
- [ ] **Step 3:** Green on the device. Commit `"peacock_handle_schema: a handle's schema without its rows"`.

### Task 5: The device — harness, then the rollout

- [ ] **Step 1: Harness cycle.** `PCK_TEST_FILTER='_cases'`: every family green with
  `Device::fetch` now passing declarations; `aggregate_cases.rs:294`'s #187 pin and
  `source_cases.rs:168`, `exec_cases.rs:232` flip — retire them with the ticket.
- [ ] **Step 2: The rollout.** The survey's 53 decimal queries (§1) at `tp1_single`, as in the
  previous task's plan: enable where green, ticket where the values fail; `187` struck from a
  row only when every cell still disabled in it carries another ticket
  (`registry.rs:229-240`); the registry test green in both corpus binaries before the push.
- [ ] **Step 3:** Close #187 in `active-tickets.md`; `build-test.md` counts and the `bug_` table.
- [ ] **Step 4: Commit.** `git commit -m "#187 closed: the export says the precision the plan declared; N cells enabled"`.

### Task 6: The record

- [ ] Detail file: the ABI change, the four C++ cases' output, the harness lines, the rollout
  table. `git commit -m "decimal-precision-at-export: the record"`.
