# Device schema harness implementation plan

**Goal:** A handle's schema readable from Rust as cuDF's `{type_id, scale}`, a comparator that
projects the plan's arrow schema the same way, a new schema case per node kind in every harness
family, and seven hand-written spot-checks inside the recipe walk — every existing case
untouched.

**Architecture:** One read-only ABI function returns the IPC schema message; `test_support::
device_schema` decodes it into `DeviceSchema` and projects `DataType` onto `DeviceType`;
`Device::schema_of(handle)` is the harness's reader. New cases live in `*_schema_cases.rs`
files beside each family; the walk gains an `on_call` hook and named spot-check tests. A red
case is a `bug_` with a ticket, never a fix.

**Tech stack:** C++ (`gpu_executor.cpp`, one function), Rust FFI, `test_support`, the gpu-rung
harness on `shad-gpu`.

**Spec:** [`device-schema-harness.md`](device-schema-harness.md) — frozen.

## Global constraints

- **No existing case changes.** Not an assertion added, not a `bug_` retyped. `git diff` on the
  existing `*_cases.rs` files shows only `mod` lines and shared-builder visibility.
- The projection is exactly `arrow_utilities.cpp`'s: a type it does not map panics naming it.
  Precision, timezone and nullability are not compared.
- No fix. A red case gets a ticket (fifteen lines at most) and becomes a `bug_` asserting the
  divergence string.
- Names say node and type: `a_<node>_over_<type>_declares_the_state_the_device_holds`.
- `rustfmt`; commits at most 10 lines; device cycles foreground, one per family group.

## File structure

| file | responsibility |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | `peacock_handle_schema` |
| `peacockdb-ffi/src/lib.rs` | extern |
| `peacockdb-core/src/test_support/device_schema.rs` (new), `test_support/mod.rs` | `TypeId`, `DeviceType`, `DeviceSchema`, `device_type_of`, `device_schema_of`, `schema_divergence`, `from_ipc_schema`; unit tests |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `Device::schema_of(&GpuBatch) -> DeviceSchema` |
| `peacockdb-core/src/tests/gpu_tests/{source,exec,accumulate,emit,join,nested,aggregate}_schema_cases.rs` (new), `gpu_tests/mod.rs` | the suite |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | `Walk::on_call`; `Session::schema_of`; the spot-checks |

---

### Task 1: The ABI function

