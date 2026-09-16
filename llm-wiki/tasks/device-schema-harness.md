# What the device holds, read at the node

Kind: production

**Testing only; closes no ticket and fixes nothing.** A helper that reads a cuDF table's schema
into Rust, a comparator that holds it against the node's declared schema under the projection
cuDF can express, and the cases that use them — at every operator family and at chosen calls
inside the recipe walk. Fifth of chain B, after the four fixes, which is why its comparisons can
be exact: after them a red case is a defect, not a known class. A simpler replacement for the
rejected `declared-schemas`, which measured per call against declarations that had to be
invented; this measures against the declarations the plan already has.

## The helpers

1. **Interop.** One new ABI function, read-only:

       int peacock_handle_schema(peacock_executor_t*, uint64_t handle,
                                 uint8_t** out_ipc, uint64_t* out_len);

   the IPC *schema message* alone for the handle's table (`to_arrow_schema` on its view, no
   rows, freed with `peacock_result_free`). `cpp/include/peacock_gpu.h`, `gpu_executor.cpp`
   beside `peacock_result_from_handle`, `peacockdb-ffi/src/lib.rs`.
2. **The reduced schema**, in `peacockdb-core/src/test_support/device_schema.rs`:

       pub struct DeviceType { pub id: TypeId, pub scale: Option<i32> }   // cuDF's data_type
       pub struct DeviceSchema(pub Vec<(String, DeviceType)>);
       pub fn device_type_of(arrow: &DataType) -> DeviceType;             // the projection
       pub fn device_schema_of(declared: &Schema) -> DeviceSchema;
       pub fn schema_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String>;

   `TypeId` mirrors `cudf::type_id` for the types the wire admits. `device_type_of` is the
   fixed mapping `arrow_utilities.cpp` implements: `Utf8/LargeUtf8 → STRING`,
   `Decimal128(_, s) → DECIMAL128/s`, `Date32 → TIMESTAMP_DAYS`, `Timestamp(unit, _) →
   TIMESTAMP_<unit>`, integers and floats by width, `Boolean → BOOL8`, `Null → EMPTY`; a type
   with no cuDF image panics naming it. `schema_divergence` compares by position, name and
   `DeviceType`, and reports every diverging column in the sink's spelling
   (`3 o_year: Int32 vs INT16`). Precision, timezone and nullability are outside the projection
   on purpose: after tasks 1–2 they are labels the export carries, not facts the device holds.
3. **Reading a handle**: `Device::schema_of(handle) -> DeviceSchema` in
   `tests/gpu_tests/device.rs`, decoding the IPC schema through `arrow::ipc` and projecting.
   The walk's `Session` gets the same method.

## The cases

**Every existing case stays as it is.** No green case gains an assertion; no `bug_` case
changes what it pins. Every schema check is a new, separately named case, so the count grows and
nothing is retyped. A new case that is red is a `bug_` with a ticket naming the producer.

- **Operator harness**, one new file per family beside the existing ones
  (`src/tests/gpu_tests/<family>_schema_cases.rs`): for each node kind the family drives, a case
  that runs the node on the device alone and asserts `schema_divergence(declared,
  schema_of(output handle)) == None` — join (each type, with and without a projection, on each
  key type), aggregate (init, merge, finalize; `sum`, `count`, `min`, `max`, `avg`, `stddev`;
  grouped and global), project (arithmetic on each numeric type, each cast the corpus uses, each
  scalar function the dispatch admits), filter, coalesce-all, emit (each key type), sort, union
  (two branches of differing cuDF types — the case `union.cpp`'s deleted block described),
  scan (each parquet column type of the fixtures). Names say the node and the type:
  `a_sum_over_a_decimal_declares_the_state_the_device_holds`.
- **Walk spot-checks**, in `wire/gpu_tests/mod.rs`: `Walk::make` takes an `on_call(seq, kind,
  &[handle])` hook; each check is a named test that drives one of the file's queries with a hook
  that reads `schema_of` after one call and asserts a literal `DeviceSchema`. Intermediates have
  no declaration, so each expectation is written by hand with its reason:

  | query | after | expected |
  |---|---|---|
  | `AVG_BY_FLAG` | `CudfAggregate{Partial}` | `[l_returnflag STRING, avg$count INT64, avg$sum DECIMAL128 s=2]` |
  | `AVG_BY_FLAG` | `CudfAggregate{Merge}` | the same |
  | `AVG_BY_FLAG` | the finalize `CudfProject` | `[STRING, DECIMAL128 s=6]` |
  | `SUM_BY_FLAG` | `CudfAggregate{Partial}` | `[STRING, DECIMAL128 s=2]` |
  | `ROLLUP` | the grouping-set aggregate | `[STRING, STRING, INT32, DECIMAL128 s=2]` — a `bug_` naming [#65](../tickets.md#t65): the plan says `UInt8` |
  | `PROJECT_OVER_FILTER` | `CudfFilter` | `[c_custkey INT64, c_nationkey INT64]` |
  | `PROJECT_OVER_FILTER` | `CudfProject` | `[doubled INT64]` |
  | `INNER_JOIN` | the join call | `[n_name STRING, r_name STRING]` plus the keys the recipe keeps, in recipe order |
  | `MAX_OF_SUMS` | the inner finalize | `[per_flag DECIMAL128 s=2]` |
  | `MAX_OF_SUMS` | the outer `max` | `[DECIMAL128 s=2]` |
  | `SEMI_JOIN` | the single join call | `[c_custkey INT64]` |

  A spot-check whose device answer differs from the table is a `bug_` with a ticket, and the
  table's row is corrected to say what the device holds and why that is wrong.

## Scope

| file | change |
|---|---|
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | `peacock_handle_schema` |
| `peacockdb-ffi/src/lib.rs` | extern |
| `peacockdb-core/src/test_support/device_schema.rs` (new), `test_support/mod.rs` | the reduced schema and comparator, with unit tests of the projection |
| `peacockdb-core/src/tests/gpu_tests/device.rs`, `*_schema_cases.rs` (new) | `schema_of`; the suite |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the hook; the spot-checks |
| `llm-wiki/build-test.md`, `tickets.md`, `active-tickets.md` | the suite's rows; every new `bug_`'s ticket |

Component-level API: one ABI symbol added; `test_support::device_schema` (test-only). No wire
change. No production Rust or C++ behaviour changes.

## Restriction

No fix rides along: a red case is a ticket. No existing case is edited. The comparator projects
exactly what cuDF stores — a wider comparison belongs to the sink, a narrower one hides bugs.

## Verification bar

- rust-only: `--lib` (the projection's unit tests), `test_module_layout` (the new files are
  where the layout rules put them).
- device: the whole harness, old cases untouched and green, new suite green or pinned; the
  walk file green with the spot-checks.
- `build-test.md`'s counts grow by the new cases exactly.

## Device workflow

`build-test-shadgpu.sh`; expect two or three cycles, since the suite is written family by
family.
