# Operator harness implementation plan

**Goal:** One hand-built node, one script of batches the test wrote, both backends, the outputs
compared exactly — built here and proven on the three operators whose recipe carries no seq.

**Architecture:** A test-only ABI symbol adopts an Arrow batch into the device session's handle
registry, so a synthetic `RecordBatch` becomes a `GpuBatch` in one call. A stub leaf (`Given`)
declares a schema and a layout, and `attach_recipes` learns that a node outside the registry emits
no seq, so an operator over stub leaves gets its real recipe with no plan. One driver per executor
category, generic over `Backend`, runs the same call sequence on `CpuBackend` and `GpuBackend`;
`assert_same` compares the two slot by slot.

**Tech stack:** Rust, in-crate test modules at the rust and gpu rungs; ~40 lines of C++ behind one
new symbol. Device runs on `shad-gpu` through `scripts/build-test-shadgpu.sh`.

**Spec:** [`operator-harness.md`](operator-harness.md). Read "What it is" and "Scope" first; every
task below is one paragraph of the former, and a file outside the latter is a finding.

## Global constraints

- **Base:** the `visibility` branch — `test-layout` has already moved the tests in-crate. Every
  path below is the post-move one; the first step of Task 2 confirms them.
- **Synthetic data only.** No `testdata/`, no sf1, no dataset on the device host.
- **Exact comparison, slot by slot.** No tolerance, no flattening across calls or lanes.
- **No driver, no forwarder.** The harness calls executors through `executors_for` and never
  `run`.
- **No fix, anywhere.** A divergence is a ticket and a `bug_` test; the harness never casts,
  filters or special-cases one away, and no production behaviour changes.
- **Production code changes in two places only:** the additive test-only symbol (C++ + one
  extern) and one arm in `wire/attach.rs` with the `Writer` method it calls. Nothing else under
  `cpp/src/` or `peacockdb-core/src/` outside `src/tests/`.
- **Rung discipline:** `src/tests/{given,synthetic,compare}.rs` compile under `rust-only`;
  everything touching a device is under `src/tests/gpu_tests/` behind `feature = "gpu"`.
- **A one-sided failure is a `bug_` test with a ticket**, never a fix and never a workaround.
- No new dependencies. `inventory` is already a dev-dependency.
- Commit messages at most 10 lines. `rustfmt` on the leaves you touched, never the crate;
  `git clang-format` on the C++ lines you changed.

## File structure

| file | responsibility |
|---|---|
| `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp` | `NodeSession::adopt(TableResult) -> handle` |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | `peacock_handle_from_arrow`, test-only, documented as such |
| `peacockdb-ffi/src/lib.rs` | the extern, beside `peacock_spark_partition_ids` |
| `peacockdb-core/src/wire/attach.rs`, `wire/writer.rs` | `emit` over `try_as_node_ref`; `None` emits the writer's stub and no recipe |
| `peacockdb-core/src/tests/given.rs` | `Given` and `columns`, lifted from the cpu executor tests |
| `peacockdb-core/src/tests/synthetic.rs` | `synthetic(rows, seed)`, `decimals(rows, seed)`, `prefixed` |
| `peacockdb-core/src/tests/compare.rs` | `Order`, `Slot`, `same`, `assert_same`, and the red cases |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `Device`: open, upload, fetch, drop |
| `peacockdb-core/src/tests/gpu_tests/script.rs` | `Script`, `Outcome`, `run_both`, the generic drivers |
| `peacockdb-core/src/tests/gpu_tests/coverage.rs` | `Covers`, `operator_case!`, the kind guard |
| `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` | the round trip, `GpuUnload`, `GpuLimit` |
| `llm-wiki/build-test.md`, `llm-wiki/architecture.md` | one row; the ABI count |

---

### Task 1: The device adopts an Arrow batch

**Files:**
- Modify: `cpp/src/plan_executor.h` (the `NodeSession` class, after `slice_handle`)
- Modify: `cpp/src/node_session.cpp` (beside `slice_handle`'s definition)
- Modify: `cpp/include/peacock_gpu.h` (after the conformance hook)
- Modify: `cpp/src/gpu_executor.cpp` (after `peacock_spark_partition_ids`)
- Modify: `peacockdb-ffi/src/lib.rs` (inside the `unsafe extern "C"` block, after
  `peacock_spark_partition_ids`)

**Interfaces:**
- Produces: `int peacock_handle_from_arrow(peacock_executor_t*, const void* schema, const void*
  array, uint64_t* out_handle)` — returns 0 and a handle in the live session, or non-zero with the
  message in `peacock_last_error`. Task 4 is its only caller.

- [ ] **Step 1: Confirm the two facts the symbol rests on**

```bash
grep -n 'cudf::from_arrow' cpp/src/gpu_executor.cpp
grep -n 'next_handle++' cpp/src/node_session.cpp | head -3
grep -n 'std::unique_ptr<peacock::NodeSession> session' cpp/src/gpu_executor.cpp
```

Expect the import in `peacock_spark_partition_ids`, handles allocated inline as
`impl_->next_handle++` then `impl_->registry.emplace`, and the session owned by the executor struct.
If the registry has become a type of its own since, `adopt` goes on that type instead.

- [ ] **Step 2: `NodeSession::adopt`**

In `cpp/src/plan_executor.h`, after `slice_handle`:

```cpp
  /// Register a table the caller built and return its handle — the operator harness's
  /// upload, and nothing on the production path. Test-only by contract, not by build.
  uint64_t adopt(TableResult result);
```

In `cpp/src/node_session.cpp`, after `slice_handle`'s definition:

```cpp
uint64_t NodeSession::adopt(TableResult result) {
  uint64_t handle = impl_->next_handle++;
  impl_->registry.emplace(handle, std::move(result));
  return handle;
}
```

- [ ] **Step 3: The C entry point**

In `cpp/include/peacock_gpu.h`, after `peacock_spark_partition_ids`:

```cpp
// ---------------------------------------------------------------------------
// Test-only: adopt the Arrow C-Data struct array (`schema`/`array` as above; a
// struct array = one table) into the live session and return its handle, so a
// test can hand an executor a batch it wrote rather than one a scan read. Needs
// peacock_executor_begin_plan first: the handle registry is the session's. No
// recipe names this symbol and no production path calls it.
/// @return 0 on success; non-zero with the message in peacock_last_error.
int peacock_handle_from_arrow(peacock_executor_t* executor, const void* schema,
                              const void* array, uint64_t* out_handle);
```

In `cpp/src/gpu_executor.cpp`, after `peacock_spark_partition_ids`:

```cpp
int peacock_handle_from_arrow(peacock_executor_t* executor, const void* schema,
                              const void* array, uint64_t* out_handle) {
  if (!executor || !schema || !array || !out_handle) return 1;
  if (!executor->session) {
    executor->last_error = "no plan loaded (call peacock_executor_begin_plan first)";
    return 1;
  }
  try {
    auto const* c_schema = reinterpret_cast<const ArrowSchema*>(schema);
    auto table = cudf::from_arrow(c_schema, reinterpret_cast<const ArrowArray*>(array));
    std::vector<std::string> names;
    names.reserve(static_cast<size_t>(c_schema->n_children));
    for (int64_t i = 0; i < c_schema->n_children; ++i) {
      names.emplace_back(c_schema->children[i]->name);
    }
    *out_handle = executor->session->adopt(peacock::TableResult{std::move(table), std::move(names)});
    return 0;
  } catch (const std::exception& e) {
    // Nothing was consumed, so the session stays usable — unlike execute_node's reset.
    executor->last_error = e.what();
    return 1;
  }
}
```

If `TableResult` is not in namespace `peacock`, or the two `ArrowSchema`/`ArrowArray` names need an
include the file lacks, take them from how `peacock_spark_partition_ids` spells them. If, by the
time this builds, `TableResult` carries a per-column precision (a fix for #187 would add one),
fill it from the C schema's decimal format string (`d:18,2`) for each decimal
column — an adopted decimal that exports at 38 would be the harness inventing the defect it is
meant to find.

- [ ] **Step 4: The extern**

In `peacockdb-ffi/src/lib.rs`, inside the `unsafe extern "C"` block, directly after
`peacock_spark_partition_ids`:

```rust
        // Test-only: adopt an Arrow C-Data struct array (= one table) into the live
        // session and return its handle — the operator harness's upload. Needs
        // begin_plan first; no recipe names it and no production path calls it.
        pub fn peacock_handle_from_arrow(
            executor: *mut PeacockExecutor,
            schema: *const std::ffi::c_void,
            array: *const std::ffi::c_void,
            out_handle: *mut u64,
        ) -> i32;
```

- [ ] **Step 5: Build both halves and see the symbol exported**

```bash
timeout 3600 scripts/build-test-shadgpu.sh --build
nm -D "$(ls -d target-cudf-*/debug/build/peacockdb-ffi-*/out/lib | head -1)/libpeacock_gpu.so" | grep handle_from_arrow
```

Expected: one `T peacock_handle_from_arrow` line. `git clang-format` the C++ hunks.

- [ ] **Step 6: Commit**

```bash
git add cpp/src/plan_executor.h cpp/src/node_session.cpp cpp/include/peacock_gpu.h cpp/src/gpu_executor.cpp peacockdb-ffi/src/lib.rs
git commit -m "the session adopts a table a test built

peacock_handle_from_arrow: the murmur3 hook's Arrow import, registered as a
handle instead of hashed. Test-only, needs begin_plan, no recipe names it."
```

---

### Task 2: The rust-rung helpers: `Given`, `synthetic`, the comparator

**Files:**
- Create: `peacockdb-core/src/tests/given.rs`
- Create: `peacockdb-core/src/tests/synthetic.rs`
- Create: `peacockdb-core/src/tests/compare.rs`
- Modify: `peacockdb-core/src/tests/mod.rs` (declare the three)
- Modify: `peacockdb-core/src/executor/cpu_backend/tests/mod.rs` (its `Given` goes; the family's
  tests `use crate::tests::Given`), and wherever `test-layout` put `test_cpu_executors`' copy
- Leave: the `Given`s in `wire/tests.rs` and `plan/tests/mod.rs` unless they are the same
  three-method shape — a copy that differs is another task's refactor, not this one's

**Interfaces:**
- Produces: `Given::of(schema: Schema, batches: BatchLayout) -> Box<dyn GpuNode>`;
  `Given::with_layout(schema: Schema, layout: PartitionLayout) -> Box<dyn GpuNode>` (N lanes, a
  declared sort order — what a partition accumulator's child must say); `Given::of_columns(&[(&str,
  DataType)]) -> Box<dyn GpuNode>` (one lane, multiple batches, the cpu tests' old `of`);
  `columns(&[(&str, DataType)]) -> Schema`; `synthetic(rows: usize, seed: u64) -> RecordBatch`
  (eight columns, no decimal); `decimals(rows: usize, seed: u64) -> RecordBatch` (`id Int64`,
  `dec Decimal128(18, 2)`); `prefixed(&RecordBatch, &str) -> RecordBatch`; `type Slot = Vec<RecordBatch>`;
  `enum Order { Any, AsEmitted }`; `same(cpu: &[Slot], gpu: &[Slot], Order) -> Result<(), String>`;
  `assert_same(cpu: &[Slot], gpu: &[Slot], Order)`. Tasks 4–7 consume all of them.

- [ ] **Step 1: Confirm the layout `test-layout` left**

```bash
sed -n 1,40p peacockdb-core/src/tests/mod.rs
grep -rn 'struct Given' -A 1 peacockdb-core/src/
grep -n 'mod tests\|mod gpu_tests' peacockdb-core/src/lib.rs peacockdb-core/src/tests/mod.rs
```

Expect `src/tests/mod.rs` to exist with `injection`, `rebuild` and `join_fixture` beside it, and a
`Given` in `executor/cpu_backend/tests/mod.rs` with `of(columns)` and `of_schema(schema)`, plus
copies in `wire/tests.rs` and `plan/tests/mod.rs` and the one `test-layout` moved. If `src/tests/`
does not exist, stop: the base is wrong.

- [ ] **Step 2: Lift `Given` and `columns`**

`peacockdb-core/src/tests/given.rs` — the body is the one `executor/cpu_backend/tests/mod.rs`
carries today (`Given { kind: NodeKind::Intermediate { layout, schema } }`, `children()` empty,
`validate_schemas_and_partitions` `Ok`), `columns` from `test_cpu_executors` building nullable
fields, and three constructors:

```rust
/// A child that declares a schema and a layout and nothing else. Outside the node registry
/// on purpose: `attach_recipes` emits no seq for it, so an operator over `Given` leaves gets
/// its own recipe and the wire carries only the operator.
#[derive(Debug)]
pub(crate) struct Given {
    kind: NodeKind,
}