**Files:**
- Modify: `cpp/include/peacock_gpu.h` (after `peacock_result_from_handle`), `cpp/src/gpu_executor.cpp` (after `:292`'s function)
- Modify: `peacockdb-ffi/src/lib.rs`
- Test: `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Produces: `int peacock_handle_schema(peacock_executor_t*, uint64_t handle, uint8_t** out_ipc, uint64_t* out_len)` — the Arrow IPC stream containing the schema message only; freed with `peacock_result_free`; 0 on success.

- [ ] **Step 1: Failing gtest.** `TEST(HandleSchema, ReadsTheTablesTypesWithoutRows)`: a scan
  of `nation` registered as a handle; call the function; decode with `arrow::ipc::RecordBatchStreamReader`;
  assert the schema has four fields, `n_nationkey` is `int64`, `n_name` is `utf8`, and
  `ReadNext` yields no batch. `TEST(HandleSchema, UnknownHandleFails)` with handle 999.
- [ ] **Step 2: Red** — undeclared function.
- [ ] **Step 3: Implement.** `gpu_executor.cpp`: look up `table_for(handle)` as the export does;
  `col_meta` from `column_names` (no precision — intermediates declare none, and the reduced
  schema drops it anyway); `cudf::to_arrow_schema` → `arrow::ImportSchema` →
  `MakeStreamWriter` + `Close()` with no batch written → buffer to `malloc`. Same error
  handling as `peacock_result_from_handle`. Header doc: "the schema alone; for reading what the
  device holds at a handle without moving rows."
- [ ] **Step 4: Extern** in `ffi/src/lib.rs` beside the export's.
- [ ] **Step 5:** Build; the two gtest cases green on the device.
- [ ] **Step 6: Commit.** `git commit -m "peacock_handle_schema: a handle's schema without its rows"`.

### Task 2: The reduced schema

**Files:**
- Create: `peacockdb-core/src/test_support/device_schema.rs`
- Modify: `peacockdb-core/src/test_support/mod.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeId { Empty, Bool8, Int8, Int16, Int32, Int64, UInt8, UInt16, UInt32, UInt64,
                  Float32, Float64, TimestampDays, TimestampSeconds, TimestampMilliseconds,
                  TimestampMicroseconds, TimestampNanoseconds, DurationSeconds, /*…*/ String,
                  Decimal128, List, Struct }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceType { pub id: TypeId, pub scale: Option<i32> }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSchema(pub Vec<(String, DeviceType)>);
pub fn device_type_of(arrow: &DataType) -> DeviceType;          // panics on an unmapped type
pub fn device_schema_of(declared: &Schema) -> DeviceSchema;
pub fn from_ipc_schema(schema: &Schema) -> DeviceSchema;          // the export's schema → reduced
pub fn schema_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String>;
```

- [ ] **Step 1: Unit tests** in the same file (`#[cfg(test)] mod tests`):
  `utf8_and_large_utf8_are_one_string` (`Utf8`, `LargeUtf8` → `String`, no scale);
  `a_decimal_keeps_its_scale_and_loses_its_precision` (`Decimal128(15,2)` and `(38,2)` →
  `{Decimal128, Some(2)}`, equal); `date32_is_timestamp_days`; `timestamps_keep_the_unit_and_lose_the_zone`;
  `integers_and_floats_by_width`; `boolean_is_bool8`; `null_is_empty`;
  `an_unmapped_type_panics_by_name` (`Interval`, `#[should_panic(expected = "Interval")]`);
  `divergence_names_every_differing_column_in_the_sinks_spelling` — declared `[a Int32, b
  Utf8, c Decimal128(15,2)]` vs actual `[a Int16, b String, c Decimal128 s=4]` →
  `Some("0 a: Int32 vs INT16; 2 c: Decimal128(15, 2) vs DECIMAL128 scale 4")`;
  `a_matching_schema_diverges_nowhere`; `a_column_count_mismatch_is_a_divergence`.
- [ ] **Step 2: Red** — module missing.
- [ ] **Step 3: Implement** — `device_type_of` as one `match` mirroring
  `third_party/cudf/cpp/src/interop/arrow_utilities.cpp:30-70` in the same order, with a
  comment pointing there; `from_ipc_schema` = `device_type_of` over the decoded fields (the
  export's `utf8` is `String`, its `decimal128(_, s)` keeps `s`); `schema_divergence` compares
  length, then position by position name and `DeviceType`, formatting `"{at} {name}: {declared
  arrow type} vs {actual}"` joined by `"; "`, `None` when equal. `Display` for `DeviceType`
  prints `INT16`, `STRING`, `DECIMAL128 scale 2`, `TIMESTAMP_DAYS`.
- [ ] **Step 4: Green**, rust-only `--lib -- test_support::device_schema`; `test_module_layout`.
- [ ] **Step 5: Commit.** `git commit -m "test_support::device_schema: an arrow schema projected onto what cuDF stores"`.

### Task 3: `Device::schema_of`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/device.rs` (after `fetch`), `wire/gpu_tests/mod.rs` `Session` (after `export`, `:163`)

- [ ] **Step 1:** `pub(crate) fn schema_of(&self, batch: &GpuBatch) -> DeviceSchema` —
  `peacock_handle_schema` on `batch.handle()`, `StreamReader` over the buffer for its
  `schema()`, `from_ipc_schema`, `peacock_result_free`. Does not consume the batch. The walk's
  `Session::schema_of(handle: u64)` is the same over a raw handle.
- [ ] **Step 2: First device case**, in a new `source_schema_cases.rs`: a `Given` leaf of every
  fixture column type uploaded, `schema_of(upload)` compared with `device_schema_of(&schema)`
  — `schema_divergence == None`. This proves the reader against the uploader.
- [ ] **Step 3:** Device cycle `PCK_TEST_FILTER='schema_cases'`: green.
- [ ] **Step 4: Commit.** `git commit -m "Device::schema_of reads what the device holds at a handle"`.

### Task 4: The suite, family by family

**Files:**
- Create: `exec_schema_cases.rs`, `accumulate_schema_cases.rs`, `emit_schema_cases.rs`, `join_schema_cases.rs`, `nested_schema_cases.rs`, `aggregate_schema_cases.rs`; extend `source_schema_cases.rs`
- Modify: `gpu_tests/mod.rs` (the `mod` lines); shared builders in the existing files become `pub(super)` where a schema case needs them (visibility only)

One shape for every case: run the node on the device alone (`script.rs`'s device-only runner —
`run_gpu` if it still exists, else the device half of `run_both` factored out as
`device_outputs(node, script) -> Vec<GpuBatch>`, a builder not a mechanism), then for every
output batch `assert_eq!(schema_divergence(node.kind().schema(), &device.schema_of(&batch)),
None)`. A case that fails is rewritten as `bug_…` asserting the divergence string, with its
ticket above.

- [ ] **Step 1: `exec`** — `GpuProject`: arithmetic on `Int32`, `Int64`, `Float64`,
  `Decimal128(15,2)` (`+`, `*`, `/` — the divide's declared scale); each `Cast` the corpus
  emits (`Int64→Decimal128`, `Decimal128→Float64`, `Int32→Int64`, `Utf8→…` if any); each
  scalar function in `expr.cpp`'s dispatch (`upper`, `lower`, `substr`, `concat`, `coalesce`,
  `round`, `date_part` each field, `sqrt`), `CASE`; `GpuFilter` with and without projection.
  Device cycle; commit `"exec schema cases: what a project and a filter hand the device up"`.
- [ ] **Step 2: `source`, `accumulate`, `emit`** — scan of each fixture table (every parquet
  column type); coalesce-all over two batches; emit on `Int64`, `Utf8`, `Date32`, composite
  keys. Cycle; commit.
- [ ] **Step 3: `join`, `nested`** — each join type × with/without projection × key types
  `Int64`, `Utf8`, `Date32`, composite (reuse `keyed`/`hash_join_keyed` from
  `join_dimension_cases.rs`, made `pub(super)`); the nested loop `Inner` with and without
  projection. Cycle; commit.
- [ ] **Step 4: `aggregate`** — init, merge, finalize for `sum`, `count`, `min`, `max`, `avg`,
  `stddev`; grouped on `Int32`, `Utf8`, `Date32`, two keys; global; the grouping-set state
  (`__grouping_id`: expect a `bug_` on #65 — write it as one from the start). Cycle; commit.
- [ ] **Step 5: `union`** — two `Given` branches whose same-named column differs in cuDF type
  (`Decimal128(15,2)` against an `Int64(0)` literal cast, as `union.cpp`'s deleted comment
  described) under the planner's per-branch casts; the union's output schema on the device.
  Fold into `exec_schema_cases.rs` or its own file per `test_module_layout`'s size rule.

### Task 5: The walk spot-checks

**Files:**
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs:233-300` (`Walk`, `make`), the test section

- [ ] **Step 1: The hook.** `Walk` gains `on_call: Option<&'a mut dyn FnMut(Seq, FbKind,
  &[u64], &Session)>`; `make` calls it after `self.session.execute(...)` with the returned
  handles. `assert_walk_matches_datafusion(sql, knobs)` gains a sibling
  `walk_with(sql, knobs, hook)` that installs it and still compares the final batches.
- [ ] **Step 2: The seven tests**, each named for what it reads, each a `walk_with` whose
  hook matches `(seq, kind)` on the call the spec's table names, reads
  `session.schema_of(handle)` and asserts the literal `DeviceSchema`:

```rust
#[tokio::test]
async fn an_avg_partial_holds_a_string_key_an_int64_count_and_a_scale_2_sum() {
    let seen = walk_with(AVG_BY_FLAG, ONE_LANE, |_, kind, handles, session| {
        if let FbKind::Aggregate { mode: Partial } = kind {
            assert_eq!(session.schema_of(handles[0]), DeviceSchema(vec![
                ("l_returnflag".into(), DeviceType { id: TypeId::String, scale: None }),
                ("avg(lineitem.l_quantity)$count".into(), DeviceType { id: TypeId::Int64, scale: None }),
                ("avg(lineitem.l_quantity)$sum".into(), DeviceType { id: TypeId::Decimal128, scale: Some(2) }),
            ]));
        }
    }).await;
    assert!(times(&seen, partial_aggregate_kind()) == 1);
}
```

  The state column names come from the plan golden for that query (`--- recipes ---` and the
  `schema=[…]` of the aggregate node); read them, do not invent them. The rest of the table:
  `AVG_BY_FLAG` merge and finalize; `SUM_BY_FLAG` partial; `ROLLUP`'s grouping-set aggregate
  as `bug_…_holds_an_int32_grouping_id_where_the_plan_says_uint8` naming #65;
  `PROJECT_OVER_FILTER` after filter and after project; `INNER_JOIN` after the join;
  `MAX_OF_SUMS` inner finalize and outer max; `SEMI_JOIN` after its one call. A check whose
  device answer differs from the table's expectation is a `bug_` with a ticket, and the detail
  file records the correction to the table.
- [ ] **Step 3:** Device cycle `PCK_TEST_FILTER='wire::gpu_tests'`: the existing nine green,
  the spot-checks green or pinned.
- [ ] **Step 4: Commit.** `git commit -m "walk spot-checks: what the device holds between calls, read by hand"`.

### Task 6: The record

- [ ] `build-test.md`: the suite's row per family and the walk's, counts; the `bug_` table
  gains every new pin with its ticket. Detail file: every device run's `test result:` lines,
  the projection table, every ticket opened.
- [ ] `git commit -m "device-schema-harness: the record"`.
