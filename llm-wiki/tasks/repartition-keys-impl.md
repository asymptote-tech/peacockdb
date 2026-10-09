# repartition-keys implementation plan

> **For agentic workers:** the chain coordinator (`ensemble`) runs this plan task by task through
> the developer, with a reviewer per round. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** the shuffle hashes every key type the planner emits — Boolean, Int8, Float32/64,
Timestamp in four units, Decimal128 — by one rule on both engines, proved against the production
lane rule; the rollup's grouping id is never hashed. Closes #201, #189, #206, #240, #95.

**Architecture:** #201 first, so every new gate proves the kernel against `rows_per_lane`, not a
copy. #189 is planner only. Each kernel arm is written red-first as a live gate in
`murmur_conformance.rs`, then the arm. Decimals hash the 16 LE bytes of the unscaled value on
both engines: the cpu casts a decimal key to `Decimal128(38, s)` before comet, the kernel hashes
the `int128`. Then the operator-harness cases for every key type, and #243's two pins.

The wire's `DataType` gains the four timestamp types (Task 5b), and every NaN hashes as one
canonical NaN on both engines (Task 4).

**Tech stack:** CUDA C++ (`spark_hash_partition.cu`), Rust (cpu lane rule, planner, harness),
comet 0.6.0's murmur3, the shad-gpu device cycle.

**Spec:** [`repartition-keys.md`](repartition-keys.md) — frozen (`9e563348`). Design §5.3.

## Global constraints