impl Given {
    /// One lane, the batch layout given.
    pub(crate) fn of(schema: Schema, batches: BatchLayout) -> Box<dyn GpuNode> {
        Self::with_layout(
            schema,
            PartitionLayout { batch_layout: batches, ..PartitionLayout::new(1) },
        )
    }

    /// One lane, several batches — the shape most cases start from.
    pub(crate) fn of_columns(fields: &[(&str, DataType)]) -> Box<dyn GpuNode> {
        Self::of(columns(fields), BatchLayout::MultipleBatches)
    }

    /// Whatever the operator above needs its child to have said: N lanes for a partition
    /// accumulator, a sort order for a sorted merge, a hash for a co-partitioned join.
    pub(crate) fn with_layout(schema: Schema, layout: PartitionLayout) -> Box<dyn GpuNode> {
        Box::new(Self { kind: NodeKind::Intermediate { layout, schema } })
    }
}
```

The cpu tests' `of(columns)` becomes `of_columns`, and their `of_schema(schema)` becomes
`of(schema, BatchLayout::MultipleBatches)` — a rename at each call site, nothing else.

Declare in `src/tests/mod.rs`:

```rust
mod given;
mod synthetic;
mod compare;
pub(crate) use given::{columns, Given};
pub(crate) use synthetic::{decimals, prefixed, synthetic};
pub(crate) use compare::{assert_same, same, Order, Slot};
```

Delete the copies in the cpu backend's test directory and point its `use` at `crate::tests`.

- [ ] **Step 3: Run the cpu executor tests to prove the lift changed nothing**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests
```

Expected: the same count green as before the lift (`git stash` to read it if unsure).

- [ ] **Step 4: Write `synthetic`'s failing tests**

In `peacockdb-core/src/tests/synthetic.rs`, under `#[cfg(test)] mod tests`... this whole file is
already test code, so plain `#[test]` at the bottom:

```rust
#[test]
fn a_synthetic_batch_is_the_same_batch_twice() {
    assert_eq!(synthetic(50, 7), synthetic(50, 7));
    assert_ne!(synthetic(50, 7), synthetic(50, 8));
}

#[test]
fn every_column_but_id_carries_a_null_and_id_carries_none() {
    let batch = synthetic(64, 1);
    for (index, field) in batch.schema().fields().iter().enumerate() {
        let nulls = batch.column(index).null_count();
        if field.name() == "id" {
            assert_eq!(nulls, 0, "id is the tie-breaker and is never null");
        } else {
            assert!(nulls > 0, "{} has no null in 64 rows", field.name());
        }
    }
}

#[test]
fn zero_rows_is_a_batch_with_the_schema_and_nothing_else() {
    let batch = synthetic(0, 1);
    assert_eq!(batch.num_rows(), 0);
    assert_eq!(batch.schema().fields().len(), 8);
}

#[test]
fn decimals_carry_an_id_and_a_decimal_with_nulls() {
    let batch = decimals(64, 1);
    assert_eq!(batch.schema().field(1).data_type(), &DataType::Decimal128(18, 2));
    assert_eq!(batch.column(0).null_count(), 0);
    assert!(batch.column(1).null_count() > 0);
}

#[test]
fn prefixing_renames_every_column_and_touches_no_value() {
    let batch = synthetic(5, 3);
    let renamed = prefixed(&batch, "r_");
    assert_eq!(renamed.schema().field(0).name(), "r_id");
    assert_eq!(renamed.columns(), batch.columns());
}
```

- [ ] **Step 5: Run them and watch them fail**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- tests::synthetic
```

Expected: `synthetic` not found.

- [ ] **Step 6: Write `synthetic`**

```rust
//! One batch every operator case starts from: eight typed columns, a null in each but the
//! id, values a reader can predict from the row number, and floats that are dyadic so a
//! sum of them is exact on both engines. Decimals are a second batch, because the device
//! exports every decimal at precision 38 (#187) and one column would make every case that
//! ticket's `bug_` test.

use std::sync::Arc;

