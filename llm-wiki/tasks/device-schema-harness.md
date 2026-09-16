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

1. **Interop.** `peacock_handle_schema(executor, handle, out_ipc, out_len)` — the IPC
   *schema message* alone for the handle's table — already exists: `decimal-precision-at-export`
   declared it on the chain's one header rebuild, with its two gtests, and nothing in Rust
   calls it until here.
2. **The reduced schema**, in `peacockdb-core/src/test_support/device_schema.rs`, under the
   `test-support` feature like its neighbours and `#[cfg(not(feature = "rust-only"))]` for
   the part that touches a handle:

       pub struct DeviceType { pub id: TypeId, pub scale: Option<i32> }   // cuDF's data_type
       pub struct DeviceSchema(pub Vec<(String, DeviceType)>);
       pub fn device_type_of(arrow: &DataType) -> DeviceType;             // the projection
       pub fn device_schema_of(declared: &Schema) -> DeviceSchema;
       pub fn device_divergence(declared: &Schema, actual: &DeviceSchema) -> Option<String>;
       pub fn schema_of(batch: &GpuBatch) -> DeviceSchema;                // not(rust-only)

   `TypeId` mirrors `cudf::type_id` for the types the wire admits. `device_type_of` is the
   fixed mapping cuDF's interop implements for those types — identical in 25.02 and the
   vendored 25.10, which differ only in view and narrow-decimal arms the wire no longer has:
   `Utf8/LargeUtf8 → STRING`, `Decimal128(_, s) → DECIMAL128/s`, `Date32 → TIMESTAMP_DAYS`,
   `Timestamp(unit, _) → TIMESTAMP_<unit>`, integers and floats by width, `Boolean → BOOL8`,
   `Null → EMPTY`; a type with no cuDF image panics naming it. `device_divergence` (named apart
   from `executor/errors.rs`'s `schema_divergence`) compares by position, name and
   `DeviceType`, and reports every diverging column in the sink's spelling
   (`3 o_year: Int32 vs INT16`). Precision, timezone and nullability are outside the projection
   on purpose: after tasks 1–2 they are labels the export carries, not facts the device holds.
   `schema_of` calls `peacock_handle_schema` on the batch's executor and handle (a `GpuBatch`
   carries both), decodes the schema through `arrow::ipc` and projects. It lives in
   `test_support`, not under `src/tests/`' `cfg(test)`, because the corpus binary and the
   next task's validator must see it.
3. **Reading a handle from the harness**: `Device::schema_of(&GpuBatch)` in
   `tests/gpu_tests/device.rs` and the walk's `Session::schema_of(handle)` both delegate to
   `test_support::device_schema::schema_of`.

## The cases

**Every existing case stays as it is.** No green case gains an assertion; no `bug_` case
changes what it pins. Every schema check is a new, separately named case, so the count grows and
nothing is retyped. A new case that is red is a `bug_` with a ticket naming the producer.

- **Operator harness**, one new file per family beside the existing ones
  (`src/tests/gpu_tests/<family>_schema_cases.rs`): for each node kind the family drives, a case
  that runs the node on the device alone and asserts `device_divergence(declared,
  schema_of(output handle)) == None` — join (each type, with and without a projection, on each
  key type, reusing join-cases' `keyed`/`hash_join_keyed` builders, which chain E merges
  before this chain starts), aggregate (init, merge, finalize; `sum`, `count`, `min`, `max`,
  `avg`, `stddev`; grouped and global), project (arithmetic on each numeric type, each cast the
  corpus uses, each scalar function the dispatch admits), filter, coalesce-all, emit (each key
  type), sort, scan (each parquet column type of the fixtures). A union is a forwarder with no
  executor (`coverage.rs:37`) and cannot be run alone: the case `union.cpp`'s deleted block
  described — two branches whose same-named column differs in cuDF type — is two per-branch
  `GpuProject` casts, each checked. Names say the node and the type:
  `a_sum_over_a_decimal_declares_the_state_the_device_holds`.
- **Walk spot-checks**, in `wire/gpu_tests/mod.rs`: `Walk::make` takes an `on_call(seq, kind,
  &[handle], &Session)` hook; each check is a named test that drives one of the file's queries
  with a hook that reads `schema_of` after one call and asserts a literal `DeviceSchema`.
  Intermediates have no declaration, so each expectation is written by hand with its reason;
  the state column order and names come from the query's plan golden (`aggregates.rs:64`
  orders `avg`'s state `[$sum, $count]`):

  | query | after | expected |
  |---|---|---|
  | `AVG_BY_FLAG` | `CudfAggregate{Partial}` | `[l_returnflag STRING, avg$sum DECIMAL128 s=2, avg$count INT64]` |
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

  Eleven checks. A spot-check whose device answer differs from the table is a `bug_` with a
  ticket, and the table's row is corrected to say what the device holds and why that is wrong.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/test_support/device_schema.rs` (new), `test_support/mod.rs` | the reduced schema, the comparator, `schema_of`, with unit tests of the projection |
| `peacockdb-core/src/tests/gpu_tests/device.rs`, `*_schema_cases.rs` (new) | `Device::schema_of` delegating; the suite |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the hook; the spot-checks |
| `llm-wiki/build-test.md`, `tickets.md`, `active-tickets.md` | the suite's rows; every new `bug_`'s ticket |

Component-level API: `test_support::device_schema` (under the `test-support` feature). No ABI
or wire change — `peacock_handle_schema` arrived with task 2. No production Rust or C++
behaviour changes.

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