- The lane rule is "the cpu and the device agree" (`architecture.md`, "Rehash and the comet
  hash"); comet is the shared implementation, not a promise of Spark's placement.
- Decimals: 16 LE bytes of the unscaled `int128` on both engines, via the cpu-side cast to
  `Decimal128(38, s)`. No C ABI or `partitioning.hpp` signature change.
- Floats: every NaN is hashed as **one canonical bit pattern** on both engines (`f64` `0x7ff8000000000000`,
  `f32` `0x7fc00000` — Rust's `f64::NAN`/`f32::NAN`): the cpu maps NaNs before comet, the kernel in
  its float arm; `-0.0` as `+0.0`, as comet already does. So `NaN` and `-NaN` share a lane, and the
  device answers a float-keyed group or join the same at tp1 and tp4. The same "the engines agree,
  not Spark" decision as decimals. #243 keeps only the cpu's *equality* (DataFusion compares and
  groups floats by bits); it is reworded so, and stays open.
- The wire gains the four timestamp types: `DataType` appends `TimestampSecond`,
  `TimestampMillisecond`, `TimestampMicrosecond`, `TimestampNanosecond` (an append — existing
  values keep their numbers); `convert_data_type` maps `Timestamp(unit, _)` to them, and the C++
  `fb_to_type_id` to `TIMESTAMP_{SECONDS,MILLISECONDS,MICROSECONDS,NANOSECONDS}`. pbench's
  `timestamp-s-key-group` (an `arrow_cast` to `Timestamp(Second)`) then crosses the wire and leaves
  `NOT_RUNNABLE`. join-session-cpp reuses these values; it does not add them again.
- Unsigned keys (review row 11, no ticket): one rule on both engines — UInt8/UInt16 cast to Int32,
  UInt32 to Int64 (value casts), UInt64 reinterpreted as Int64 bits — applied by the cpu in
  `hash_keys` before comet and by the kernel's normalizing switch. Spark has no unsigned types, so
  there is no placement to match. pbench's `uint-key-group` and `uint-key-join` turn on at tp4.
- No join change; no change to which keys DataFusion picks except #189's drop.
- Every kernel arm red first as a live gate; every `bug_` pin flips, none is deleted.
- Device cycles run foreground (`scripts/build-test-shadgpu.sh`); sandboxed ssh needs the
  sandbox off. Never share a cargo target dir across worktrees.
- Commit messages at most 10 lines, ending with the co-author trailer.

## Review Focus

1. **A float column of specials only** (every row -0.0, NaN, -NaN or NULL): both engines put every
   -0.0 with +0.0's lane and every NaN, whatever its sign or payload, in one lane — Task 4's gate
   carries a column of nothing else, and `every_nan_shares_a_lane_and_so_do_the_two_zeros` asserts it.
2. **Negative and wide decimals**: two's-complement `int128` bytes for negatives, and p=38 values
   past `i64` — Task 6's gates carry `-1`, `i64::MIN - 1` and `10^37`.
3. **A decimal key composed with a string key**: the seed chains across the 16-byte block into the
   string's — Task 6's composite gate.
4. **A zero-row batch over a key the kernel normalizes** (bool, timestamp, decimal): the
   normalizing cast runs on zero rows and the scatter answers N empty lanes — Task 7's
   `a_zero_row_batch_scatters_on_every_key_type`.
5. **A timestamp with a time zone**: comet matches `Timestamp(unit, _)` for any zone and cuDF
   reads the same `int64`; Task 5's gate includes `Timestamp(Microsecond, Some("UTC"))`.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs` | `pmod` visible to the gate; the decimal cast, the unsigned widening and the NaN canonicalization in `hash_keys` |
| `flatbuffers/gpu_plan.fbs` | `DataType` appends the four timestamp types |
| `peacockdb-core/src/wire/serialize.rs:101-124` | `convert_data_type`'s timestamp arms |
| `cpp/src/expr.cpp:77-97` | `fb_to_type_id`'s timestamp arms |
| `peacockdb-core/src/planner/tests/plan_goldens.rs:442` | pbench's `timestamp-s-key-group` leaves `NOT_RUNNABLE` |
| `peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs` | the gate onto `rows_per_lane`; every new live gate |
| `cpp/src/spark_hash_partition.cu` | the bool, float, timestamp, decimal and unsigned arms |
| `cpp/tests/gpu/test_cudf.cpp` | the two hardcoded-comet-id smoke tests, deleted |
| `peacockdb-core/src/planner/translator/aggregate.rs` | #189's `drop_grouping_id` |
| `peacockdb-core/src/planner/translator/tests.rs` | #189's planner test |
| `peacockdb-core/src/tests/synthetic.rs` | `key_types(rows, seed)`, `key_types_schema()` |
| `peacockdb-core/src/tests/gpu_tests/emit_cases.rs`, `emit_schema_cases.rs` | a case per key type; the three pins flipped |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `join_cases.rs` | #243's pins |
| `testdata/goldens/**`, `testdata/cost-registry.csv`, `peacockdb-core/tests/common/corpus_cases.inc` | #189's goldens and cells |
| `llm-wiki/` | tickets archived, counts, the decimal rule in `architecture.md` |

---

### Task 1: The gate proves the production lane rule (#201)

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/spark_partitioning.rs:25` (`pmod` → `pub(crate)`)
- Modify: `peacockdb-core/src/executor/cpu_backend/gpu_tests/murmur_conformance.rs`
- Delete from: `cpp/tests/gpu/test_cudf.cpp` — `CudfGpu.SparkPartitionIdsMatchCometSingleCol`
  (`:75`) and `CudfGpu.SparkPartitionIdsMatchComet2ColWithNulls` (`:87`) with their helper

**Interfaces:**
- Produces: `fn production_partition_ids(cols: &[(Field, ArrayRef)], lanes: usize) -> Vec<i32>`
  in `murmur_conformance.rs`, which every later gate calls through `assert_gpu_matches_rule_live`.

- [x] **Step 1: Replace the local copy.** Delete `pmod` and `cpu_partition_ids` from the gate and
  add, beside the imports:

```rust
use datafusion::arrow::datatypes::{Field, Schema as ArrowSchema};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::physical_expr::PhysicalExpr;
use datafusion::physical_expr::expressions::Column;

use super::super::spark_partitioning::{pmod, rows_per_lane};

/// The lane of each row under the production rule — `rows_per_lane`, the function the cpu's
/// emitter calls — inverted from per-lane row lists to one id per row, so it compares with the
/// kernel's output position by position.
fn production_partition_ids(cols: &[(Field, ArrayRef)], lanes: usize) -> Vec<i32> {
    let schema = Arc::new(ArrowSchema::new(
        cols.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(),
    ));
    let batch = RecordBatch::try_new(schema, cols.iter().map(|(_, a)| Arc::clone(a)).collect())
        .expect("the gate's columns share a length");
    let exprs: Vec<Arc<dyn PhysicalExpr>> = cols
        .iter()
        .enumerate()
        .map(|(i, (f, _))| Arc::new(Column::new(f.name(), i)) as Arc<dyn PhysicalExpr>)
        .collect();
    let per_lane = rows_per_lane(&batch, &exprs, lanes).expect("the production rule answers");
    let mut ids = vec![-1i32; batch.num_rows()];
    for (lane, rows) in per_lane.iter().enumerate() {
        for row in rows {
            ids[*row as usize] = lane as i32;
        }
    }
    assert!(ids.iter().all(|id| *id >= 0), "every row has a lane");
    ids
}
```

  Rename `assert_gpu_matches_comet_live` to `assert_gpu_matches_rule_live`; its first lines
  become `let comet = production_partition_ids(&cols, n_parts as usize);` (computed before `cols`
  is consumed by the struct export — clone the vector of pairs first). Every existing gate keeps
  its data and calls the renamed helper. `step_i_comet_murmur3_public_api_compiles_and_runs`
  keeps its `spark_compatible_murmur3_hash` half and its array half calls
  `production_partition_ids`; `cpu_reference_2col_partition_ids_for_probe` (a print-only probe
  over the old copy) is deleted. `pmod_handles_negative_hashes` now asserts the imported
  production `pmod`.
- [x] **Step 2: Prove the gate reads production.** Locally change `SEED` in
  `spark_partitioning.rs` to `43`; device cycle with
  `PCK_TEST_FILTER='murmur_conformance'`: every live gate red. Restore `42`; rerun: green. Paste
  both outputs into the detail file. (Before this task the same edit left the gate green — that
  is #201.)
- [x] **Step 3: The C++ copy.** Delete the two `CudfGpu.SparkPartitionIds*` tests and their
  `gpu_partition_ids` helper from `test_cudf.cpp` — hardcoded comet ids, a third copy of the
  rule the live gate now proves against production. The file's header comment says so.
  `build-test.md`'s "cuDF GPU smoke" row drops to its two remaining cases.
- [ ] **Step 4: Commit.** `git commit -m "#201: the murmur gate proves rows_per_lane, not a copy of it"`.

### Task 2: The rollup shuffle never hashes the grouping id (#189)

**Files:**
- Modify: `peacockdb-core/src/planner/translator/aggregate.rs` (above `tree = match shuffle`, `:375`)
- Test: `peacockdb-core/src/planner/translator/tests.rs` (after
  `grouping_sets_expand_at_the_init_and_group_on_the_id_above_it`, `:650`)

**Interfaces:**
- Produces: `fn drop_grouping_id(keys: Vec<u32>, id: u32) -> Result<Vec<u32>, PlanError>` (private
  to `aggregate.rs`).

- [x] **Step 1: The failing planner test.**

```rust
#[tokio::test]
async fn a_rollup_shuffle_hashes_its_keys_and_never_the_grouping_id() {
    // #189: comet has no UInt8 arm, and the id is #65's — the device holds INT32. The Final
    // groups on keys plus the id; hashing the keys alone still lands each group in one lane.
    let tree = translated_at_tp4(
        "SELECT c_nationkey, c_mktsegment, count(*) FROM customer \
         GROUP BY ROLLUP(c_nationkey, c_mktsegment)",
        0,
    )
    .await;
    let emit = find(tree.as_ref(), &|node| {
        matches!(as_node_ref(node), NodeRef::EmitPartitions(_))
    })
    .unwrap_or_else(|| panic!("no shuffle in {}", shape(tree.as_ref())));
    let NodeRef::EmitPartitions(emit) = as_node_ref(emit) else {
        unreachable!()
    };
    assert_eq!(emit.hash_keys, vec![0, 1]);
    validate_all(tree.as_ref());
}
```

- [x] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib a_rollup_shuffle` —
  FAIL: `left: [0, 1, 2], right: [0, 1]`.
- [x] **Step 3: The drop.** In `aggregate.rs`:

```rust
/// A grouping-set aggregate's shuffle hashes its user keys and not `__grouping_id`, which
/// sits right after them (`id`). A subset of the Final's group columns still lands every row
/// of a group in one lane (`plan/aggregate.rs`'s subset rule); the id itself is unhashable on
/// the cpu (comet has no unsigned arm) and differs on the device (#65).
fn drop_grouping_id(mut keys: Vec<u32>, id: u32) -> Result<Vec<u32>, PlanError> {
    keys.retain(|k| *k != id);
    if keys.is_empty() || keys.iter().any(|k| *k > id) {
        return Err(PlanError::Invalid(format!(
            "a grouping-set shuffle over keys {keys:?}: after dropping the grouping id at {id} \
             every key must be a user key below it, and one must remain"
        )));
    }
    Ok(keys)
}
```

  and, directly above `tree = match shuffle {`:

```rust
    let shuffle = match shuffle {
        Shuffle::ByHash { keys, n } if !group.is_single() => Shuffle::ByHash {
            keys: drop_grouping_id(keys, group.expr().len() as u32)?,
            n,
        },
        other => other,
    };
```

  Unit tests beside it (`#[cfg(test)] mod tests` in `aggregate.rs`, or the translator tests):
  `drop_grouping_id(vec![0, 1, 2], 2) == Ok(vec![0, 1])`; `drop_grouping_id(vec![2], 2)` is
  `Err`; `drop_grouping_id(vec![0, 3], 2)` is `Err`.
- [x] **Step 4:** the planner test and the three unit tests green; `--lib` green.
- [x] **Step 5: Goldens.** `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 cargo test
  --features rust-only -p peacockdb-core --lib plan_goldens` (with the `/tmp` testdata symlink
  `build-test.md:646` names). Expect the tp4 plan sections of tpch rollup_over_join and tpcds
  q5, q18, q22, q77, q80 to move (`hash=` loses `__grouping_id`) and their `recipe-payloads.txt`
  bytes; nothing else. Review the diff as one.
- [ ] **Step 6: Commit.** `git commit -m "#189: a rollup's shuffle hashes its keys, never the grouping id"`.

### Task 3: Boolean keys, red then green (#206, part)

**Files:**
- Modify: `murmur_conformance.rs` (new gates); `cpp/src/spark_hash_partition.cu:144-157`

- [x] **Step 1: The gates.**

```rust
/// Boolean: comet hashes `i32::from(b)`, 4 bytes. NULL skipped.
#[test]
fn gpu_spark_partition_ids_boolean_match_rule_live() {
    use datafusion::arrow::array::BooleanArray;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(BooleanArray::from(vec![Some(true), Some(false), None, Some(true)]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Boolean, true), k)], 8);
}

/// Int8, which the kernel widens to INT32 already and no gate proved.
#[test]
fn gpu_spark_partition_ids_int8_match_rule_live() {
    use datafusion::arrow::array::Int8Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Int8Array::from(vec![
        Some(1), Some(0), Some(-1), Some(i8::MAX), Some(i8::MIN), None,
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int8, true), k)], 8);
}

/// A zero-row input and an all-NULL key: every row (none, or all) in `pmod(seed, n)`'s lane.
#[test]
fn gpu_spark_partition_ids_zero_rows_and_all_null_match_rule_live() {
    use datafusion::arrow::array::Int32Array;
    use datafusion::arrow::datatypes::DataType;
    let none: ArrayRef = Arc::new(Int32Array::from(Vec::<Option<i32>>::new()));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int32, true), none)], 8);
    let nulls: ArrayRef = Arc::new(Int32Array::from(vec![None::<i32>; 5]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Int32, true), nulls)], 8);
}
```

  (If the zero-row case trips the FFI's `rows` assert, the helper returns early when `rows == 0`
  after checking `rule.is_empty()` — the kernel is skipped at `n == 0` already.)
- [x] **Step 2:** device cycle, `PCK_TEST_FILTER='murmur_conformance'`: boolean RED on
  `unsupported key column cuDF type_id=11`; int8 and the empty cases green.
- [x] **Step 3: The arm.** In the normalizing `switch` (`.cu:144`), with INT8/INT16:

```cpp
      case cudf::type_id::BOOL8:   // comet: i32::from(bool) — a value cast, 0 or 1
      case cudf::type_id::INT8:
      case cudf::type_id::INT16:
```

  and the `CUDF_FAIL` text's supported list gains BOOL8.
- [x] **Step 4:** cycle: all green. **Commit:** `git commit -m "#206: a boolean key hashes as comet's i32"`.

### Task 4: Float keys, red then green (#206)

**Files:** `murmur_conformance.rs`; `spark_hash_partition.cu`

- [x] **Step 1: The gates.**

```rust
fn float_specials() -> Vec<Option<f64>> {
    vec![
        Some(0.0), Some(-0.0), Some(f64::NAN), Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))),
        None, Some(1.5), Some(f64::INFINITY), Some(f64::NEG_INFINITY), Some(-2.25),
    ]
}

/// Float64: comet hashes the value's 8 bytes, -0.0 as 0 (its +0.0 bits), NaN by its bits.
#[test]
fn gpu_spark_partition_ids_float64_match_rule_live() {
    use datafusion::arrow::array::Float64Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Float64Array::from(float_specials()));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float64, true), k)], 8);
}

/// Float32, 4 bytes, the same rules.
#[test]
fn gpu_spark_partition_ids_float32_match_rule_live() {
    use datafusion::arrow::array::Float32Array;
    use datafusion::arrow::datatypes::DataType;
    let v: Vec<Option<f32>> = float_specials()
        .into_iter()
        .map(|x| x.map(|f| if f.is_nan() { f32::from_bits(if f.is_sign_negative() { 0xffc0_0000 } else { 0x7fc0_0000 }) } else { f as f32 }))
        .collect();
    let k: ArrayRef = Arc::new(Float32Array::from(v));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float32, true), k)], 8);
}

/// Review focus 1: a column of specials only.
#[test]
fn gpu_spark_partition_ids_float_specials_only_match_rule_live() {
    use datafusion::arrow::array::Float64Array;
    use datafusion::arrow::datatypes::DataType;
    let k: ArrayRef = Arc::new(Float64Array::from(vec![
        Some(-0.0), Some(f64::NAN), None, Some(-0.0), Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))),
    ]));
    assert_gpu_matches_rule_live(vec![(Field::new("k", DataType::Float64, true), k)], 4);
}

/// Every NaN — either sign, any payload — and 0.0/-0.0 land together, on the production rule
/// (cpu-only: this proves the rule; the live gates above prove the kernel follows it).
#[test]
fn every_nan_shares_a_lane_and_so_do_the_two_zeros() {
    use datafusion::arrow::array::{Float32Array, Float64Array};
    use datafusion::arrow::datatypes::DataType;
    let nans64 = vec![
        Some(f64::NAN),
        Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))), // -NaN, as x86's 0/0 gives
        Some(f64::from_bits(0x7ff0_0000_0000_0001)),         // a signalling payload
        Some(0.0),
        Some(-0.0),
    ];
    let col: ArrayRef = Arc::new(Float64Array::from(nans64));
    let lane = lanes_of(vec![(Field::new("k", DataType::Float64, true), col)], 8);
    assert_eq!(lane[0], lane[1]);
    assert_eq!(lane[0], lane[2]);
    assert_eq!(lane[3], lane[4]);
    let nans32 = vec![Some(f32::NAN), Some(f32::from_bits(0xffc0_0000)), Some(0.0f32), Some(-0.0f32)];
    let col: ArrayRef = Arc::new(Float32Array::from(nans32));
    let lane = lanes_of(vec![(Field::new("k", DataType::Float32, true), col)], 8);
    assert_eq!(lane[0], lane[1]);
    assert_eq!(lane[2], lane[3]);
}
```

  `lanes_of(cols, lanes) -> Vec<usize>` is the per-row lane vector Task 1's gate already inverts
  from `rows_per_lane`; lift it to a named helper there. Red now: comet hashes the NaNs by their
  bits, so `lane[0] != lane[1]`.

- [x] **Step 2:** cycle: RED on `type_id=10` and `type_id=9`; locally
  `cargo test --features rust-only -p peacockdb-core --lib every_nan_shares_a_lane` red.
- [x] **Step 3: The cpu's canonical NaN.** In `spark_partitioning.rs`, beside `hash_keys`:

```rust
use datafusion::arrow::array::AsArray;
use datafusion::arrow::datatypes::{Float32Type, Float64Type};

/// Every NaN as Rust's `NAN` (`0x7ff8000000000000` / `0x7fc00000`), so comet — which hashes a
/// float by its bits — puts NaN and -NaN in one lane. The device's float arm does the same.
fn canonical_nans(array: ArrayRef) -> ArrayRef {
    match array.data_type() {
        DataType::Float64 => Arc::new(
            array.as_primitive::<Float64Type>().unary::<_, Float64Type>(|v| if v.is_nan() { f64::NAN } else { v }),
        ) as ArrayRef,
        DataType::Float32 => Arc::new(
            array.as_primitive::<Float32Type>().unary::<_, Float32Type>(|v| if v.is_nan() { f32::NAN } else { v }),
        ) as ArrayRef,
        _ => array,
    }
}
```

  `hash_keys` (`spark_partitioning.rs:51`) maps each evaluated key array through it:
  `.map(|expr| Ok(canonical_nans(expr.evaluate(batch)?.into_array(batch.num_rows())?)))` (Task 6
  adds the decimal cast before it in the same closure).
  `every_nan_shares_a_lane_and_so_do_the_two_zeros` goes green locally.
- [x] **Step 4: The arm.** A kernel beside `spark_hash_fixed_col_kernel`:

```cpp
// One thread per row; folds one FLOAT key column. comet hashes `value.to_le_bytes()` of the
// float, except -0.0, which it hashes as 0 — the +0.0 bit pattern. Every NaN is first made the
// one canonical NaN the cpu makes (Rust's f64::NAN / f32::NAN), so NaN and -NaN share a lane on
// both engines. Null rows are skipped.
template <typename T>
__global__ void spark_hash_float_col_kernel(cudf::column_device_view col,
                                            uint32_t* hashes,
                                            cudf::size_type n) {
  auto const row = static_cast<cudf::size_type>(blockIdx.x) * blockDim.x + threadIdx.x;
  if (row >= n) return;
  if (col.is_null(row)) return;
  T v = col.element<T>(row);
  if (v != v) {  // NaN
    if constexpr (sizeof(T) == 8) {
      v = __longlong_as_double(0x7ff8000000000000LL);
    } else {
      v = __int_as_float(0x7fc00000);
    }
  } else if (v == T{0}) {
    v = T{0};  // -0.0 == 0.0 is true: both become +0.0's bits
  }
  hashes[row] = spark_hash_bytes(reinterpret_cast<char const*>(&v),
                                 static_cast<int>(sizeof(T)), hashes[row]);
}
```

  and in the dispatch `switch`:

```cpp
        case cudf::type_id::FLOAT32:
          spark_hash_float_col_kernel<float><<<grid, block, 0, stream.value()>>>(
              *dcol, hashes.data(), n);
          break;
        case cudf::type_id::FLOAT64:
          spark_hash_float_col_kernel<double><<<grid, block, 0, stream.value()>>>(
              *dcol, hashes.data(), n);
          break;
```

- [x] **Step 5:** cycle: green, every float gate including the specials-only one. **Commit:**
  `git commit -m "#206: float keys hash on both engines, -0.0 as +0.0 and every NaN as one"`.

### Task 5: Timestamp keys, red then green (#240)

**Files:** `murmur_conformance.rs`; `spark_hash_partition.cu`

- [x] **Step 1: The gates**, one per unit plus a zoned one:

```rust
fn ts_values() -> Vec<Option<i64>> {
    vec![Some(0), Some(1_700_000_000), Some(-1), Some(i64::MAX), Some(i64::MIN), None, Some(86_400)]
}

#[test]
fn gpu_spark_partition_ids_timestamps_match_rule_live() {
    use datafusion::arrow::array::{
        TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
        TimestampSecondArray,
    };
    use datafusion::arrow::datatypes::{DataType, TimeUnit};
    let cases: Vec<(DataType, ArrayRef)> = vec![
        (DataType::Timestamp(TimeUnit::Second, None), Arc::new(TimestampSecondArray::from(ts_values()))),
        (DataType::Timestamp(TimeUnit::Millisecond, None), Arc::new(TimestampMillisecondArray::from(ts_values()))),
        (DataType::Timestamp(TimeUnit::Microsecond, None), Arc::new(TimestampMicrosecondArray::from(ts_values()))),
        (DataType::Timestamp(TimeUnit::Nanosecond, None), Arc::new(TimestampNanosecondArray::from(ts_values()))),
        (
            DataType::Timestamp(TimeUnit::Microsecond, Some("UTC".into())),
            Arc::new(TimestampMicrosecondArray::from(ts_values()).with_timezone("UTC")),
        ),
    ];
    for (ty, k) in cases {
        assert_gpu_matches_rule_live(vec![(Field::new("k", ty, true), k)], 8);
    }
}
```

- [x] **Step 2:** cycle: RED on `type_id=13` (TIMESTAMP_SECONDS) first.
- [x] **Step 3: The arm.** In the normalizing `switch`, beside `TIMESTAMP_DAYS`:

```cpp
      case cudf::type_id::TIMESTAMP_SECONDS:
      case cudf::type_id::TIMESTAMP_MILLISECONDS:
      case cudf::type_id::TIMESTAMP_MICROSECONDS:
      case cudf::type_id::TIMESTAMP_NANOSECONDS:
        // comet hashes every unit as its i64 — the same bytes, so a zero-copy bit cast.
        col = cudf::bit_cast(col, cudf::data_type{cudf::type_id::INT64});
        break;
```

  The kernel's comment at `:84-85` ("Timestamp-as-i64 → 8B") becomes true.
- [x] **Step 4:** cycle: green. **Commit:** `git commit -m "#240: timestamp keys hash as their i64, every unit"`.

### Task 5b: The wire names the four timestamp types (#240)

**Files:**
- Modify: `flatbuffers/gpu_plan.fbs:14-35` (`enum DataType`)
- Modify: `peacockdb-core/src/wire/serialize.rs:101-124` (`convert_data_type`)
- Modify: `cpp/src/expr.cpp:77-97` (`fb_to_type_id`)
- Modify: `peacockdb-core/src/planner/tests/plan_goldens.rs:442` (`NOT_RUNNABLE`)
- Test: `peacockdb-core/src/wire/tests.rs`, `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Produces: `fb::DataType::{TimestampSecond, TimestampMillisecond, TimestampMicrosecond, TimestampNanosecond}`
  (values 20–23), which join-session-cpp's pads and absent-side schemas read and join-backend's
  `CudfJoin` writer emits.

- [x] **Step 1: The failing tests.** `wire/tests.rs`:

```rust
#[test]
fn every_timestamp_unit_crosses_the_wire_with_its_unit() {
    use datafusion::arrow::datatypes::{DataType, TimeUnit};
    for (unit, fb_ty) in [
        (TimeUnit::Second, fb::DataType::TimestampSecond),
        (TimeUnit::Millisecond, fb::DataType::TimestampMillisecond),
        (TimeUnit::Microsecond, fb::DataType::TimestampMicrosecond),
        (TimeUnit::Nanosecond, fb::DataType::TimestampNanosecond),
    ] {
        assert_eq!(convert_data_type(&DataType::Timestamp(unit, None)), Ok(fb_ty));
        assert_eq!(convert_data_type(&DataType::Timestamp(unit, Some("UTC".into()))), Ok(fb_ty));
    }
}
```

  and a plan-executor gtest `PlanExecutor.CastTimestampMicrosToSeconds`: a `CudfProject` with one
  `CastExprNode { target_type: TimestampSecond }` over a `TIMESTAMP_MICROSECONDS` column holding
  `1'500'000` and `-1`, asserting a `TIMESTAMP_SECONDS` column holding `1` and `-1` (cuDF's
  unit-narrowing cast floors toward negative infinity: `-1µs` is second `-1`).
- [x] **Step 2: Run red** — the Rust test fails to compile (no variant); the gtest throws from
  `cudf::cast` on `EMPTY`.
- [x] **Step 3: The append** (never insert: existing values keep their numbers):

```
  Decimal128,
  TimestampSecond,
  TimestampMillisecond,
  TimestampMicrosecond,
  TimestampNanosecond,
}
```

  `convert_data_type` (`TimeUnit` from `datafusion::arrow::datatypes`):

```rust
        ArrowDataType::Timestamp(TimeUnit::Second, _) => fb::DataType::TimestampSecond,
        ArrowDataType::Timestamp(TimeUnit::Millisecond, _) => fb::DataType::TimestampMillisecond,
        ArrowDataType::Timestamp(TimeUnit::Microsecond, _) => fb::DataType::TimestampMicrosecond,
        ArrowDataType::Timestamp(TimeUnit::Nanosecond, _) => fb::DataType::TimestampNanosecond,