use datafusion::arrow::array::{
    ArrayRef, BooleanArray, Date32Array, Decimal128Array, Float64Array, Int32Array, Int64Array,
    StringArray,
};
use datafusion::arrow::datatypes::{DataType, Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;

/// Column order is the ordinal every case's expressions use, so it is stated once.
pub(crate) fn schema() -> Arc<ArrowSchema> {
    Arc::new(ArrowSchema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("key", DataType::Int32, true),
        Field::new("i32", DataType::Int32, true),
        Field::new("i64", DataType::Int64, true),
        Field::new("f64", DataType::Float64, true),
        Field::new("s", DataType::Utf8, true),
        Field::new("d", DataType::Date32, true),
        Field::new("b", DataType::Boolean, true),
    ]))
}

/// splitmix64: a seed and a row number give one value, with no crate behind it.
fn mix(seed: u64, row: u64) -> u64 {
    let mut z = seed.wrapping_add(row.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// `rows` rows from `seed`. `id` is `0..rows` and never null; every other column is null
/// where `row % stride == stride - 1`, one stride per column, so no row is all null and no
/// two columns share their null rows. `key` takes seven values, so it has duplicates for a
/// group or a join and every lane of a 4-way scatter receives something.
pub(crate) fn synthetic(rows: usize, seed: u64) -> RecordBatch {
    let at = |row: usize| mix(seed, row as u64);
    let null_at = |row: usize, stride: usize| row % stride == stride - 1;
    let id: ArrayRef = Arc::new(Int64Array::from_iter_values((0..rows).map(|r| r as i64)));
    let key: ArrayRef = Arc::new(Int32Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 11)).then(|| (at(r) % 7) as i32)
    })));
    let i32s: ArrayRef = Arc::new(Int32Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 5)).then(|| (at(r) % 1000) as i32 - 500)
    })));
    let i64s: ArrayRef = Arc::new(Int64Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 7)).then(|| (at(r) % 1_000_000) as i64 - 500_000)
    })));
    let f64s: ArrayRef = Arc::new(Float64Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 13)).then(|| ((at(r) % 4096) as f64 - 2048.0) / 8.0)
    })));
    let words = ["alpha", "beta", "gamma", "delta", "", "epsilon"];
    let strings: ArrayRef = Arc::new(StringArray::from_iter((0..rows).map(|r| {
        (!null_at(r, 3)).then(|| words[(at(r) % words.len() as u64) as usize].to_string())
    })));
    let dates: ArrayRef = Arc::new(Date32Array::from_iter((0..rows).map(|r| {
        (!null_at(r, 19)).then(|| (at(r) % 20_000) as i32)
    })));
    let bools: ArrayRef = Arc::new(BooleanArray::from_iter((0..rows).map(|r| {
        (!null_at(r, 23)).then(|| at(r) % 2 == 0)
    })));
    RecordBatch::try_new(schema(), vec![id, key, i32s, i64s, f64s, strings, dates, bools])
        .expect("eight arrays of one length under the schema that declares them")
}

/// `[id, dec]` from the same generator, for the decimal cases alone.
pub(crate) fn decimals(rows: usize, seed: u64) -> RecordBatch {
    let at = |row: usize| mix(seed, row as u64);
    let id: ArrayRef = Arc::new(Int64Array::from_iter_values((0..rows).map(|r| r as i64)));
    let decs: ArrayRef = Arc::new(
        Decimal128Array::from_iter((0..rows).map(|r| {
            (r % 17 != 16).then(|| (at(r) % 10_000_000) as i128 - 5_000_000)
        }))
        .with_precision_and_scale(18, 2)
        .expect("18,2 holds every value above"),
    );
    RecordBatch::try_new(
        Arc::new(ArrowSchema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("dec", DataType::Decimal128(18, 2), true),
        ])),
        vec![id, decs],
    )
    .expect("two arrays of one length")
}

/// The same columns under `prefix`-ed names — a join's two sides over one batch shape
/// need distinct names, and a join's declared output cannot carry `id` twice.
pub(crate) fn prefixed(batch: &RecordBatch, prefix: &str) -> RecordBatch {
    let fields: Vec<Field> = batch
        .schema()
        .fields()
        .iter()
        .map(|f| Field::new(format!("{prefix}{}", f.name()), f.data_type().clone(), f.is_nullable()))
        .collect();
    RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), batch.columns().to_vec())
        .expect("a rename keeps every array under its type")
}
```

- [ ] **Step 7: Run green**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- tests::synthetic
```

Expected: 5 passed. If `Date32Array::from_iter` or `Decimal128Array::from_iter` do not take an
`Option` iterator in the pinned arrow, build them with `from(Vec<Option<_>>)` instead — the
behaviour is the assertion, not the constructor.

- [ ] **Step 8: Write the comparator's red cases first**

In `peacockdb-core/src/tests/compare.rs`, at the bottom. Each names the reason it must fail for,
and the guard is proven by the message it produces:

```rust
#[test]
fn a_slot_whose_value_differs_is_named_with_the_slot() {
    let cpu = vec![vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(8, 2)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the values differ");
    assert!(why.starts_with("slot 0:"), "{why}");
    assert!(why.contains("row"), "{why}");
}

#[test]
fn a_slot_both_sides_left_empty_is_equal_and_one_side_empty_is_named() {
    assert!(same(&[vec![]], &[vec![]], Order::Any).is_ok());
    let why = same(&[vec![synthetic(4, 1)]], &[vec![]], Order::Any).expect_err("one side empty");
    assert!(why.contains("gpu produced no batch") && why.contains("4 rows"), "{why}");
    let why = same(&[vec![]], &[vec![synthetic(0, 1)]], Order::Any).expect_err("no batch is not zero rows");
    assert!(why.contains("cpu produced no batch") && why.contains("0 rows"), "{why}");
}

#[test]
fn nullability_alone_is_not_a_difference() {
    let batch = synthetic(8, 1);
    let fields: Vec<Field> = batch.schema().fields().iter().map(|f| Field::new(f.name(), f.data_type().clone(), true)).collect();
    let all_nullable = RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), batch.columns().to_vec()).unwrap();
    assert!(same(&[vec![batch]], &[vec![all_nullable]], Order::AsEmitted).is_ok());
}

#[test]
fn a_slot_whose_type_differs_fails_before_any_row_is_read() {
    let cpu = vec![vec![synthetic(8, 1)]];
    let widened = prefixed(&synthetic(8, 1), ""); // same values, then retype one column
    let mut fields: Vec<Field> = widened.schema().fields().iter().map(|f| f.as_ref().clone()).collect();
    fields[2] = Field::new("i32", DataType::Int64, true);
    let mut columns = widened.columns().to_vec();
    columns[2] = datafusion::arrow::compute::cast(&columns[2], &DataType::Int64).unwrap();
    let gpu = vec![vec![RecordBatch::try_new(Arc::new(ArrowSchema::new(fields)), columns).unwrap()]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the schemas differ");
    assert!(why.contains("schema"), "{why}");
    assert!(why.contains("Int32") && why.contains("Int64"), "{why}");
}

#[test]
fn a_differing_row_count_is_named_as_a_count() {
    let cpu = vec![vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(9, 1)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the counts differ");
    assert!(why.contains("8 rows") && why.contains("9 rows"), "{why}");
}

#[test]
fn a_differing_slot_count_is_named_before_any_slot_is_compared() {
    let cpu = vec![vec![synthetic(8, 1)], vec![synthetic(8, 1)]];
    let gpu = vec![vec![synthetic(8, 1)]];
    let why = same(&cpu, &gpu, Order::Any).expect_err("the slot counts differ");
    assert!(why.contains("2 slots") && why.contains("1 slot"), "{why}");
}

#[test]
fn any_order_sorts_and_as_emitted_does_not() {
    let batch = synthetic(8, 1);
    let reversed = {
        let indices = datafusion::arrow::array::UInt32Array::from((0..8u32).rev().collect::<Vec<_>>());
        datafusion::arrow::compute::take_record_batch(&batch, &indices).unwrap()
    };
    assert!(same(&[vec![batch.clone()]], &[vec![reversed.clone()]], Order::Any).is_ok());
    assert!(same(&[vec![batch]], &[vec![reversed]], Order::AsEmitted).is_err());
}

#[test]
fn a_slot_of_several_batches_is_one_table() {
    let whole = synthetic(8, 1);
    let halves = vec![whole.slice(0, 3), whole.slice(3, 5)];
    assert!(same(&[vec![whole]], &[halves], Order::AsEmitted).is_ok());
}
```

- [ ] **Step 9: Run them and watch them fail**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- tests::compare
```

Expected: `same` not found.

- [ ] **Step 10: Write the comparator**

```rust
//! What "the same answer" means for one operator on two backends: slot by slot, since output
//! timing is a function of the call sequence on both; within a slot the column names and
//! types, then the rows, exact. A slot is what one call produced — every batch of a `Vec`
//! return, or one lane of an emit — so a scatter that put a row in the wrong lane is a wrong
//! slot and not a passing multiset. A call that produced no batch is an empty slot, which is
//! not a zero-row batch: a limit outside its interval and a one-call join's finish return
//! nothing, and both sides returning nothing is agreement. Nullability is not compared —
//! the device reports it from the data, and `plan/validate.rs` ignores it for that reason.

