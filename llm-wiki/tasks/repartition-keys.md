# The shuffle hashes every key type the planner emits, by the rule the cpu uses

Kind: production

**This task closes [#201](../tickets/corpus-coverage.md#t201) (the murmur gate proves a copy of
the lane rule, not the rule), [#206](../tickets/corpus-coverage.md#t206) (a float or boolean
partition key is refused on the device), [#240](../tickets/corpus-coverage.md#t240) (a timestamp
partition key is refused on the device), [#95](../tickets/corpus-coverage.md#t95) (a decimal
partition key is refused on the device) and [#189](../tickets/corpus-coverage.md#t189) (the
shuffle cannot hash a rollup's grouping-set id).** Fourth of the join-rewrite chain: the joins
that follow shuffle on keys of these types.

## Why it happens

The device kernel's key switch (`cpp/src/spark_hash_partition.cu:144-185`) hashes STRING, INT32
and INT64, after narrowing INT8/16 and bit-casting TIMESTAMP_DAYS; every other type falls to
`CUDF_FAIL "unsupported key column cuDF type_id=N"` (`:179`). Comet's hasher, the cpu's lane rule
(`executor/cpu_backend/spark_partitioning.rs`), takes Boolean as i32, floats by their bits with
-0.0 as 0, every timestamp unit as i64, and Decimal128 by declared precision — 8 LE bytes of the
unscaled value at p ≤ 18, 16 above (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs`).
cuDF's `data_type` has no precision, and the loader widens every decimal to Decimal128, so the
kernel cannot pick comet's width itself. The gate
that proves the kernel matches comet (`cpu_backend/gpu_tests/murmur_conformance.rs`) proves it
against its own `cpu_partition_ids`, not the production `rows_per_lane` (#201). The rollup shuffle
copies DataFusion's keys, `__grouping_id: UInt8` among them, which comet has no arm for (#189).

## The work

1. **#201 first.** The gate calls production `rows_per_lane` over the same columns and compares
   lane by lane; its `cpu_partition_ids` and local `pmod` go, and `pmod_handles_negative_hashes`
   tests the production `pmod`. Every new type below is proved against the real rule.
2. **#189** (corpus-fixes fix 2): `aggregate_sequence` drops the grouping id from a rollup's
   shuffle keys (`keys.retain(...)` before the shuffle, guarded on `!group.is_single()`), so it is
   never hashed. Correct because hashing a subset of the Final group's keys still puts every row
   of a group on one lane, and the validator's subset rule (`plan/aggregate.rs:236-256`) already
   admits it; the cost is a little more skew where grouping sets share visible key values. A key
   at or past `group.expr().len()` left after the drop, or an empty key list, is refused. Planner
   only; a planner test asserts `hash_keys` at tp4.
3. **Decimals hash 16 bytes on both engines** (decision D2, taken: the cpu's rule changes, not the
   wire). The lane rule must make the cpu and the device agree (`architecture.md`, "Rehash and the
   comet hash"); comet is the shared implementation of Spark's murmur3, not a promise of Spark's
   placement, and nothing reads our lane numbers from outside. So `rows_per_lane` casts each
   decimal key to `Decimal128(38, s)` before calling comet, which then always hashes the 16 LE
   bytes of the unscaled value, and the kernel hashes the same 16 bytes of the `int128` (widening
   a DECIMAL32/64 should one arrive). No wire field, no `partitioning.hpp` signature change, no FFI
   change; a decimal's lane no longer matches Spark's at p ≤ 18. A scale drift at the emit's input
   is the schema validator's to catch, where the line enables it.
4. **Kernel arms**, each red first as a live gate: Boolean (as INT32); Float32 and Float64 (their
   bits, -0.0 as +0.0, NaN by its bits, as comet does); Timestamp in all four units (as i64);
   Decimal128 (16 LE bytes of the unscaled `int128`, matching step 3's cpu cast). Int8, a zero-row input and an all-NULL
   key get gates too — the survey found them ungated.
5. **Operator harness cases** (`tests/gpu_tests/emit_cases.rs`, in-memory batches): one per key
   type, both backends placing every row in the same lane — Int8, Int16, Int32, Int64, Float32,
   Float64 (each with -0.0, 0.0, NaN, -NaN and NULL), Boolean, Date32, Timestamp in four units,
   Decimal128 at p ≤ 18 and p > 18, Utf8, a mixed composite; `emit_schema_cases.rs` the same types
   held as declared. `tests/synthetic.rs` gains `key_types(rows, seed)`, one column per hashable
   type with those special values, which join-backend's join cases reuse. The `bug_` pins for
   float, boolean (#206) and decimal (#95) flip.
6. **#243's pins** (open; not fixed here): two `bug_` cases, cpu against device, over float keys
   holding -0.0, 0.0, NaN and -NaN — a float-keyed `GpuAggregate` (cpu six groups, device four)
   and a float-keyed Inner `GpuHashJoin` over one probe batch. Each asserts the divergence.

## Scope

| path | change |
|---|---|
| `cpp/src/spark_hash_partition.cu` | the arms |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs` | the decimal cast before comet |
| `peacockdb-core/src/planner/translator/aggregate.rs` | #189 |
| `peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs` | #201; the new gates |
| `peacockdb-core/src/tests/synthetic.rs`, `tests/gpu_tests/emit_cases.rs`, `emit_schema_cases.rs`, `aggregate_cases.rs`, `join_cases.rs` | the cases and pins |
| `cpp/tests/gpu/test_cudf.cpp` | the hardcoded comet ids, a third copy of the rule: replaced by a case read from the gate's vectors, or deleted with the reason |
| `testdata/cost-registry.csv`, `corpus_cases.inc`, `llm-wiki/` | cells, tags, counts, #95/#189/#201/#206/#240 archived |

Component-level API: none. No wire, fbs, C ABI or `partitioning.hpp` change; the cpu's lane rule
changes for decimal keys only.

## Restriction

Key types and the lane rule only. No float normalization (#243 stays open), no join change, no
change to which keys DataFusion picks except #189's drop.

## Registry

#189's 15 cpu cells (tp4 × 3: tpch rollup_over_join; tpcds q5, q18, q22, q80) turn on, compared
with DuckDB. #95's eight rows lose `95` where it is their last ticket on a cell (most wait on
#152 too). pbench's key-type rows' tp4 device cells turn on where they pass, except the float
rows, commented out on #243.

## Verification bar

- rust-only: the planner test for #189; `test_cpu_corpus` with the 15 cells and their DuckDB cases.
- device: every live gate red before its arm and green after; the emit cases and schema cases;
  the two #243 pins red as pins.

## Device workflow

`build-test-shadgpu.sh`; a cycle per arm group (bool and float, timestamp, decimal), then one for
the corpus cells.