```

  `fb_to_type_id`:

```cpp
    case fb::DataType_TimestampSecond:      return cudf::type_id::TIMESTAMP_SECONDS;
    case fb::DataType_TimestampMillisecond: return cudf::type_id::TIMESTAMP_MILLISECONDS;
    case fb::DataType_TimestampMicrosecond: return cudf::type_id::TIMESTAMP_MICROSECONDS;
    case fb::DataType_TimestampNanosecond:  return cudf::type_id::TIMESTAMP_NANOSECONDS;
```

  `fb_text` renders a `DataType` through the generated enum's `Debug`, so it names the new values
  with no change; the time zone is not on the wire (cuDF has none; the values are UTC `int64`s).
  The column-path cast (`expr.cpp:896-917`) already calls `cudf::cast` with the mapped type, and
  the AST router sends any non-INT64/FLOAT64 target there (`:423-430`).
- [x] **Step 4** (run as plain `cargo test ... --lib plan_goldens`, NOT through `scripts/cargo-cudf.sh`, which redirects to the cuDF target dir — under 7 GiB free on `/`): `NOT_RUNNABLE` loses `("pbench", "timestamp-s-key-group", "240")` (pbench put it
  there); `UPDATE_CANONICAL=1 scripts/cargo-cudf.sh test --features rust-only -p peacockdb-core --lib plan_goldens::pbench`
  rewrites that query's five plan sections (now a plan, not `not runnable`); `git diff --stat`
  shows only those. `recipe-payloads.txt` holds no timestamp, so it does not move (checked by
  `the_payload_golden_carries_what_each_call_hands_the_executor`).
- [x] **Step 5: Run green**, rust-only half only: `cargo test --features rust-only -p peacockdb-core --lib wire::tests plan_goldens`;
  device cycle with `PCK_TEST_FILTER=PlanExecutor.CastTimestamp` — **NOT RUN, no card, and the gtest of step 1 is unwritten.** **Commit:**
  `git commit -m "#240: the wire names the four timestamp types; pbench's second-unit key crosses it"`.

### Task 5c: An unmapped type is a plan-time refusal, in every schema (#249)

**Files:**
- Modify: `peacockdb-core/src/wire/serialize.rs:130-140` (`serialize_schema`) and its callers in `wire/`
- Modify: `peacockdb-core/src/wire/expr_writer.rs:197-199` (`data_type`: the `(#240)` arm goes)
- Modify: `peacockdb-core/src/planner/tests/plan_goldens.rs:442` (`NOT_RUNNABLE`)
- Test: `peacockdb-core/src/wire/tests.rs`

- [x] **Step 1: The failing test.**

```rust
#[test]
fn bug_a_schema_holding_an_interval_is_refused_at_plan_time() {   // #249
    let schema = Arc::new(Schema::new(vec![
        Field::new("k", DataType::Int32, true),
        Field::new("iv", DataType::Interval(IntervalUnit::MonthDayNano), true),
    ]));
    let mut b = FlatBufferBuilder::new();
    let err = serialize_schema(&mut b, &schema).expect_err("an interval has no wire type (#249)");
    assert!(matches!(&err, PlanError::Unsupported(why) if why.contains("iv") && why.contains("Interval")), "{err}");
}
```
- [x] **Step 2: Run red** — `serialize_schema` returns a schema with `iv: Null`.
  (A `bug_` pin: it asserts #249's refusal, and goes red when the wire gains the interval type.)
- [x] **Step 3: Implement.** `serialize_schema` returns `Result<_, PlanError>`, and every field
  goes through `convert_data_type`:

```rust
pub(crate) fn serialize_schema<'a>(b: &mut FlatBufferBuilder<'a>, schema: &SchemaRef)
    -> Result<WIPOffset<fb::Schema<'a>>, PlanError> {
    let mut fields = Vec::with_capacity(schema.fields().len());
    for f in schema.fields() {
        let dt = convert_data_type(f.data_type()).map_err(|why| PlanError::Unsupported(format!(
            "column {} of type {}: {why} (#249)", f.name(), f.data_type())))?;
        fields.push(field(b, f, dt));
    }
    let fields = b.create_vector(&fields);
    Ok(fb::Schema::create(b, &fb::SchemaArgs { fields: Some(fields) }))
}
```
  Its callers gain `?` (the writer's node functions already return `Result<_, PlanError>`).
  join-backend's `serialize_join_schema` then reduces to this call. `expr_writer::data_type`
  (pbench's Step 0 made it cite a ticket) keeps its `(#249)` suffix for every unnamed type; its
  `Timestamp(..) => "#240"` arm goes, since Task 5b maps every timestamp unit:

```rust
fn data_type(data_type: &DataType) -> Result<fb::DataType, PlanError> {
    convert_data_type(data_type).map_err(|why| PlanError::Unsupported(format!("{why} (#249)")))
}
```
- [~] **Step 4: DELETED, not deferred** — the three query files do not exist, they are #255's, and the declaration guard is bidirectional (`-detail.md`, "The deviation the partial requires") — `test_duckdb_result.py:165` asserts their absence, and `NOT_RUNNABLE` is checked both ways (`plan_goldens.rs:456`, and the `uncrossable == declared` assert at `:421-434`), so the step would go red on contact rather than fail to find them. Original text: pbench's three queries whose plans now hold a type the wire cannot name go into
  `NOT_RUNNABLE` with `"249"`: `interval-through-join`, `struct-through-join` and
  `struct-key-join` (its scan schema holds the struct key). Each line cites `(#249)`. `the_payload_golden_carries_what_each_call_hands_the_executor`
  and every plan golden of tpch and tpcds stay byte-identical (no such type in either).
- [x] **Step 5: Run green**: `cargo test --features rust-only -p peacockdb-core --lib wire::tests plan_goldens`.
  **Commit:** `git commit -m "#249: a type the wire cannot name is refused at plan time, in every schema"`.

### Task 6: Decimal keys — 16 bytes on both engines (#95)

**Files:**
- Modify: `spark_partitioning.rs` (`hash_keys`); `murmur_conformance.rs`; `spark_hash_partition.cu`

- [x] **Step 1: The gates.**

```rust
fn decimal(values: Vec<Option<i128>>, p: u8, s: i8) -> ArrayRef {
    use datafusion::arrow::array::Decimal128Array;
    Arc::new(Decimal128Array::from(values).with_precision_and_scale(p, s).expect("fits"))
}

/// Decimal128 at p ≤ 18 and p > 18: both engines hash the unscaled value's 16 LE bytes — the
/// cpu by casting to (38, s) before comet, the kernel from the int128. Not Spark's placement
/// at p ≤ 18, by decision (D2).
#[test]
fn gpu_spark_partition_ids_decimals_match_rule_live() {
    use datafusion::arrow::datatypes::DataType;
    let small = vec![Some(0), Some(1), Some(-1), Some(12_345), Some(-99_999_999), None];
    assert_gpu_matches_rule_live(
        vec![(Field::new("k", DataType::Decimal128(15, 2), true), decimal(small, 15, 2))],
        8,
    );
    let wide = vec![
        Some(0), Some(-1), Some(i64::MIN as i128 - 1), Some(10i128.pow(37)), Some(-(10i128.pow(37))), None,
    ];
    assert_gpu_matches_rule_live(
        vec![(Field::new("k", DataType::Decimal128(38, 4), true), decimal(wide, 38, 4))],
        8,
    );
}

/// Review focus 3: a decimal then a string, seed-chained.
#[test]
fn gpu_spark_partition_ids_decimal_composite_match_rule_live() {
    use datafusion::arrow::datatypes::DataType;
    let d = decimal(vec![Some(150), None, Some(-3), Some(150)], 15, 2);
    let s: ArrayRef = Arc::new(StringArray::from(vec![Some("a"), Some("b"), None, Some("c")]));
    assert_gpu_matches_rule_live(
        vec![
            (Field::new("d", DataType::Decimal128(15, 2), true), d),
            (Field::new("s", DataType::Utf8, true), s),
        ],
        8,
    );
}
```

- [x] **Step 2:** cycle: RED on `type_id=27`.
- [x] **Step 3: The cpu cast.** `hash_keys` in `spark_partitioning.rs`:

```rust
use datafusion::arrow::compute::cast;
use datafusion::arrow::datatypes::DataType;

/// The key columns as comet hashes them. A decimal is widened to precision 38 first, so
/// comet hashes its unscaled value's 16 bytes at every precision — the bytes the device's
/// int128 holds. Without it comet hashes 8 bytes at p ≤ 18, a width the device cannot tell
/// from its columns (cuDF keeps no precision and the loader widens every decimal).
fn hash_keys(batch: &RecordBatch, hash_exprs: &[Arc<dyn PhysicalExpr>]) -> DfResult<Vec<ArrayRef>> {
    hash_exprs
        .iter()
        .map(|expr| {
            let array = expr.evaluate(batch)?.into_array(batch.num_rows())?;
            let array = match array.data_type() {
                DataType::Decimal128(p, s) if *p < 38 => cast(&array, &DataType::Decimal128(38, *s))?,
                _ => array,
            };
            Ok(canonical_nans(array)) // Task 4
        })
        .collect()
}
```

- [x] **Step 4: The kernel arm.**

```cpp
// One thread per row; folds one DECIMAL128 key column: the unscaled int128's 16 LE bytes,
// matching the cpu, which widens every decimal key to precision 38 before comet (repartition-keys).
__global__ void spark_hash_decimal128_col_kernel(cudf::column_device_view col,
                                                 uint32_t* hashes,
                                                 cudf::size_type n) {
  auto const row = static_cast<cudf::size_type>(blockIdx.x) * blockDim.x + threadIdx.x;
  if (row >= n) return;
  if (col.is_null(row)) return;
  __int128_t const v = col.data<__int128_t>()[row];
  hashes[row] = spark_hash_bytes(reinterpret_cast<char const*>(&v), 16, hashes[row]);
}
```

  In the normalizing `switch`, a DECIMAL32/64 key is widened to DECIMAL128 at its scale
  (`cudf::cast(col, cudf::data_type{cudf::type_id::DECIMAL128, col.type().scale()}, stream, mr)`,
  kept in `decoded_keep`); in the dispatch, `case cudf::type_id::DECIMAL128:` launches the kernel.
- [x] **Step 5:** cycle: green. **Commit:** `git commit -m "#95: decimal keys hash 16 bytes on both engines"`.

### Task 6b: Unsigned keys — one rule on both engines (review row 11)

**Files:**
- Modify: `spark_partitioning.rs` (`hash_keys`); `murmur_conformance.rs`; `spark_hash_partition.cu`

Comet has no unsigned arm, nor has the kernel, so an unsigned key across a shuffle is refused at
run time on both engines (the tp1 modes, which do not shuffle, answer). Spark has no unsigned
types, so there is no placement to match: the rule is ours, the same on both engines —
**UInt8 and UInt16 cast to Int32, UInt32 to Int64 (value casts, zero-extended), UInt64
reinterpreted as Int64 bits** — then hashed as that signed type is.

- [x] **Step 1: The gates.**

```rust
/// Unsigned keys: the cpu and the device apply the same widening before hashing —
/// u8/u16 → i32, u32 → i64 by value, u64 → i64 by its bits. No Spark placement to match.
#[test]
fn gpu_spark_partition_ids_unsigned_match_rule_live() {
    use datafusion::arrow::array::{UInt16Array, UInt32Array, UInt64Array, UInt8Array};
    use datafusion::arrow::datatypes::DataType;
    let cases: Vec<(DataType, ArrayRef)> = vec![
        (DataType::UInt8, Arc::new(UInt8Array::from(vec![Some(0), Some(1), Some(u8::MAX), None]))),
        (DataType::UInt16, Arc::new(UInt16Array::from(vec![Some(0), Some(1), Some(u16::MAX), None]))),
        (DataType::UInt32, Arc::new(UInt32Array::from(vec![Some(0), Some(1), Some(u32::MAX), Some(1 << 31), None]))),
        (DataType::UInt64, Arc::new(UInt64Array::from(vec![Some(0), Some(1), Some(u64::MAX), Some(1 << 63), None]))),
    ];
    for (ty, k) in cases {
        assert_gpu_matches_rule_live(vec![(Field::new("k", ty, true), k)], 8);
    }
}
```

- [x] **Step 2:** cycle: RED on the cpu side first — `Unsupported data type in hasher: UInt8`
  from comet inside `rows_per_lane` — and, once step 3 lands, on `type_id=5` (UINT8) on the device.
- [x] **Step 3: The cpu widening**, in `hash_keys` beside the decimal cast:

```rust
            let array = match array.data_type() {
                DataType::Decimal128(p, s) if *p < 38 => cast(&array, &DataType::Decimal128(38, *s))?,
                DataType::UInt8 | DataType::UInt16 => cast(&array, &DataType::Int32)?,
                DataType::UInt32 => cast(&array, &DataType::Int64)?,
                DataType::UInt64 => {
                    // by its bits, not its value: a u64 past i64::MAX has no i64 value
                    let u = array.as_primitive::<datafusion::arrow::datatypes::UInt64Type>();
                    let bits: datafusion::arrow::array::Int64Array =
                        u.unary(|v: u64| v as i64);
                    Arc::new(bits) as ArrayRef
                }
                _ => array,
            };
```
  (`PrimitiveArray::unary` keeps the null buffer; `as_primitive` is `AsArray`'s.)
- [x] **Step 4: The kernel**, in the normalizing `switch`:

```cpp
      case cudf::type_id::UINT8:
      case cudf::type_id::UINT16:
        keep = cudf::cast(col, cudf::data_type{cudf::type_id::INT32}, stream, mr);  // zero-extended value
        col = keep->view();
        break;
      case cudf::type_id::UINT32:
        keep = cudf::cast(col, cudf::data_type{cudf::type_id::INT64}, stream, mr);  // zero-extended value
        col = keep->view();
        break;
      case cudf::type_id::UINT64:
        col = cudf::bit_cast(col, cudf::data_type{cudf::type_id::INT64});           // its bits, no copy
        break;
```
  (`keep` is the switch's existing owner for a converted column — `decoded_keep` where the
  decimal arm keeps its widened column; use the same vector.) The `CUDF_FAIL` text's supported list
  gains the four.
- [x] **Step 5:** cycle: green. **Commit:** `git commit -m "unsigned keys hash as their widened signed value on both engines"`.

### Task 7: Every key type through the operator harness

**Files:**
- Modify: `peacockdb-core/src/tests/synthetic.rs`; `tests/gpu_tests/emit_cases.rs`;
  `tests/gpu_tests/emit_schema_cases.rs`

**Interfaces:**
- Produces: `pub(crate) fn key_types_schema() -> Arc<ArrowSchema>` and
  `pub(crate) fn key_types(rows: usize, seed: u64) -> RecordBatch` — column order:
  `id Int64, k8 Int8, k16 Int16, k32 Int32, k64 Int64, f32 Float32, f64 Float64, b Boolean,
  d Date32, ts_s, ts_ms, ts_us, ts_ns Timestamp(unit, None), dec15 Decimal128(15,2),
  dec38 Decimal128(38,4), s Utf8, u8 UInt8, u16 UInt16, u32 UInt32, u64 UInt64` (ordinals 0..19;
  the unsigned four appended last so the others keep their ordinals). join-backend reuses it for
  join keys.

- [x] **Step 1: The generator.** Each column from `mix(seed, row)` as `synthetic` does, every
  column but `id` NULL at its own stride; `f32` and `f64` cycle through `[0.0, -0.0, NaN, -NaN]`
  on every fifth row (`row % 5 == 0` picks `specials[(row / 5) % 4]`), otherwise a dyadic value;
  decimals include negatives and, for `dec38`, values past `i64`; `u32` and `u64` include values
  past their signed type's maximum (`1 << 31`, `1 << 63`, `u64::MAX`). A test
  `key_types_is_the_same_batch_twice` beside `a_synthetic_batch_is_the_same_batch_twice`.
- [x] **Step 2: The cases.** In `emit_cases.rs`:

```rust
fn key_types_given() -> Box<dyn GpuNode> {
    Given::of(Schema::new(key_types_schema()), BatchLayout::MultipleBatches)
}

/// One case per key column of `key_types`: both engines place every row in the same lane.
fn every_row_in_the_same_lane_on(key: u32) {
    let node = GpuEmitPartitions::new(key_types_given(), vec![key], 4);
    run_both(&node, Script::Emit(vec![key_types(96, 5)])).same(Order::Any);
}
```

  and `operator_case!`s calling it, one per ordinal 1–19, named for the type
  (`an_int8_key_places_every_row_the_same`, `a_float32_key_…`, `a_float64_key_…` (replaces the
  #206 float pin), `a_boolean_key_…` (replaces the #206 boolean pin), `a_date_key_…`,
  `a_timestamp_second_key_…` … `_nanosecond_…`, `a_narrow_decimal_key_…` (replaces the #95 pin),
  `a_wide_decimal_key_…`, `a_utf8_key_…`, `a_uint8_key_…` … `a_uint64_key_…`);
  `a_mixed_composite_key_places_every_row_the_same`
  over `vec![6, 14, 15, 9]`; and review focus 4:

```rust
operator_case! {
    GpuEmitPartitions,
    fn a_zero_row_batch_scatters_on_every_key_type() {
        for key in 1..20 {
            let node = GpuEmitPartitions::new(key_types_given(), vec![key], 4);
            let outcome = run_both(&node, Script::Emit(vec![key_types(0, 1)]));
            outcome.same(Order::Any);
        }
    }
}
```

  Delete `refused_key_type` once nothing calls it. `emit_schema_cases.rs`: one
  `assert_holds_as_declared` case per key type over `key_types(64, 1)`.
- [x] **Step 3:** cycle, `PCK_TEST_FILTER='emit_cases|emit_schema_cases'`: green; the three old
  pins are gone, replaced by the green cases named in the commit.
- [~] **Step 3b: BLOCKED by [#255](../tickets/complete-coverage.md#t255), not deferred** — the pin was written and run, and `common::type_structural_size` panics on a Struct at `common.rs:66` before any scatter, which is #255 by its own citation (it names that line and says "Not #245 either"). Adding a Struct arm there would be production code changed for a test, outside the spec's Restriction. See `-detail.md`. Original text: **Step 3b: #245's pin.** In `emit_cases.rs`, beside the key-type cases:

```rust
// #245 — no hasher arm, on either engine, for a nested type: a struct key is refused at the scatter.
operator_case! {
    GpuEmitPartitions,
    fn bug_a_struct_key_is_refused_on_both() {
        let key: ArrayRef = Arc::new(StructArray::from(vec![
            (Arc::new(Field::new("a", DataType::Int32, true)), Arc::new(Int32Array::from(vec![1, 2, 1])) as ArrayRef),
            (Arc::new(Field::new("b", DataType::Utf8, true)), Arc::new(StringArray::from(vec!["x", "y", "x"])) as ArrayRef),
        ]));
        let outcome = run_both(&emit(vec![1], 4), script(None, vec![with_key(key)]));
        assert!(outcome.cpu.is_err() && outcome.gpu.is_err(), "both refuse a nested key (#245)");
    }
}
```
- [ ] **Step 4: Commit.** `git commit -m "every key type through the harness; the #206 and #95 pins flip; #245 pinned"`.

### Task 8: #243's pins

**Files:** `tests/gpu_tests/aggregate_cases.rs`; `tests/gpu_tests/join_cases.rs`

- [x] **Step 1: The aggregate pin.**

```rust
/// `synthetic`'s eight columns with `f64` (ordinal 4) holding the six keys #243 measures.
fn float_key_batch() -> RecordBatch {
    let batch = synthetic(6, 1);
    let mut columns = batch.columns().to_vec();
    columns[4] = Arc::new(Float64Array::from(vec![
        Some(0.0), Some(-0.0), Some(f64::NAN), Some(f64::from_bits(f64::NAN.to_bits() | (1 << 63))),
        None, Some(1.0),
    ]));
    RecordBatch::try_new(batch.schema(), columns).expect("one Float64 column for another")
}

fn rows(slots: &[Slot]) -> usize {
    slots.iter().flatten().map(RecordBatch::num_rows).sum()
}

// #243 — the cpu's equality: DataFusion groups a float by its bits (six groups); cuDF by value
// with NaNs equal (four). One lane (the harness's tp1), so the lane rule is not in play: since
// Task 4 both engines put every NaN and both zeros in one lane, and this pin is equality alone.
operator_case! {
    GpuAggregate,
    fn bug_a_float_group_key_splits_negative_zero_and_the_nans_on_the_cpu() {
        let node = init_by(
            &[(4, "f64", DataType::Float64)],
            vec![call(PlanAgg::Count, Expr::column(0, "id"), "count(id)", DataType::Int64)],
        );
        let outcome = run_both(&node, Script::Exec(vec![float_key_batch()]));
        assert_eq!(rows(outcome.cpu.as_ref().expect("the cpu answers")), 6);
        assert_eq!(rows(outcome.gpu.as_ref().expect("the device answers")), 4);
    }
}
```

- [x] **Step 2: The join pin**, in `join_cases.rs`, Inner on the `f64` columns (ordinal 4 of each
  side), one probe batch:

```rust
// #243 — the cpu's equality: Inner over b.f64 = p.f64, the cpu pairs equal bits (0↔0, -0↔-0,
// NaN↔NaN, 1↔1: four); the device pairs equal values (0 and -0 both ways, NaN with both NaNs,
// 1: seven). One probe batch, one lane.
operator_case! {
    GpuHashJoin,
    fn bug_a_float_join_key_misses_negative_zero_and_nan_pairs_on_the_cpu() {
        let mut node = join(JoinType::Inner);
        node.keys = vec![(4, 4)];
        let build = prefixed(&float_key_batch(), "b_");
        let probe = {
            let b = synthetic(5, 2);
            let mut c = b.columns().to_vec();
            c[4] = Arc::new(Float64Array::from(vec![Some(0.0), Some(-0.0), Some(f64::NAN), None, Some(1.0)]));
            prefixed(&RecordBatch::try_new(b.schema(), c).expect("same shape"), "p_")
        };
        let outcome = run_both(&node, script(Some(build), vec![probe]));
        assert_eq!(rows(outcome.cpu.as_ref().expect("the cpu answers")), 4);
        assert_eq!(rows(outcome.gpu.as_ref().expect("the device answers")), 7);
    }
}
```

  (If `GpuHashJoin`'s `keys` is not a public field, build it with `hash_join_with`'s
  `GpuHashJoin::new(...)` call and `vec![(4, 4)]` in place of `vec![(1, 1)]`; `float_key_batch`
  is shared from `aggregate_cases.rs` as `pub(crate)`.)
- [x] **Step 3:** cycle: both pins green (they assert the divergence). **Commit:**
  `git commit -m "#243 pinned: the cpu's float equality splits -0.0 and the NaNs"`.

### Task 9: Cells, registry and goldens

- [x] **Step 1** (round 1; the `65` retag of pbench's row was left to the device half — `-detail.md`): `corpus_cases.inc`: the cpu `tp4_single | tp4_rowgroup | tp4_sized` of tpch
  `rollup_over_join` and tpcds `q5`, `q18`, `q22`, `q80` enabled (15 cells, #189's; the estimate's
  §2), and pbench's `rollup-small-keys` the same (its cpu tp4 cells; its gpu tp4 cells then meet
  #65, as rollup_over_join's do — tagged `65`). `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test
  test_cpu_corpus` under `PCK_TEST_FILTER` for the five, writing their cpu sections; each one's
  `duckdb_*` case green (duckdb-oracle's comparison).
- [x] **Step 2:** `cost-registry.csv`: those cells `enabled`; `189` struck from the five rows
  where no off cell is left without another ticket (`registry.rs:229-240`); `95` struck the same
  way from tpch q2, q10, q15, q18 and tpcds q24, q37, q75, q82. Device cycle over the #95 rows'
  tp4 gpu cells whose only other ticket is closed — none is expected (the estimate: they wait on
  #152) — and pbench's key-type rows' tp4 gpu cells, enabled where they pass, `timestamp-s-key-group`'s
  among them now that it crosses the wire (its `240` struck where it was the last ticket), and
  `uint-key-group` and `uint-key-join` (Task 6b; their cpu tp4 cells too, which comet refused
  before the widening — pbench tagged them `189`, struck now); the float rows stay commented out on #243 (the cpu's equality).
- [x] **Step 3:** registry tests both ways green. **Commit:**
  `git commit -m "#189's cpu cells on; 95 and 189 struck where nothing else holds a cell"`.

### Task 10: The wiki

- [~] **Split: `build-test.md`'s counts are done; every other page is the human's to write.** This
  dispatch reserves `llm-wiki/` markdown to the human except this plan and `-detail.md`, and
  carves out `build-test.md`'s counts, which are re-summed by script — every block header equals
  its rows (cpu 1688, ffi 7, gpu 671) and the grand total its parts (2477 + 97 + 401 = 2975). The
  cuDF-smoke row and the murmur row's example name moved with their counts, because both named a
  deleted symbol. **Owed, with the exact text in `-detail.md`:** `architecture.md`'s hash section
  (the decimal and NaN rules, the supported key list), `corpus-fixes.md`'s D2, #243's rewording,
  `tickets.md`'s counts, and the archivals. **No ticket was archived** — see the next line.
- [~] **The archival is not takeable yet, and #240 is why.** #95, #201 and #206 are ready: every
  cell they held is run and on. #189 is ready: all 24 cells. **#240 is not** —
  `pbench/timestamp-s-key-group`'s five device cells are still off, on a cause that is not #240
  (the device refuses a non-`ColumnRef` group expr), and registry row 195 still tags `240` because
  `cost-report` exits 1 on a ticket that resolves nowhere and the new ticket is not mine to file.
  Archiving #240 now would leave five cells explained by a closed ticket, which is the exact rot
  this task was told to avoid. File the proposed ticket, retag row 195, then archive.
- [ ] `architecture.md`, "Rehash and the comet hash": the decimal rule (16 bytes on both engines,
  the cpu's cast, not Spark's placement at p ≤ 18) and the supported key list. `build-test.md`:
  the murmur gate's count and description (production rule, the new types), the emit cases'
  count, the cuDF smoke row. Archive #95, #189, #201, #206, #240 to
  `archive/archived-tickets.md` with "Done <date> by repartition-keys"; `tickets.md` counts;
  `corpus-fixes.md` D2 marked decided (the cpu-side cast); #243 reworded to the cpu's equality
  alone — "DataFusion compares and groups floats by their bits" — with its lane half struck as
  fixed here (every NaN one lane, both zeros one lane) and its two pins named; `architecture.md`'s
  hash section states the float rule; the flatbuffers table names the four timestamp types.
- [ ] **Commit:** `git commit -m "repartition-keys: wiki, tickets archived"`.

### Task 11: The record

- [x] Detail file: the red/green outputs of every gate, the SEED mutation run, the golden diff
  summary, the cells enabled. **Commit:** `git commit -m "repartition-keys: the record"`.