use datafusion::arrow::compute::{concat_batches, lexsort_to_indices, take_record_batch, SortColumn};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::arrow::util::pretty::pretty_format_batches;

pub(crate) type Slot = Vec<RecordBatch>;

/// `Any`: rows sorted by every column before comparing, for the operators with no order
/// contract. `AsEmitted`: the sorts, whose order is the answer; their synthetic keys have
/// no ties, so neither engine's unstable sort can disagree with the other.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Order {
    Any,
    AsEmitted,
}

pub(crate) fn assert_same(cpu: &[Slot], gpu: &[Slot], order: Order) {
    if let Err(why) = same(cpu, gpu, order) {
        panic!("cpu and gpu differ: {why}");
    }
}

/// The fallible form, so the comparator's own tests can read the reason.
pub(crate) fn same(cpu: &[Slot], gpu: &[Slot], order: Order) -> Result<(), String> {
    if cpu.len() != gpu.len() {
        return Err(format!(
            "cpu produced {} slot{}, gpu {} slot{}",
            cpu.len(), plural(cpu.len()), gpu.len(), plural(gpu.len())
        ));
    }
    for (index, (c, g)) in cpu.iter().zip(gpu).enumerate() {
        same_slot(c, g, order).map_err(|why| format!("slot {index}: {why}"))?;
    }
    Ok(())
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn same_slot(cpu: &Slot, gpu: &Slot, order: Order) -> Result<(), String> {
    let (c, g) = match (one_table(cpu), one_table(gpu)) {
        (None, None) => return Ok(()),
        (Some(c), None) => return Err(format!("gpu produced no batch, cpu {} rows", c.num_rows())),
        (None, Some(g)) => return Err(format!("cpu produced no batch, gpu {} rows", g.num_rows())),
        (Some(c), Some(g)) => (c, g),
    };
    let names_and_types = |batch: &RecordBatch| -> Vec<(String, datafusion::arrow::datatypes::DataType)> {
        batch.schema().fields().iter().map(|f| (f.name().clone(), f.data_type().clone())).collect()
    };
    if names_and_types(&c) != names_and_types(&g) {
        return Err(format!("schema differs\n  cpu: {}\n  gpu: {}", c.schema(), g.schema()));
    }
    if c.num_rows() != g.num_rows() {
        return Err(format!("cpu has {} rows, gpu {} rows", c.num_rows(), g.num_rows()));
    }
    let (c, g) = match order {
        Order::Any => (sorted(&c), sorted(&g)),
        Order::AsEmitted => (c, g),
    };
    // Nullability is out of the comparison, so the rows are compared column by column
    // rather than as batches, whose equality would read the fields' flags too.
    let differ = c.columns().iter().zip(g.columns()).any(|(a, b)| a.as_ref() != b.as_ref());
    if differ {
        return Err(format!(
            "rows differ\n  cpu:\n{}\n  gpu:\n{}",
            pretty_format_batches(&[c]).unwrap(),
            pretty_format_batches(&[g]).unwrap()
        ));
    }
    Ok(())
}

/// A slot's batches concatenated: what one call produced is one table, however it was cut.
/// `None` is a call that produced no batch at all.
fn one_table(slot: &Slot) -> Option<RecordBatch> {
    let first = slot.first()?;
    Some(concat_batches(&first.schema(), slot).expect("one call's batches share a schema"))
}

fn sorted(batch: &RecordBatch) -> RecordBatch {
    if batch.num_rows() == 0 {
        return batch.clone();
    }
    let columns: Vec<SortColumn> = batch
        .columns()
        .iter()
        .map(|values| SortColumn { values: values.clone(), options: None })
        .collect();
    let indices = lexsort_to_indices(&columns, None).expect("every synthetic type sorts");
    take_record_batch(batch, &indices).expect("a permutation of the batch")
}
```

The empty slot is deliberate and load-bearing: `mark_done_and_fetch` on a limit, a one-call
join's `finish_and_fetch`, a build-side semi join's `probe_and_fetch` and an accumulator over
nothing all return `Vec::new()` on a correct engine (`cpu_backend/accumulate.rs`, both
`join.rs`). A comparator that called that an error would fail every limit and join case green.
What it must still catch is one side returning nothing where the other returned a batch — the
#173 shape — which is why `(None, Some)` is a named difference and never folded into "zero rows".

- [ ] **Step 11: Run green, then the whole rust-only lib**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- tests::compare tests::synthetic
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib
```

Expected: 13 passed in the first; the second green with the lifted cpu tests included. The
`sorted` helper's `lexsort_to_indices` must see every column, nulls included, which it does by
default; `take_record_batch` keeps the schema, so the nullability flags never enter the row
comparison.

- [ ] **Step 12: Commit**

```bash
git add peacockdb-core/src/tests/ peacockdb-core/src/executor/cpu_backend/tests/
git commit -m "a stub leaf, a synthetic batch and a comparator that says why

Given moves up to src/tests/ for both backends' tests to share. synthetic(rows,
seed) is eight typed columns with a null in each but the id. same() compares slot
by slot and names the slot, the schema, the count or the row that differs."
```

---

### Task 3: A leaf outside the registry emits no seq

**Files:**
- Modify: `peacockdb-core/src/wire/attach.rs:75` (the `match as_node_ref(node)` in `emit`)
- Modify: `peacockdb-core/src/wire/writer.rs:85` (`stub` gains a caller outside `take`)
- Test: `peacockdb-core/src/wire/tests.rs` (or wherever `test-layout` put `wire`'s unit tests —
  `grep -rn 'fn attach_recipes\|attach_recipes(' peacockdb-core/src/wire/` finds the existing ones)

**Interfaces:**
- Produces: `attach_recipes(root)` returns `Ok` for a tree whose leaves are `Given`, with `None` at
  each leaf's post-order, one stub wire node per leaf, and the operator's recipe at the last.
  `Writer::leaf(&mut self) -> Seq`, `pub(super)`, the stub left in the pool for the parent to take.
  Tasks 4–7 rely on it.

- [ ] **Step 1: Write the failing test**

Beside `wire`'s existing `attach_recipes` tests:

```rust
/// A leaf the registry does not know — `Given`, the operator harness's stub — emits no
/// seq, so the operator above it is the only wire node and the recipe index is the
/// operator's post-order. A filter takes one child, so the writer stubs the slot: two
/// wire nodes, one recipe.
#[test]
fn a_leaf_outside_the_registry_emits_no_seq_and_its_parent_takes_a_stub() {
    use crate::tests::{columns, Given};
    let input = columns(&[("k", DataType::Utf8), ("v", DataType::Int64)]);
    let filter = GpuFilter::new(
        Given::of(input.clone(), BatchLayout::MultipleBatches),
        Expr::binary(
            Expr::column(1, "v"),
            BinaryOp::Gt,
            Expr::Literal(ScalarValue::Int64(Some(1))),
            DataType::Boolean,
        ),
        None,
        input,
    );
    let plan = attach_recipes(&filter).expect("a filter over a given leaf is writable");
    assert!(plan.get(0).is_none(), "the leaf has no recipe");
    let recipe = plan.get(1).expect("the filter has one");
    assert_eq!(recipe.calls.len(), 1);
    assert_eq!(recipe.calls[0].target.map(|(seq, _)| seq), Some(1));
    assert_eq!(plan.wire_nodes(), 2, "the stub and the filter");
}

/// The seqless three put nothing on the wire themselves, so the leaf's stub is the whole
/// plan: the writer refuses a plan with no root, and so does the C++. A limit over a given
/// leaf is one stub node with one bare-call recipe.
#[test]
fn a_seqless_operator_over_a_given_leaf_is_a_plan_of_one_stub() {
    use crate::tests::{columns, Given};
    let input = columns(&[("k", DataType::Utf8)]);
    let limit = GpuLimit::new(
        Given::of(input, BatchLayout::MultipleBatches),
        RowInterval { skip: 1, fetch: Some(2) },
    );
    let plan = attach_recipes(&limit).expect("writable");
    assert!(plan.get(0).is_none());
    assert_eq!(plan.get(1).expect("the limit's").calls[0].symbol, AbiSymbol::SliceHandle);
    assert_eq!(plan.wire_nodes(), 1, "the stub, and nothing else");
}
```

- [ ] **Step 2: Run them and watch them fail**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- outside_the_registry seqless_operator
```

Expected: a panic, "a plan node outside the registry reached a consumer of it".

- [ ] **Step 3: The writer's stub, reachable**

In `writer.rs`, beside `stub`:

```rust
    /// A leaf with nothing to call, occupying one slot: the stub `take` would have made for
    /// its parent, made now so a seqless parent still has a root. Test-built trees only —
    /// a planned tree has no leaf but a scan.
    pub(super) fn leaf(&mut self) -> Seq {
        let offset = self.stub();
        self.pool.push(offset);
        self.next_seq - 1
    }
```

Read `stub` and `push` first: `stub` pops what `push` pooled, so `leaf` pushes it back, and the
seq is whatever `push` handed out — take it from `push`'s return rather than `next_seq - 1` if
that is what `push` returns.

- [ ] **Step 4: The arm**

In `attach.rs`'s `emit`, replace `match as_node_ref(node) {` with:

```rust
    // A node the registry does not know is a hand-built leaf under test, which declares a
    // schema and nothing to call: a stub on the wire, so the plan keeps a root under a
    // seqless parent, and no recipe.
    let Some(node_ref) = try_as_node_ref(node) else {
        writer.leaf();
        return Ok(None);
    };
    match node_ref {
```

and swap the `as_node_ref` import for `try_as_node_ref`. The other arms are untouched. A
registered parent above the stub takes it through `take`, exactly as it takes a forwarder's.

- [ ] **Step 5: Run green, then `wire`'s whole test set and the plan goldens**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- wire::
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

Expected: green, no golden moved (`git status testdata/goldens` clean). The recipe-payload golden
is byte-pinned; an unregistered leaf never occurs in a planned tree, so nothing there can move.

- [ ] **Step 6: Commit**

```bash
git add peacockdb-core/src/wire/
git commit -m "a leaf outside the registry is a stub on the wire and no recipe

try_as_node_ref in attach's emit: a hand-built leaf under test declares a schema
and nothing to call. It occupies one stub node so a seqless parent keeps a root."
```

---

### Task 4: `Device`, and the round trip

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/mod.rs`
- Create: `peacockdb-core/src/tests/gpu_tests/device.rs`
- Create: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` (the round trip; Tasks 5–6 add to it)
- Modify: `peacockdb-core/src/tests/mod.rs` (`#[cfg(feature = "gpu")] mod gpu_tests;` — spelled
  exactly as `test-layout` spelled the other `gpu_tests` modules; `grep -rn 'mod gpu_tests'
  peacockdb-core/src/` shows the form)

**Interfaces:**
- Produces: `Device::open(node: &dyn GpuNode) -> Device`; `Device::ctx(&self) -> &GpuContext`;
  `Device::upload(&self, &RecordBatch) -> GpuBatch`; `Device::fetch(&self, GpuBatch, RowRange) ->
  Option<RecordBatch>` (`None` when the device shipped nothing — a range naming no rows). `Drop`
  ends the plan and destroys the executor. Tasks 5–7 consume it.

- [ ] **Step 1: Write the round-trip cases**

`peacockdb-core/src/tests/gpu_tests/harness_cases.rs`:

```rust
//! The seqless three, and the helper round trip — the cases whose recipe puts nothing on
//! the wire, so a wrong helper has nowhere to hide.

use super::device::Device;
use crate::executor::RowRange;
use crate::plan::{BatchLayout, GpuUnload, Schema};
use crate::tests::{assert_same, synthetic, Given, Order};

/// Any node over a given leaf opens a session; the unload's is the emptiest plan there is.
fn empty_plan() -> GpuUnload {
    GpuUnload::new(
        Given::of(Schema::new(synthetic(0, 0).schema()), BatchLayout::MultipleBatches),
        None,
    )
}

#[test]
fn an_uploaded_batch_comes_back_as_itself() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device.fetch(device.upload(&batch), RowRange::WHOLE).expect("a whole export ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_row_range_ships_those_rows_in_order() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(64, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange { offset: 10, length: 5 })
        .expect("five rows ship");
    assert_same(&[vec![batch.slice(10, 5)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn a_range_past_the_end_ships_nothing() {
    let device = Device::open(&empty_plan());
    let back = device.fetch(device.upload(&synthetic(8, 1)), RowRange { offset: 8, length: 1 });
    assert!(back.is_none());
}

#[test]
fn a_range_over_the_end_is_clamped() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(8, 1);
    let back = device
        .fetch(device.upload(&batch), RowRange { offset: 6, length: 100 })
        .expect("two rows ship");
    assert_same(&[vec![batch.slice(6, 2)]], &[vec![back]], Order::AsEmitted);
}

#[test]
fn zero_rows_round_trip_as_zero_rows_under_the_schema() {
    let device = Device::open(&empty_plan());
    let batch = synthetic(0, 1);
    let back = device.fetch(device.upload(&batch), RowRange::WHOLE).expect("the schema ships");
    assert_same(&[vec![batch]], &[vec![back]], Order::AsEmitted);
}
```

- [ ] **Step 2: Build and watch it fail to compile**

```bash
timeout 3600 scripts/build-test-shadgpu.sh --build
```

Expected: `Device` not found. (The `--build` compiles the `gpu` feature; a rust-only build never
sees this module.)

- [ ] **Step 3: Write `Device`**

`peacockdb-core/src/tests/gpu_tests/mod.rs`:

```rust
//! The operator harness's device half: a session over one operator's recipe, an upload,
//! a fetch, and the same call sequence run on both backends.

mod device;
mod harness_cases;
```

`peacockdb-core/src/tests/gpu_tests/device.rs`:

```rust
use std::ffi::c_void;
use std::sync::Arc;

use datafusion::arrow::array::{Array, ArrayRef, StructArray};
use datafusion::arrow::datatypes::Field;
use datafusion::arrow::ffi::to_ffi;
use datafusion::arrow::ipc::reader::StreamReader;
use datafusion::arrow::record_batch::RecordBatch;
use peacockdb_ffi::raw::{
    peacock_executor_begin_plan, peacock_executor_create, peacock_executor_destroy,
    peacock_executor_end_plan, peacock_handle_from_arrow, peacock_last_error,
    peacock_result_free, peacock_result_from_handle, PeacockExecutor,
};

use crate::executor::{Batch, CpuBatch, GpuBatch, GpuContext, RowRange};
use crate::plan::GpuNode;
use crate::wire::attach_recipes;

/// One session over one operator: `attach_recipes` over the node, its bytes handed to
/// `begin_plan`, and the context `GpuBackend::executors_for` reads. Batches go up through
/// [`Device::upload`] and come back through [`Device::fetch`], so a test never writes a
/// file and the scan is never in the path of an operator it is not testing.
pub(crate) struct Device {
    ctx: GpuContext,
}

impl Device {
    pub(crate) fn open(node: &dyn GpuNode) -> Self {
        let recipes = attach_recipes(node).expect("every node's payload is writable");
        let mut executor: *mut PeacockExecutor = std::ptr::null_mut();
        assert_eq!(unsafe { peacock_executor_create(0, &mut executor) }, 0, "executor_create");
        let bytes = recipes.bytes();
        let mut nodes = 0u64;
        let rc = unsafe {
            peacock_executor_begin_plan(executor, bytes.as_ptr(), bytes.len() as u64, &mut nodes)
        };
        assert_eq!(rc, 0, "begin_plan: {}", error_of(executor));
        assert_eq!(nodes as usize, recipes.wire_nodes());
        Self { ctx: GpuContext { executor, recipes } }
    }

    pub(crate) fn ctx(&self) -> &GpuContext {
        &self.ctx
    }

    /// The batch as the device's table, priced exactly as `CpuBatch` prices the same rows,
    /// so an accumulator's byte threshold trips at the same arrival on both sides.
    pub(crate) fn upload(&self, batch: &RecordBatch) -> GpuBatch {
        let columns: Vec<(Arc<Field>, ArrayRef)> = batch
            .schema()
            .fields()
            .iter()
            .cloned()
            .zip(batch.columns().iter().cloned())
            .collect();
        let table = StructArray::from(columns);
        let (array, schema) = to_ffi(&table.to_data()).expect("arrow exports its own array");
        let mut handle = 0u64;
        let rc = unsafe {
            peacock_handle_from_arrow(
                self.ctx.executor,
                &schema as *const _ as *const c_void,
                &array as *const _ as *const c_void,
                &mut handle,
            )
        };
        assert_eq!(rc, 0, "upload: {}", error_of(self.ctx.executor));
        let bytes = CpuBatch::new(batch.clone()).byte_size();
        GpuBatch::new(self.ctx.executor, handle, batch.num_rows(), bytes)
    }

    /// What the device exported, under the schema it exported it with — never the one the
    /// node declared, so a type the device changed reaches the comparator as itself.
    /// `None` is the device shipping nothing: a range naming no rows of a non-empty table.
    pub(crate) fn fetch(&self, batch: GpuBatch, rows: RowRange) -> Option<RecordBatch> {
        let mut ipc: *mut u8 = std::ptr::null_mut();
        let mut len = 0u64;
        let rc = unsafe {
            peacock_result_from_handle(
                self.ctx.executor, batch.handle(), rows.offset, rows.length, &mut ipc, &mut len,
            )
        };
        assert_eq!(rc, 0, "fetch: {}", error_of(self.ctx.executor));
        if len == 0 {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(ipc, len as usize) };
        let batches: Vec<RecordBatch> = StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .expect("an IPC stream")
            .collect::<Result<_, _>>()
            .expect("every batch of it");
        unsafe { peacock_result_free(ipc) };
        let schema = batches.first().expect("a stream carries its schema").schema();
        Some(datafusion::arrow::compute::concat_batches(&schema, &batches).expect("one table"))
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe {
            peacock_executor_end_plan(self.ctx.executor);
            peacock_executor_destroy(self.ctx.executor);
        }
    }
}

pub(crate) fn error_of(executor: *mut PeacockExecutor) -> String {
    let message = unsafe { peacock_last_error(executor) };
    if message.is_null() {
        return String::new();
    }
    unsafe { std::ffi::CStr::from_ptr(message) }.to_string_lossy().into_owned()
}
```

Two things to check against the tree rather than assume: the IPC stream of a zero-row table
may carry a schema and no batch — then `batches.first()` is wrong and the schema comes from
`StreamReader::schema()`, so read the schema off the reader before collecting; and
`peacock_executor_create`'s budget argument may be asserted non-zero somewhere — use the same
constant `test_gpu_executors`' `Session::open` uses if so.

- [ ] **Step 4: Run the round trip on shad-gpu**

```bash
PCK_TEST_FILTER='tests::gpu_tests::harness_cases' timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: 5 passed. If `from_arrow` refuses a type, the message names it; `Date32` is the one to
suspect. A decimal never reaches this task's fixture: `decimals` is task 9's, and its round trip
is expected to come back at precision 38 (#187) — the first decimal case there says so.

- [ ] **Step 5: Commit**

```bash
git add peacockdb-core/src/tests/
git commit -m "a batch the test wrote goes up and comes back as itself

Device: attach_recipes over one operator, begin_plan on its bytes, an upload
through the test-only symbol, a fetch that keeps the schema the device exported."
```

---

### Task 5: `run_both`, and `GpuUnload` through `executors_for`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/script.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/mod.rs` (`mod script;`)
- Modify: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` (the unload cases)

**Interfaces:**
- Produces: `enum Script { Exec(Vec<RecordBatch>), Accumulate(Vec<RecordBatch>),
  Lanes(Vec<Vec<RecordBatch>>), Emit(Vec<RecordBatch>), Join { build: Option<RecordBatch>,
  probe: Vec<RecordBatch> }, Unload { batch: RecordBatch, rows: RowRange }, Source { lane: usize } }`;
  `struct Outcome { cpu: Result<Vec<Slot>, BackendError>, gpu: Result<Vec<Slot>, BackendError> }`;
  `run_both(node: &dyn GpuNode, script: Script) -> Outcome`; `Outcome::same(&self, Order)` (panics
  with the reason); `Outcome::gpu_refuses(&self) -> &str` and `cpu_refuses` (the message, for a
  `bug_` test). Tasks 6–7 and every case in `operator-cases.md` consume it.

- [ ] **Step 1: Write the unload cases**

Append to `harness_cases.rs`:

```rust
use datafusion::arrow::record_batch::RecordBatch;

use super::script::{run_both, Script};

fn unload_over(rows: usize) -> (GpuUnload, RecordBatch) {
    let batch = synthetic(rows, 2);
    let node = GpuUnload::new(
        Given::of(Schema::new(batch.schema()), BatchLayout::MultipleBatches),
        None,
    );
    (node, batch)
}

#[test]
fn an_unload_hands_the_whole_batch_over_on_both_backends() {
    let (node, batch) = unload_over(64);
    run_both(&node, Script::Unload { batch, rows: RowRange::WHOLE }).same(Order::AsEmitted);
}

#[test]
fn an_unload_over_a_range_hands_those_rows_over() {
    let (node, batch) = unload_over(64);
    run_both(&node, Script::Unload { batch, rows: RowRange { offset: 20, length: 7 } })
        .same(Order::AsEmitted);
}

#[test]
fn an_unload_clamps_a_range_over_the_end_the_same_way() {
    let (node, batch) = unload_over(64);
    run_both(&node, Script::Unload { batch, rows: RowRange { offset: 60, length: 100 } })
        .same(Order::AsEmitted);
}

#[test]
fn an_unload_of_a_range_past_the_end_is_zero_rows_on_both() {
    let (node, batch) = unload_over(8);
    run_both(&node, Script::Unload { batch, rows: RowRange { offset: 8, length: 4 } })
        .same(Order::AsEmitted);
}

// Empty inputs, each its own case.
#[test]
fn an_unload_of_a_zero_row_batch_is_zero_rows_under_the_schema_on_both() {
    let (node, batch) = unload_over(0);
    run_both(&node, Script::Unload { batch, rows: RowRange::WHOLE }).same(Order::AsEmitted);
}

#[test]
fn a_range_over_a_zero_row_batch_is_zero_rows_on_both() {
    let (node, batch) = unload_over(0);
    run_both(&node, Script::Unload { batch, rows: RowRange { offset: 0, length: 5 } })
        .same(Order::AsEmitted);
}
```

- [ ] **Step 2: Write `script.rs`**

```rust
//! One call sequence, two backends. `Script` says which calls in `RecordBatch` terms;
//! `drive` makes them on any `Backend` through `executors_for`, so what runs is the trait
//! and not a constructor; `run_both` does it on each and hands back both results.

use std::sync::Arc;

use datafusion::arrow::record_batch::RecordBatch;
use datafusion::execution::context::SessionContext;

use super::device::Device;
use crate::executor::cpu_backend::CpuBackend;
use crate::executor::gpu_backend::GpuBackend;
use crate::executor::{
    Backend, BackendError, BatchAccumulatorExecutor, CpuBatch, ExecExecutor, JoinExecutor,
    LaneEvent, NodeExecutors, PartitionAccumulatorExecutor, PartitionEmitterExecutor, ProbingJoin,
    RowRange, SourceExecutor, SourceStep, UnloadExecutor,
};
use crate::plan::{category_of, ExecutorCategory, GpuNode};
use crate::tests::{same, Order, Slot};

pub(crate) enum Script {
    /// One `exec` per batch; one slot each.
    Exec(Vec<RecordBatch>),
    /// `accumulate_and_fetch` per batch, then `mark_done_and_fetch`; one slot per call.
    Accumulate(Vec<RecordBatch>),
    /// Per lane, in lane order: `Batch` per batch then `Done`; one slot per call.
    Lanes(Vec<Vec<RecordBatch>>),
    /// One `emit` per batch; one slot per output lane per call.
    Emit(Vec<RecordBatch>),
    /// `set_build` then `probe_and_fetch` per probe batch then `finish_and_fetch`, one slot
    /// per call; with no build, `without_build` and no slots at all.
    Join { build: Option<RecordBatch>, probe: Vec<RecordBatch> },
    /// One `unload`; one slot.
    Unload { batch: RecordBatch, rows: RowRange },
    /// `next_batch` until exhausted, on `lane`; one slot per batch.
    Source { lane: usize },
}

impl Script {
    fn category(&self) -> ExecutorCategory {
        match self {
            Script::Exec(_) => ExecutorCategory::Exec,
            Script::Accumulate(_) => ExecutorCategory::BatchAccumulator,
            Script::Lanes(_) => ExecutorCategory::PartitionAccumulator,
            Script::Emit(_) => ExecutorCategory::PartitionEmitter,
            Script::Join { .. } => ExecutorCategory::Join,
            Script::Unload { .. } => ExecutorCategory::Unload,
            Script::Source { .. } => ExecutorCategory::Source,
        }
    }

    fn lane(&self) -> usize {
        match self {
            Script::Source { lane } => *lane,
            _ => 0,
        }
    }
}

pub(crate) struct Outcome {
    pub(crate) cpu: Result<Vec<Slot>, BackendError>,
    pub(crate) gpu: Result<Vec<Slot>, BackendError>,
}

impl Outcome {
    /// Both answered, and with the same thing.
    pub(crate) fn same(&self, order: Order) {
        let cpu = self.cpu.as_ref().unwrap_or_else(|why| panic!("cpu refused: {}", why.message));
        let gpu = self.gpu.as_ref().unwrap_or_else(|why| panic!("gpu refused: {}", why.message));
        if let Err(why) = same(cpu, gpu, order) {
            panic!("cpu and gpu differ: {why}");
        }
    }

    /// The device's refusal, for a `bug_` test to pin by message; a device that answered
    /// is the failure that says the ticket closed.
    pub(crate) fn gpu_refuses(&self) -> &str {
        assert!(self.cpu.is_ok(), "the cpu refused too: {:?}", self.cpu);
        &self.gpu.as_ref().expect_err("the device was expected to refuse").message
    }

    pub(crate) fn cpu_refuses(&self) -> &str {
        assert!(self.gpu.is_ok(), "the device refused too: {:?}", self.gpu);
        &self.cpu.as_ref().expect_err("the cpu was expected to refuse").message
    }
}

pub(crate) fn run_both(node: &dyn GpuNode, script: Script) -> Outcome {
    assert_eq!(
        category_of(node),
        script.category(),
        "the script's shape is not the node's category"
    );
    let cpu_ctx = SessionContext::new().task_ctx();
    let cpu = drive::<CpuBackend>(
        &cpu_ctx,
        node,
        &script,
        |batch| CpuBatch::new(batch.clone()),
        |batch| batch.into_record_batch(),
    );
    let device = Device::open(node);
    let gpu = drive::<GpuBackend>(
        device.ctx(),
        node,
        &script,
        |batch| device.upload(batch),
        |batch| device.fetch(batch, RowRange::WHOLE).expect("a whole export ships its schema"),
    );
    Outcome { cpu, gpu }
}

/// The script on one backend. `up` and `down` are that backend's two conversions, and the
/// only thing that differs between the two runs.
fn drive<B: Backend>(
    ctx: &B::Context,
    node: &dyn GpuNode,
    script: &Script,
    up: impl Fn(&RecordBatch) -> B::Batch,
    down: impl Fn(B::Batch) -> RecordBatch,
) -> Result<Vec<Slot>, BackendError> {
    // The root's post-order is the tree's size less one. Counted here rather than through
    // `post_order_of_every_node`, whose index asks every node's category and so refuses a
    // `Given` leaf.
    fn size(node: &dyn GpuNode) -> usize {
        1 + node.children().into_iter().map(size).sum::<usize>()
    }
    let post_order = size(node) - 1;
    let executors = B::executors_for(ctx, node, post_order, script.lane())
        .map_err(|why| BackendError::new(format!("executors_for: {why}")))?;
    let lower = |batches: Vec<B::Batch>| batches.into_iter().map(&down).collect::<Slot>();
    let mut slots = Vec::new();
    match (executors, script) {
        (NodeExecutors::Exec(mut exec), Script::Exec(batches)) => {
            for batch in batches {
                let (out, _) = exec.exec(up(batch))?;
                slots.push(vec![down(out)]);
            }
        }
        (NodeExecutors::BatchAccumulator(mut acc), Script::Accumulate(batches)) => {
            for batch in batches {
                let (out, _) = acc.accumulate_and_fetch(up(batch))?;
                slots.push(lower(out));
            }
            let (out, _) = acc.mark_done_and_fetch()?;
            slots.push(lower(out));
        }
        (NodeExecutors::PartitionAccumulator(mut acc), Script::Lanes(lanes)) => {
            for (lane, batches) in lanes.iter().enumerate() {
                for batch in batches {
                    let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Batch(up(batch)))?;
                    slots.push(lower(out));
                }
                let (out, _) = acc.accumulate_and_fetch(lane, LaneEvent::Done)?;
                slots.push(lower(out));
            }
        }
        (NodeExecutors::PartitionEmitter(mut emitter), Script::Emit(batches)) => {
            for batch in batches {
                let (lanes, _) = emitter.emit(up(batch))?;
                for out in lanes {
                    slots.push(vec![down(out)]);
                }
            }
        }
        (NodeExecutors::Join(join), Script::Join { build, probe }) => match build {
            Some(build) => {
                let (mut probing, _) = join.set_build(up(build))?;
                for batch in probe {
                    let (out, _) = probing.probe_and_fetch(up(batch))?;
                    slots.push(lower(out));
                }
                let (out, _) = probing.finish_and_fetch()?;
                slots.push(lower(out));
            }
            None => {
                assert!(probe.is_empty(), "a join with no build side is never probed");
                join.without_build()?;
            }
        },
        (NodeExecutors::Unload(mut unload), Script::Unload { batch, rows }) => {
            let (out, _) = unload.unload(up(batch), *rows)?;
            slots.push(vec![out.into_record_batch()]);
        }
        (NodeExecutors::Source(mut source), Script::Source { .. }) => loop {
            match source.next_batch()? {
                SourceStep::Batch { batch, source: next, .. } => {
                    slots.push(vec![down(batch)]);
                    source = next;
                }
                SourceStep::Exhausted => break,
            }
        },
        (executors, _) => panic!(
            "executors_for answered {:?} for a script of another shape",
            executors.category()
        ),
    }
    Ok(slots)
}
```

Read `executor/mod.rs` for the exact trait method names before trusting the ones above — the
signatures quoted in the spec's survey are `exec`, `accumulate_and_fetch`, `mark_done_and_fetch`,
`emit`, `set_build`, `without_build`, `probe_and_fetch`, `finish_and_fetch`, `unload`,
`next_batch`. If `RowRange` is not `Copy`, clone it at the unload arm. If `BackendError::new` is
not the constructor, use whatever `gpu_backend/mod.rs` uses.

- [ ] **Step 3: Run on shad-gpu**

```bash
PCK_TEST_FILTER='tests::gpu_tests::harness_cases' timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: 11 passed. A `CpuUnload` that keeps rows a `GpuExport` drops, or the reverse, is a
divergence: ticket it, pin it as a `bug_` test with `outcome.gpu_refuses()` or the wrong slot, and
leave the green case beside it only if a green case remains.

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/
git commit -m "one script, two backends: run_both

drive is generic over Backend and takes the two batch conversions as arguments,
so the same seven arms run on the cpu and on a device. The unload is the first
operator through it."
```

---

### Task 6: `GpuLimit`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs`

- [ ] **Step 1: Write the cases**

```rust
use crate::plan::{GpuLimit, RowInterval};

/// A limit over a stream of `batches` batches of `rows` rows each.
fn limit_over(skip: u64, fetch: Option<u64>, rows: usize, batches: usize) -> (GpuLimit, Vec<RecordBatch>) {
    let stream: Vec<RecordBatch> = (0..batches).map(|i| synthetic(rows, 10 + i as u64)).collect();
    let node = GpuLimit::new(
        Given::of(Schema::new(stream[0].schema()), BatchLayout::MultipleBatches),
        RowInterval { skip, fetch },
    );
    (node, stream)
}

#[test]
fn an_interval_inside_one_batch_slices_that_batch() {
    let (node, stream) = limit_over(3, Some(4), 16, 1);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn an_interval_straddling_two_batches_slices_both() {
    let (node, stream) = limit_over(12, Some(8), 16, 2);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn batches_entirely_outside_the_interval_produce_nothing() {
    let (node, stream) = limit_over(40, Some(4), 16, 4);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_skip_alone_drops_the_prefix_and_keeps_the_rest() {
    let (node, stream) = limit_over(20, None, 16, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_stream_of_several_batches_is_cut_at_the_same_two_edges() {
    let (node, stream) = limit_over(5, Some(30), 8, 6);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

// Empty inputs, each its own case.
#[test]
fn a_stream_of_one_zero_row_batch_answers_nothing_on_both() {
    let (node, stream) = limit_over(0, Some(4), 0, 1);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_zero_row_batch_inside_a_stream_counts_no_rows_on_both() {
    let (node, mut stream) = limit_over(10, Some(10), 8, 3);
    stream.insert(1, synthetic(0, 99));
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn a_stream_of_nothing_but_zero_row_batches_answers_nothing_on_both() {
    let (node, stream) = limit_over(2, Some(4), 0, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}

#[test]
fn an_interval_no_batch_reaches_answers_nothing_on_both() {
    let (node, stream) = limit_over(1000, Some(4), 8, 3);
    run_both(&node, Script::Accumulate(stream)).same(Order::AsEmitted);
}
```

- [ ] **Step 2: Run on shad-gpu**

```bash
PCK_TEST_FILTER='tests::gpu_tests::harness_cases' timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: 20 passed. Three things a limit can do differently and each is a divergence, not a
harness fault: a batch entirely outside the interval coming back as a zero-row batch on one side
and as no batch on the other (the comparator names it "produced no batch" against a row count);
the two straddling batches sliced at an edge off by one; a pure offset never satisfying on one
side. Each gets a ticket and a `bug_` test asserting the wrong slot, and the green case stays only
if a green case remains.

- [ ] **Step 3: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/harness_cases.rs
git commit -m "a limit cuts the same two edges on both backends"
```

---

### Task 7: The kind registry and its guard

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/coverage.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/mod.rs` (`#[macro_use] mod coverage;` first)
- Modify: `peacockdb-core/src/tests/gpu_tests/harness_cases.rs` (every case through `operator_case!`)

**Interfaces:**
- Produces: `operator_case! { GpuLimit, fn name() { … } }` — defines the `#[test]` and registers
  `Covers { kind: "GpuLimit", case: "name" }`; `PENDING: &[&str]`, the kinds without a case yet,
  which `operator-cases.md` empties and then deletes. Every case in that task is written through
  the macro.

- [ ] **Step 1: Write the guard first**

`peacockdb-core/src/tests/gpu_tests/coverage.rs`:

```rust
//! Which kinds the harness has a case for, read off a registry the cases write — never off
//! source text, which is the reader-that-stops-at-the-first-match class — and checked
//! against the node registry in both directions.

use std::collections::BTreeSet;

use crate::plan::node_name;
use crate::tests::rebuild::every_kind;

pub(crate) struct Covers {
    pub(crate) kind: &'static str,
    pub(crate) case: &'static str,
}

inventory::collect!(Covers);

/// A case's test, and its registry entry, from one declaration — so a case cannot exist
/// unregistered and an entry cannot name a test that is not there.
macro_rules! operator_case {
    ($kind:ident, fn $name:ident() $body:block) => {
        inventory::submit! {
            $crate::tests::gpu_tests::coverage::Covers {
                kind: stringify!($kind),
                case: stringify!($name),
            }
        }
        #[test]
        fn $name() $body
    };
}

/// The three forwarders have no executor and belong to the driver, which is tested
/// elsewhere. Permanent.
const EXCLUDED: &[&str] = &["GpuMergePartitions", "GpuUnion", "GpuInterleave"];

/// Kinds with no case yet. `operator-cases.md` empties this list and deletes it; until
/// then a kind that gains a case must leave it, or the reverse check goes red.
const PENDING: &[&str] = &[
    "GpuLoadParquet",
    "GpuFilter",
    "GpuProject",
    "GpuSort",
    "GpuCoalesceAllBatches",
    "GpuAccumulateBatchesAndSort",
    "GpuAggregate",
    "GpuAggregateBatches",
    "GpuHashJoin",
    "GpuCrossJoin",
    "GpuNestedLoopJoin",
    "GpuEmitPartitions",
    "GpuMergeSortedPartitions",
];

#[test]
fn every_kind_has_a_case_or_is_named_as_pending_or_excluded() {
    let kinds: BTreeSet<&str> = every_kind().iter().map(|node| node_name(node.as_any())).collect();
    let covered: BTreeSet<&str> = inventory::iter::<Covers>.into_iter().map(|c| c.kind).collect();
    let missing: Vec<&&str> = kinds
        .iter()
        .filter(|k| !covered.contains(*k) && !PENDING.contains(k) && !EXCLUDED.contains(k))
        .collect();
    assert!(missing.is_empty(), "kinds with no case and not pending: {missing:?}");
    let unknown: Vec<&&str> = covered.iter().filter(|k| !kinds.contains(*k)).collect();
    assert!(unknown.is_empty(), "cases naming a kind that is not one: {unknown:?}");
    let stale: Vec<&&str> = PENDING.iter().chain(EXCLUDED).filter(|k| covered.contains(*k)).collect();
    assert!(stale.is_empty(), "listed as pending or excluded, but has a case: {stale:?}");
    let not_kinds: Vec<&&str> = PENDING.iter().chain(EXCLUDED).filter(|k| !kinds.contains(*k)).collect();
    assert!(not_kinds.is_empty(), "listed, but not a kind: {not_kinds:?}");
}
```

Check `node_name`'s visibility (`pub(crate)` in `plan/mod.rs`) and whether `every_kind` lives at
`crate::tests::rebuild` after `test-layout` — `grep -rn 'fn every_kind' peacockdb-core/src/`. If
`every_kind` builds a source over a real parquet path, that is the one place this task touches
`testdata/`: it constructs nodes and reads nothing, so it stays within "no dataset".

- [ ] **Step 2: Convert the fifteen operator cases to `operator_case!`**

In `harness_cases.rs`, each `#[test] fn x() { … }` becomes `operator_case! { GpuUnload, fn x() { … } }`
or `operator_case! { GpuLimit, … }`. The five round-trip cases exercise no node kind: they stay
plain `#[test]`s.

- [ ] **Step 3: Run on shad-gpu, including the guard**

```bash
PCK_TEST_FILTER='tests::gpu_tests' timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: 21 passed. Then prove the guard can go red — remove `"GpuLoadParquet"` from `PENDING`,
run, see `kinds with no case and not pending: ["GpuLoadParquet"]`, restore it. Say in the detail
file that you did.

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/
git commit -m "every kind has a case, is pending, or is a forwarder

operator_case! writes the registry entry the guard reads. PENDING is the list
operator-cases empties; a kind that gains a case must leave it."
```

---

### Task 8: The pages, the guards, and the handoff

**Files:**
- Modify: `llm-wiki/build-test.md` (the categories table, the grand total)
- Modify: `llm-wiki/architecture.md` (the Interfaces paragraph counting the ABI)
- Modify: `llm-wiki/tasks/operator-harness-detail.md`

- [ ] **Step 1: `test_ci_coverage` without a workflow edit**

```bash
timeout 900 cargo test --features rust-only -p peacockdb-core --test test_ci_coverage
```

Expected: green. The lib binary's `gpu_tests::` filter reaches `tests::gpu_tests::…` because cargo
matches substrings; if the guard has a per-module list, add this module to it and nothing else.

- [ ] **Step 2: `build-test.md`**

One row in the categories table, after "Executors on a device":

```
| Operator harness (Rust) | one hand-built node over stub leaves, a script of synthetic batches, both backends through `executors_for`, the outputs compared slot by slot and exactly — schema included, so a type the device changes is red here rather than at a query's root. This task's cases are the seqless three: the upload round trip, the unload and the limit; a guard names every kind without a case | [an_unload_hands_the_whole_batch_over_on_both_backends](../peacockdb-core/src/tests/gpu_tests/harness_cases.rs) | shad-gpu | 21 |
```

Move the grand total and the Rust figure by 21, plus whatever Tasks 2 and 3 added at the rust
rung (13 comparator and synthetic cases, 2 attach cases) to the "Lib unit" row. Count with
`--list` rather than by hand.

- [ ] **Step 3: `architecture.md`**

In "Interfaces", "The ABI is sixteen symbols in five groups" becomes seventeen, and the
conformance clause becomes "and two test hooks: `peacock_spark_partition_ids`, which runs the
murmur3 kernel over one Arrow C-data batch so the Rust side can compare it against comet's, and
`peacock_handle_from_arrow`, which adopts one such batch as a handle so the operator harness can
hand an executor a table it wrote". Nothing else on the page moved.

- [ ] **Step 4: Format, then the whole proof**

```bash
rustfmt peacockdb-core/src/tests/given.rs peacockdb-core/src/tests/synthetic.rs peacockdb-core/src/tests/compare.rs peacockdb-core/src/tests/gpu_tests/*.rs
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib
timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: rust-only lib green; every staged device binary green, this module's 21 among them.

- [ ] **Step 5: The detail file**

What the device did with a decimal, if one reached it; which arrow constructors the pinned version
wanted; whether any case became a `bug_` test and under which ticket; that the guard was shown
red once. The next developer is `operator-cases.md`'s, and every one of those is a thing they
would otherwise rediscover.

- [ ] **Step 6: Commit**

```bash
git add llm-wiki/build-test.md llm-wiki/architecture.md llm-wiki/tasks/operator-harness-detail.md peacockdb-core/src/tests/
git commit -m "the harness has a row, the ABI has seventeen symbols"
```
