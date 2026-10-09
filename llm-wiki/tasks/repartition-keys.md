# The shuffle hashes every key type the planner emits, by the rule the cpu uses

Kind: production

**This task closes [#201](../archive/archived-tickets.md#t201) (the murmur gate proves a copy of
the lane rule, not the rule), [#206](../archive/archived-tickets.md#t206) (a float or boolean
partition key is refused on the device), [#240](../archive/archived-tickets.md#t240) (a timestamp
partition key is refused on the device), [#95](../archive/archived-tickets.md#t95) (a decimal
partition key is refused on the device) and [#189](../archive/archived-tickets.md#t189) (the
shuffle cannot hash a rollup's grouping-set id).** It also makes unsigned keys shuffle on both
engines (the review's row 11; no ticket). Fourth of the join-rewrite chain: the joins
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
Comet has no unsigned arm at all, nor has the kernel, so any unsigned key across a shuffle is
refused at run time on both engines; the tp1 modes, which do not shuffle, answer.

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
   bits, -0.0 as +0.0, and **every NaN as one canonical bit pattern** — comet hashes a NaN by its
   bits (`utils.rs:78-105`), which would put `NaN` and `-NaN` (x86's `0/0`) in different lanes at
   tp4 while tp1 equates them on the device; so `rows_per_lane` canonicalizes NaN before calling
   comet and the kernel does the same, the decision D2 took for decimals: the engines agree, not
   Spark); Timestamp in all four units (as i64);
   Decimal128 (16 LE bytes of the unscaled `int128`, matching step 3's cpu cast). Int8, a zero-row input and an all-NULL
   key get gates too — the survey found them ungated.
4b. **Unsigned keys, one rule on both engines.** Spark has no unsigned types, so there is no
   placement to match: UInt8 and UInt16 cast to Int32, UInt32 to Int64 (value casts), UInt64
   reinterpreted as Int64 bits, then hashed as that signed type is — by `hash_keys` before comet
   and by the kernel's normalizing switch. A live gate per width, values past the signed maximum
   included.
5. **Operator harness cases** (`tests/gpu_tests/emit_cases.rs`, in-memory batches): one per key
   type, both backends placing every row in the same lane — Int8, Int16, Int32, Int64, Float32,
   Float64 (each with -0.0, 0.0, NaN, -NaN and NULL), Boolean, Date32, Timestamp in four units,
   Decimal128 at p ≤ 18 and p > 18, Utf8, UInt8, UInt16, UInt32, UInt64, a mixed composite; `emit_schema_cases.rs` the same types
   held as declared. `tests/synthetic.rs` gains `key_types(rows, seed)`, one column per hashable
   type with those special values, which join-backend's join cases reuse. The `bug_` pins for
   float, boolean (#206) and decimal (#95) flip.
6. **Timestamps on the wire.** The fbs `DataType` enum gains `TimestampSecond`,
   `TimestampMillisecond`, `TimestampMicrosecond` and `TimestampNanosecond` (an append), with the
   Rust `convert_data_type` mapping from Arrow `Timestamp(unit, _)`, their `fb_text` rendering, and
   the C++ `fb_to_type_id` arms. Today a timestamp reaches the device only because no node reads
   its type from the wire; a cast to a timestamp type (pbench's `timestamp-s-key-group`) cannot be
   written at all. join-session-cpp's pad and absent-side schemas consume these variants.
   pbench's `timestamp-s-key-group`, declared not runnable on #240, is enabled here.
   Then **an unmapped type is a `PlanError` in every schema**, never the silent `Null` of
   `serialize_schema` (`serialize.rs:136`): the plan golden says "not runnable" naming the column
   and its type ([#249](../tickets/complete-coverage.md#t249)). tpch and tpcds hold no such type, so
   no golden of theirs moves; pbench's two #249 queries are declared not runnable on it.
7. **#243's pins** (open; not fixed here): two `bug_` cases, cpu against device, over float keys
   holding -0.0, 0.0, NaN and -NaN — a float-keyed `GpuAggregate` (cpu six groups, device four)
   and a float-keyed Inner `GpuHashJoin` over one probe batch. Each asserts the divergence.

## Scope

| path | change |
|---|---|
| `cpp/src/spark_hash_partition.cu` | the arms, NaN canonicalized |
| `flatbuffers/gpu_plan.fbs`, `peacockdb-core/src/wire/serialize.rs`, `plan_text/`, `cpp/src` (`fb_to_type_id`) | the timestamp `DataType` variants |
| `testdata/goldens/recipe-payloads.txt` | only if a corpus payload carries a timestamp (none does today); regenerated if so |
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs` | the decimal cast, the unsigned widening and the NaN canonicalization before comet |
| `peacockdb-core/src/wire/serialize.rs` and its callers in `wire/`, `wire/expr_writer.rs` | #249: an unmapped type a `PlanError` in every schema; `data_type` cites `(#249)` |
| `peacockdb-core/src/planner/tests/plan_goldens.rs` | `NOT_RUNNABLE`: `timestamp-s-key-group` leaves; the three #249 pbench queries enter |
| `peacockdb-core/src/wire/tests.rs`, `planner/translator/tests.rs`, `cpp/tests/gpu/test_plan_executor.cpp` | the timestamp types, the #249 pin, #189's planner test, the timestamp cast gtest |
| `peacockdb-core/src/planner/translator/aggregate.rs` | #189 |
| `peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs` | #201; the new gates |
| `peacockdb-core/src/tests/synthetic.rs`, `tests/gpu_tests/emit_cases.rs`, `emit_schema_cases.rs`, `aggregate_cases.rs`, `join_cases.rs` | the cases and pins |
| `cpp/tests/gpu/test_cudf.cpp` | the hardcoded comet ids, a third copy of the rule: replaced by a case read from the gate's vectors, or deleted with the reason |
| `testdata/cost-registry.csv`, `corpus_cases.inc`, `llm-wiki/` | cells, tags, counts, #95/#189/#201/#206/#240 archived |

Component-level API: the fbs `DataType` enum gains four timestamp variants (an append; no C ABI
symbol, no `partitioning.hpp` change). The cpu's lane rule changes for decimal keys (16 bytes) and
NaN float keys (one canonical NaN).

## Restriction

Key types and the lane rule only. No float equality change on the cpu: #243 stays open, reworded
by this task to the cpu's equality alone (`-0.0 ≠ 0.0` and `NaN ≠ -NaN` in DataFusion's compare
and hash), since the lane split it also described is fixed here. No join change, no change to
which keys DataFusion picks except #189's drop.

## Registry

#189's 15 cpu cells (tp4 × 3: tpch rollup_over_join; tpcds q5, q18, q22, q80) turn on, compared
with DuckDB. #95's eight rows lose `95` where it is their last ticket on a cell (most wait on
#152 too). pbench's key-type rows' tp4 device cells turn on where they pass, except the float
rows, commented out on #243; `timestamp-s-key-group` comes off `NOT_RUNNABLE` and its cells turn
on where they pass; `uint-key-group` and `uint-key-join` turn on at tp4 on both engines.

## Verification bar

- rust-only: the planner test for #189; `test_cpu_corpus` with the 15 cells and their DuckDB cases.
- device: every live gate red before its arm and green after, a NaN and a -NaN key in one lane;
  the emit cases and schema cases; the two #243 pins red as pins.
- rust-only: a timestamp cast serializes and renders; the plan golden for `timestamp-s-key-group`
  is runnable.

## Device workflow

`build-test-shadgpu.sh`; a cycle per arm group (bool and float, timestamp, decimal), then one for
the corpus cells.
