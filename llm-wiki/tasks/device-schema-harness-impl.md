# Device schema harness implementation plan

**Goal:** A handle's schema readable from Rust as cuDF's `{type_id, scale}`, a comparator that
projects the plan's arrow schema the same way, a new schema case per node kind in every harness
family, and eleven hand-written spot-checks inside the recipe walk — every existing case
untouched.

**Architecture:** `peacock_handle_schema` (task 2's) returns the IPC schema message;
`test_support::device_schema` decodes it into `DeviceSchema`, projects `DataType` onto
`DeviceType`, and is where `schema_of(&GpuBatch)` lives — under the `test-support` feature, so
the corpus binary and the next task's validator can call it; `Device` and the walk's `Session`
delegate. New cases live in `*_schema_cases.rs` files beside each family; the walk gains an
`on_call` hook and named spot-check tests. A red case is a `bug_` with a ticket, never a fix.

**Tech stack:** Rust, `test_support`, the gpu-rung harness on `shad-gpu`. No C++.

**Spec:** [`device-schema-harness.md`](device-schema-harness.md) — frozen.

## Global constraints

- **No existing case changes.** Not an assertion added, not a `bug_` retyped. `git diff` on the
  existing `*_cases.rs` files shows only `mod` lines and shared-builder visibility.
- The projection is exactly cuDF's interop mapping for the types the wire admits: a type it
  does not map panics naming it. Precision, timezone and nullability are not compared.
- `device_divergence`, not `schema_divergence`: `executor/errors.rs:22` owns that name.
- Chain E has merged before this task starts: `keyed`/`hash_join_keyed` in
  `join_dimension_cases.rs` are on master. If they are not, the join file waits and the
  developer says so in the detail file.
- No fix. A red case gets a ticket (fifteen lines at most) and becomes a `bug_` asserting the
  divergence string.
- Names say node and type: `a_<node>_over_<type>_declares_the_state_the_device_holds`.
- `rustfmt`; commits at most 10 lines; device cycles foreground, one per family group.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/test_support/device_schema.rs` (new), `test_support/mod.rs` | `TypeId`, `DeviceType`, `DeviceSchema`, `device_type_of`, `device_schema_of`, `device_divergence`, `from_ipc_schema`, `schema_of` (`not(rust-only)`); unit tests |
| `peacockdb-core/src/tests/gpu_tests/device.rs` | `Device::schema_of(&GpuBatch)` delegating |
| `peacockdb-core/src/tests/gpu_tests/{source,exec,accumulate,emit,join,nested,aggregate}_schema_cases.rs` (new), `gpu_tests/mod.rs` | the suite |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | `Walk::on_call`; `Session::schema_of`; the spot-checks |

---

### Task 1: The ABI function — done by task 2

`peacock_handle_schema` and its two gtests landed with `decimal-precision-at-export`'s header
rebuild. Confirm before anything else: `grep -n peacock_handle_schema cpp/include/peacock_gpu.h
peacockdb-ffi/src/lib.rs` shows both, and `git log --oneline -1 -S peacock_handle_schema` names
that task's commit. If not, stop and say so in the detail file: this task does not move the
header.

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
pub fn device_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String>;
#[cfg(not(feature = "rust-only"))]
pub fn schema_of(batch: &GpuBatch) -> DeviceSchema;               // peacock_handle_schema on the batch's executor + handle
```
  The module sits in `test_support/mod.rs` beside `corpus_gpu` with the same gating: the
  feature-independent part always, `schema_of` under `not(rust-only)`.

- [ ] **Step 1: Unit tests** in the same file (`#[cfg(test)] mod tests`):
  `utf8_and_large_utf8_are_one_string` (`Utf8`, `LargeUtf8` → `String`, no scale);
  `a_decimal_keeps_its_scale_and_loses_its_precision` (`Decimal128(15,2)` and `(38,2)` →
  `{Decimal128, Some(2)}`, equal); `date32_is_timestamp_days`; `timestamps_keep_the_unit_and_lose_the_zone`;
  `integers_and_floats_by_width`; `boolean_is_bool8`; `null_is_empty`;
  `an_unmapped_type_panics_by_name` (`Interval`, `#[should_panic(expected = "Interval")]`);
  `divergence_names_every_differing_column_in_the_sinks_spelling` — declared `[a Int32, b
  Utf8, c Decimal128(15,2)]` vs actual `[a Int16, b String, c Decimal128 s=4]` →
  `Some("0 a: Int32 vs INT16; 2 c: Decimal128(15, 2) vs DECIMAL128 scale 4")`;
  `a_matching_schema_diverges_nowhere`; `a_column_count_mismatch_is_a_divergence`. All
  rust-only: nothing here touches a handle.
- [ ] **Step 2: Red** — module missing.
- [ ] **Step 3: Implement** — `device_type_of` as one `match` mirroring
  `third_party/cudf/cpp/src/interop/arrow_utilities.cpp:30-70` for the types the wire admits
  (the vendored tree is 25.10; its view and DECIMAL32/64 arms have no wire type after task 2,
  so 25.02's mapping is the same set), with a comment pointing there; `from_ipc_schema` =
  `device_type_of` over the decoded fields (the export's `utf8` is `String`, its
  `decimal128(_, s)` keeps `s`); `device_divergence` compares length, then position by
  position name and `DeviceType`, formatting `"{at} {name}: {declared arrow type} vs
  {actual}"` joined by `"; "`, `None` when equal. `Display` for `DeviceType` prints `INT16`,
  `STRING`, `DECIMAL128 scale 2`, `TIMESTAMP_DAYS`. `schema_of(&GpuBatch)`: the batch's
  executor pointer and handle → `peacock_handle_schema` → `StreamReader` over the buffer for
  its `schema()` → `from_ipc_schema` → `peacock_result_free`; does not consume the batch.
- [ ] **Step 4: Green**, rust-only `--lib -- test_support::device_schema`; `test_module_layout`.
- [ ] **Step 5: Commit.** `git commit -m "test_support::device_schema: an arrow schema projected onto what cuDF stores"`.

### Task 3: `Device::schema_of` and `Session::schema_of`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/device.rs` (after `fetch`), `wire/gpu_tests/mod.rs` `Session` (after `export`, `:163`)

- [ ] **Step 1:** `pub(crate) fn schema_of(&self, batch: &GpuBatch) -> DeviceSchema` on
  `Device` delegates to `test_support::device_schema::schema_of(batch)`. The walk's
  `Session::schema_of(handle: u64)` takes a raw handle: it calls `peacock_handle_schema` on
  its own executor and shares the decode through a `pub fn from_ipc_bytes(&[u8]) ->
  DeviceSchema` in `device_schema.rs` that `schema_of` also uses.
- [ ] **Step 2: First device case**, in a new `source_schema_cases.rs`: a `Given` leaf of every
  fixture column type uploaded, `schema_of(upload)` compared with `device_schema_of(&schema)`
  — `device_divergence == None`. This proves the reader against the uploader.
- [ ] **Step 3:** Device cycle `PCK_TEST_FILTER='schema_cases'`: green.
- [ ] **Step 4: Commit.** `git commit -m "Device::schema_of reads what the device holds at a handle"`.

### Task 4: The suite, family by family

**Files:**
- Create: `exec_schema_cases.rs`, `accumulate_schema_cases.rs`, `emit_schema_cases.rs`, `join_schema_cases.rs`, `nested_schema_cases.rs`, `aggregate_schema_cases.rs`; extend `source_schema_cases.rs`
- Modify: `gpu_tests/mod.rs` (the `mod` lines); shared builders in the existing files become `pub(super)` where a schema case needs them (visibility only)

One shape for every case: run the node on the device alone (chain E's Task 8 retired
`run_gpu`; factor the device half of `run_both` out as `device_outputs(node, script) ->
Vec<GpuBatch>` in `script.rs`, a builder not a mechanism), then for every output batch
`assert_eq!(device_divergence(node.kind().schema(), &device.schema_of(&batch)), None)`. A
case that fails is rewritten as `bug_…` asserting the divergence string, with its ticket
above.

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
- [ ] **Step 5: the union's branches** — a `GpuUnion` is a forwarder with no executor
  (`coverage.rs:37`) and cannot run alone. The case `union.cpp`'s deleted comment described —
  two branches whose same-named column differs in cuDF type (`Decimal128(15,2)` against an
  `Int64(0)` literal) — is two `GpuProject` cases in `exec_schema_cases.rs`, each casting its
  branch's column to the union's declared type and checked on the device; the concatenation
  itself is the corpus's to prove.

### Task 5: The walk spot-checks

**Files:**
- Modify: `peacockdb-core/src/wire/gpu_tests/mod.rs:233-300` (`Walk`, `make`), the test section

- [ ] **Step 1: The hook.** `Walk` gains `on_call: Option<&'a mut dyn FnMut(Seq, FbKind,
  &[u64], &Session)>`; `make` (`:285-299`) calls it after `self.session.execute(...)` with the
  returned handles. `assert_walk_matches_datafusion(sql, knobs)` gains a sibling
  `walk_with(sql, knobs, hook)` that installs it and still compares the final batches.
- [ ] **Step 2: The eleven tests**, each named for what it reads, each a `walk_with` whose
  hook matches `(seq, kind)` on the call the spec's table names, reads
  `session.schema_of(handle)` and asserts the literal `DeviceSchema`. `FbKind::Aggregate` is
  `{ merge: bool }` (`merge: false` is the partial, `true` the merge), not a mode enum:

```rust
#[tokio::test]
async fn an_avg_partial_holds_a_string_key_a_scale_2_sum_and_an_int64_count() {
    let seen = walk_with(AVG_BY_FLAG, ONE_LANE, |_, kind, handles, session| {
        if let FbKind::Aggregate { merge: false } = kind {
            assert_eq!(session.schema_of(handles[0]), DeviceSchema(vec![
                ("l_returnflag".into(), DeviceType { id: TypeId::String, scale: None }),
                ("avg(lineitem.l_quantity)$sum".into(), DeviceType { id: TypeId::Decimal128, scale: Some(2) }),
                ("avg(lineitem.l_quantity)$count".into(), DeviceType { id: TypeId::Int64, scale: None }),
            ]));
        }
    }).await;
    assert_eq!(times(&seen, FbKind::Aggregate { merge: false }), 1);
}
```

  The state column order is `aggregates.rs:64`'s (`[$sum, $count]` for `avg`) and the names
  come from the plan golden for that query (the `schema=[…]` of the aggregate node); read
  them, do not invent them. **Write every expectation before the first device cycle**, from
  the plan golden's declared types and DataFusion's typing of the same expression, and commit
  the eleven tests red-or-green-unknown before running them: that is what keeps the
  expectation the contract rather than the device's habit. When a check then fails, do not
  edit the expectation to match — rename the test `bug_…`, assert what the device holds,
  open a ticket saying what it should hold and why (cite the declared type at the nearest
  declared node), and correct the spec's table row to state both. Record in the detail file,
  per check, where its expectation came from. The rest of the table:
  `AVG_BY_FLAG` merge and finalize; `SUM_BY_FLAG` partial; `ROLLUP`'s grouping-set aggregate
  as `bug_…_holds_an_int32_grouping_id_where_the_plan_says_uint8` naming #65;
  `PROJECT_OVER_FILTER` after filter and after project; `INNER_JOIN` after the join;
  `MAX_OF_SUMS` inner finalize and outer max; `SEMI_JOIN` after its one call. A check whose
  device answer differs from the table's expectation is a `bug_` with a ticket, and the detail
  file records the correction to the table.
- [ ] **Step 3:** Device cycle `PCK_TEST_FILTER='wire::gpu_tests'`: the existing nine green,
  the spot-checks green or pinned as `bug_` with tickets — never green by a rewritten
  expectation. The reviewer's question for each: was this expectation derivable without the
  device? If the detail file cannot say where it came from, the check is sent back.
- [ ] **Step 4: Commit.** `git commit -m "walk spot-checks: what the device holds between calls, read by hand"`.

### Task 6: The record

- [ ] `build-test.md`: the suite's row per family and the walk's, counts; the `bug_` table
  gains every new pin with its ticket. Detail file: every device run's `test result:` lines,
  the projection table, every ticket opened.
- [ ] `git commit -m "device-schema-harness: the record"`.
