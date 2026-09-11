# #65 — the device's `__grouping_id` is not DataFusion's

Read-only proposal at master c18e063a. Paths relative to `/media/data/peacockdb`. DataFusion
paths are the vendored 45.0.0 crates under `~/.cargo/registry/src/*/`.

## 1. Issue

A grouping-set init emits `__grouping_id` beside the keys, and the two engines emit two
different columns for it.

- **CPU**: `executor/cpu_backend/mod.rs:344-348` builds DataFusion's
  `PhysicalGroupBy::new(keys, null_exprs, grouping_sets)`, so the column is DataFusion's own
  `group_id_array` (`datafusion-physical-plan-45.0.0/src/aggregates/mod.rs:1267-1285`):
  the set's mask folded `(acc << 1) | is_null` over key positions 0..n — position i is bit
  (n−1−i) — typed `UInt8` for ≤ 8 keys, `UInt16` ≤ 16, `UInt32` ≤ 32, `UInt64` above
  (`Aggregate::grouping_id_type`, `datafusion-expr-45.0.0/src/logical_plan/plan.rs:3223-3233`;
  the bit convention is documented at `:3239-3247`: `0b01` = second key excluded).
- **Device**: `cpp/src/operators/aggregate.cpp:388-392` folds `gid |= (1 << i)` — position i
  is bit i — and `:414-415` materialises it as `INT32`. A two-key rollup gives 0, 1, 3 on
  the CPU and 0, 2, 3 on the device; the middle sets of any rollup with ≥ 2 keys carry a
  different number, and the column is four bytes wide against a declared one.
- **The plan declares DataFusion's**: `planner/translator/aggregate.rs:261-266` reads the gid
  field off the partial's schema, so every plan golden prints `__grouping_id:UInt8` and every
  node above the init declares a column the device never produces.

Corpus cells disabled: **none today**, and the 00-tickets row is right. Every corpus rollup
projects the id away before the unload (`architecture.md:280-281`), per-node goldens compare
counts and schema-priced bytes rather than values (`architecture.md`, "What guards it, and
what does not"), and no enabled device cell is a rollup (six device cells, `q6` and `q19`).
The three places it would show are all behind other tickets:

- an expression over the id — `GROUPING()` is rewritten by DataFusion's analyzer into
  `CAST(__grouping_id …)` with bit tests (`datafusion-optimizer-45.0.0/src/analyzer/
  resolve_grouping_function.rs:180-249`), which is tpcds q70/q86 (#23 for the plan, #143 for
  their `rank() OVER`, plus unsigned literals and shift ops the C++ lacks — section 6);
- the id reaching the unload, where `gpu_backend/mod.rs:179` refuses an exported type the sink
  did not declare;
- the shuffle: with the id in the hash list (today's plan, #189) the device hashes an `INT32`
  whose value differs from the CPU's, so rollup rows of the middle sets would land in
  different lanes on the two engines — a per-lane golden divergence, unobservable because the
  CPU refuses first (comet has no `UInt8` arm) and no device tp4 rollup cell is enabled (#152).

Two corrections to the ticket text. "q70/q86 after #23" undersells it: both are window
queries (`testdata/tpcds-queries/q70.sql`, `q86.sql`; registry rows 71, 87 carry `143`), so
they are behind #23 and #143 together. And the registry's nine `65` rows (q5 q14 q18 q22 q67
q70 q77 q80 q86, `testdata/cost-registry.csv:6,15,19,23,68,71,78,81,87`) are all disabled for
another named reason first; `65` is a co-attribution nowhere the deciding one.

## 2. Root cause

One value, two owners, no comparison.

- `planner/translator/aggregate.rs:264-266` takes the gid's name and type from DataFusion's
  partial schema (`aggregates/mod.rs:271-278`, `group_fields` pushes
  `Field::new(INTERNAL_GROUPING_ID, grouping_id_type(n), false)`), and `:272-276` copies the
  masks. The wire carries the masks and the NULL placeholders (`wire/aggregate_writer.rs:41-64`
  → `CudfAggregate.grouping_sets`, `null_exprs`, `flatbuffers/gpu_plan.fbs:393-401`) and
  **no output schema and no per-set id** — `CudfAggregate` has `aggr_input_schema` only
  (`gpu_plan.fbs:407`). So the C++ has to invent the id's value and width itself.
- `aggregate.cpp:329-432` is that invention. Its comment at `:384-387` says the id "only has to
  be DISTINCT per set", which was true for the merge above it (`GpuAggregateBatches` groups on
  keys + id, `translator/aggregate.rs:334-340`) and false for everything else that reads the
  column: a projection computing `GROUPING()`, the unload, the hasher, and #164's future
  declared-vs-produced check, for which every rollup on a device would be the first hit.
- Nothing pins the C++ side. `test_gpu_recipe_walk.rs:748` (`a_rollup_answers_with_every_
  grouping_set`) compares the final result, after the projection drops the id.
  `executor_cases.inc:107-110` (`SumByKeyAndGroupingId`) is the merge over a state that
  *already carries* an id, and its device half manufactures the id with an `Int64` project
  literal (`test_gpu_executors/contract.rs:158-165`) — the expansion never runs there. The Python
  model pins the wrong rule on purpose (`scripts/exec_model/operators/aggregates.py:264-271`,
  `tests/test_operators.py:548,566`) and names this ticket as the reason.

## 3. Localized fix

**Make the C++ emit DataFusion's column: the same fold, the same width.** One function in one
C++ file, one contract case that runs on both engines, and the Python model brought to the
same rule. No planner, wire, ABI or fbs change.

### `cpp/src/operators/aggregate.cpp`

Add one static helper above `execute_aggregate` (beside `make_reduce_agg`, `:118`):

```cpp
// DataFusion's `__grouping_id`, which the plan declares and the CPU backend gets from
// DataFusion itself: the set's mask folded most-significant-first, so key position i is
// bit (nkeys - 1 - i), at the unsigned width `Aggregate::grouping_id_type` picks for the
// key count. Anything else disagrees with the CPU the moment a query reads the column.
static std::unique_ptr<cudf::column> grouping_id_column(uint64_t gid,
                                                        cudf::size_type nkeys,
                                                        cudf::size_type rows) {
  if (nkeys <= 8) {
    cudf::numeric_scalar<uint8_t> s(static_cast<uint8_t>(gid), true);
    return cudf::make_column_from_scalar(s, rows);
  }
  if (nkeys <= 16) {
    cudf::numeric_scalar<uint16_t> s(static_cast<uint16_t>(gid), true);
    return cudf::make_column_from_scalar(s, rows);
  }
  if (nkeys <= 32) {
    cudf::numeric_scalar<uint32_t> s(static_cast<uint32_t>(gid), true);
    return cudf::make_column_from_scalar(s, rows);
  }
  cudf::numeric_scalar<uint64_t> s(gid, true);
  return cudf::make_column_from_scalar(s, rows);
}
```

In the per-set loop, replace `:384-398` (the four-line comment, `int32_t gid = 0;` and the
loop body) with:

```cpp
      uint64_t gid = 0;
      for (cudf::size_type i = 0; i < nkeys; ++i) {
        bool masked = mask->Get(i);
        gid = (gid << 1) | (masked ? 1u : 0u);
        set_keys.push_back(masked ? null_placeholders[i]->view() : key_cols[i]);
      }
```

and replace `:414-415` with:

```cpp
      cols.push_back(grouping_id_column(gid, nkeys, gk->num_rows()));
```

Every include it needs is already there (`scalar_factories.hpp`, `column_factories.hpp`;
`numeric_scalar<int32_t>` is used at `:288`). Net: about +20 −10 lines. `nkeys > 64` is
DataFusion's own execution error (`group_id_array`, `:1268`) and the CPU fails there first;
no arm for it here.

### `peacockdb-core/tests/common/executor_cases.inc` — the pin, both engines

A new `Shape` and `Case`, so the contract table says what the id is:

```rust
    /// `sum(v) GROUP BY ROLLUP(k, v)` — the expanding init and the merge above it, with
    /// the id kept in the answer. Two keys, because one cannot show a bit order: the ids
    /// are DataFusion's, which the CPU emits by running DataFusion and the device by
    /// construction, and `expect` is where the two are held to one value.
    SumOverRollup,
```

```rust
    Case {
        name: "a rollup's grouping id is DataFusion's, in width and in bit order",
        shape: Shape::SumOverRollup,
        expect: &[
            "NULL|NULL|3|21",
            "a|2|0|2", "a|4|0|4", "a|6|0|6", "a|NULL|1|12",
            "b|1|0|1", "b|3|0|3", "b|5|0|5", "b|NULL|1|9",
        ],
    },
```

Rows are `k|v|__grouping_id|sum(v)`, sorted as strings by `answer` (`test_cpu_executors.rs:169`,
`contract.rs:72`); `rendered` prints a NULL cell as `NULL` (`ScalarValue::to_string`) on both
sides. Set `[F,F]` groups by `(k, v)` — six rows, id 0; `[F,T]` masks `v` — id 1 under
DataFusion's fold, **2 under today's C++**; `[T,T]` — id 3. The fixture has two columns, which
is why `v` is both a key and the aggregated argument.

Both `emitted` matches gain the arm (the enum is matched exhaustively, so the build says where):

- `test_cpu_executors.rs`, beside `SumByKeyAndGroupingId` (`:245`): state
  `columns(&[("k", Utf8), ("v", Int64), ("__grouping_id", UInt8), ("sum(v)", Int64)])`; init
  body `group_by: [column(0,"k"), column(1,"v")]`, `grouping_sets: vec![vec![false,false],
  vec![false,true], vec![true,true]]`, `null_exprs: vec![Expr::Literal(ScalarValue::Utf8(None)),
  Expr::Literal(ScalarValue::Int64(None))]`, `aggs: [Sum(column(1,"v")) → "sum(v)" Int64]`,
  `finalize: None`; merge body `group_by: [column(0,"k"), column(1,"v"),
  column(2,"__grouping_id")]`, no sets, `aggs: [Sum(column(3,"sum(v)"))]`; then
  `merged(input, state.clone(), init, merge, state)`. DataFusion produces
  `[k, v, __grouping_id:UInt8, sum(v)]` (`aggregates/mod.rs:930-945`, `create_schema` with
  `group_fields`), so `check_state_layout` (`cpu_backend/mod.rs:467`) passes.
- `test_gpu_executors/contract.rs`, beside `:158`: the same two bodies over
  `Schema::new(Arc::new(schema_of(&[…])))`, then `merged(source_per_row_group(), state.clone(),
  init, merge, state)`. The device runs the real expansion (`null_exprs` as typed NULLs cross
  the wire with `is_null`, `wire/serialize.rs:92-99`, and `build_scalar` builds them invalid,
  `cpp/src/expr.cpp:453-456`), and `merged_over` exports through `session.export(&out).unload`
  — the sink-schema concat at `gpu_backend/mod.rs:179`. **Red before the fix twice over**: the
  export is `Int32` against a declared `UInt8` (arrow's `RecordBatch::try_new` refuses the
  column type), and the middle set's rows read `a|NULL|2|12`.

### `scripts/exec_model/operators/aggregates.py` and `tests/test_operators.py`

The model is where a rule is argued, and it argues the wrong one on purpose:

- `aggregates.py:264-271` `grouping_set_id`: body becomes
  `functools.reduce(lambda acc, masked: (acc << 1) | int(masked), mask, 0)`; the docstring says
  it is DataFusion's fold and drops the #65 sentence. `rollup_masks`'s docstring (`:274-282`)
  "so the ids are 0, 2 and 3 rather than 0, 1, 2" → "so the ids are 0, 1 and 3".
- `test_operators.py:548` `{0, 2, 3}` → `{0, 1, 3}`; `:566` `== 2` → `== 1`. Nothing else reads
  the value: `test_end_to_end.py` and the TPC-DS plans use `GROUPING_ID` as a column name and
  compare results after the projection.

Runs in CI's cost-report job, so it is a red test before the change too.

### What it deliberately does not touch

- **The hashers.** No `UINT8` arm in `cpp/src/spark_hash_partition.cu:163-179` and no cast in
  `cpu_backend/spark_partitioning.rs:56-70`. Spark has no unsigned types; the id leaves the hash
  list under #189 (below). Adding an arm here is that proposal's rejected alternative 1.
- **The planner and the wire.** `translator/aggregate.rs` keeps declaring DataFusion's type;
  `aggregate_writer.rs`, `fb_text.rs`, `gpu_plan.fbs` unchanged. The payload golden prints masks
  and placeholders, never an id, so `recipe-payloads.txt` does not move.
- **The rest of `aggregate.cpp`**: the grouping-set "mean" arms (`:499-571`), the name sets and
  the `distinct` guard hacks-audit finding 10 names, `stddev_ddof` (finding 5). Same file, not
  this fix.
- **Unsigned literals and shifts on the device.** `build_scalar` (`expr.cpp:453-497`) and
  `build_expr`'s literal arms have no `UInt8..UInt64` case; `fb_to_binop` (`:498-521`) and the
  AST map have no `BitwiseShiftLeft/Right` although the fbs (`gpu_plan.fbs:85-86`) and the
  Rust IR carry them. `GROUPING()` projections need both; they are not #65.
- **`SumByKeyAndGroupingId`'s device half** (`contract.rs:158-165`) keeps its `Int64` literal
  id: that case is about `key_width`, and a `UInt8` literal cannot be built on a device today.

### How CPU and GPU stay one engine

The CPU never computes the id — DataFusion does — and the plan's declared type is DataFusion's.
The C++ now mirrors exactly two DataFusion functions, `group_id_array` and `grouping_id_type`,
and the mirror is held by a case both engines run against one `expect`: the value pins the
fold, and the device's export against a declared `UInt8` pins the width (a drift to `UInt16`
would be refused at `gpu_backend/mod.rs:179`). A DataFusion upgrade that changed either rule
would go red on the CPU half of the same case. The device's rollup state is also now priced at
what it holds: `produced()` (`gpu_backend/mod.rs:196`) prices from the declared schema, one
byte per row for the id, where the device was holding four.

### Pinning tests, goldens, registry rows and comments that change

- `executor_cases.inc`, `test_cpu_executors.rs`, `test_gpu_executors/contract.rs` — the case
  above. `test_cpu_executors` runs on dataset-matrix, `test_gpu_executors` on shad-gpu.
- `scripts/exec_model/operators/aggregates.py`, `tests/test_operators.py` — as above.
- `cpp/src/operators/aggregate.cpp:384-387` — the comment naming the ticket goes with the code.
- `peacockdb-core/src/planner/translator/tests.rs:653-655` — "#65 is about the id's ENCODING,
  which is execution-side" — drop the sentence; the shape claim stands.
- `llm-wiki/architecture.md:284-286` — "The ids are the bitmask of each set's **masked**
  positions — a two-key rollup gives 0, 2, 3 — which is distinct per set and not DataFusion's
  `GROUPING()` encoding (#65)." → "The id is DataFusion's: the mask folded most-significant-
  first, so a two-key rollup gives 0, 1, 3, at the unsigned width DataFusion picks for the key
  count — the CPU takes it from DataFusion and the C++ computes the same fold, and the executor
  contract holds the two to one answer."
- `llm-wiki/build-test.md:20` — "Eleven rows: … a merge over state whose keys carry a grouping
  id, and the scatter …" → twelve, adding "a rollup whose id is pinned to DataFusion's width and
  bit order". N stays 1 (one test over `CASES`).
- `testdata/cost-registry.csv:6,15,19,23,68,71,78,81,87` — `65` leaves the tickets column;
  every row keeps another live ticket, so `registry.rs:274-285` stays green.
- `llm-wiki/tickets.md:278-283` → `archive/archived-tickets.md`; the contents line `:19` drops
  `#65` (count 14 → 13); `:720`'s "Not #65, whose gid is the ROLLUP one" retargets to the archive
  anchor.
- **No golden regenerates.** No plan shape, no wire bytes, no CPU-authored execution section,
  no `--- memory ---` figure moves.
- Optional, red before the fix if DataFusion 45 plans it: add the section-5 query to
  `test_gpu_recipe_walk.rs`'s list — it is the one shape that reads the id through a real plan
  on a device. Contingent on the analyzer (risk 2), so the contract case is the required pin.

### Hacks-audit scaffolding

#65 was outside the audit's scope (`hacks-audit.md:8-9`) and nothing in production is shaped
around it: no branch, flag, mock knob or fixture avoids the id's value. What exists is the
C++ comment, the Python model's deliberate wrong rule and its two pins, and the registry
co-attributions, all removed above. Two audit items are respected rather than fought: finding 10
(`aggregate.cpp`'s name sets and dead `distinct` guard) sits in the same function and is left
alone; and #164's gap — "nothing checks that a child's column order is what the plan assumed",
and no C++ declared-type check — is what let this ship. After the fix the id is no longer a
would-be hit for #164; the fix does not add a check of its own.

### Relation to the #189 proposal

Complementary, independent, no shared lines. #189 (`189-proposal.md`, reviewed sound) removes
the id from the shuffle's key list in `planner/translator/aggregate.rs`; this fix changes what
the id *is* in `cpp/src/operators/aggregate.cpp`. The CPU/GPU lane divergence the 189 proposal
names ("Hashing the gid could never have agreed across the two engines anyway", its section 1)
is the value half of this ticket, and it is right that #189 removes it from the hash path
without waiting for this — after this fix the values agree but the type is still one neither
Spark hasher has an arm for, so the id has to leave the hash list regardless. Landing order does
not matter for any enabled cell: with #65 first, a device tp4 rollup would `CUDF_FAIL` on
`UINT8` at `spark_hash_partition.cu:179` exactly where the CPU refuses on comet — one refusal on
both engines instead of a refusal on one and a mis-placement on the other — and no such cell is
enabled (#152). With #189 first, the id never reaches a hasher and this fix is about the unload
and expressions only. Neither fix adds a hasher arm. The 189 review's finding 6(b)
(`architecture.md:294-295` becomes true under #189) is unaffected; the sentence this fix
rewrites is `:284-286`.

## 4. Alternatives rejected

- **Carry per-set ids, or the init's output schema, on the wire** (new `CudfAggregate` fields).
  A frozen-surface change plus a `recipe-payloads.txt` regen, to transport a value that is a
  pure function of the mask and key count the C++ already holds.
- **Declare what the C++ does** (`Int32`, LSB-first) on the Rust side. The CPU backend runs
  DataFusion and cannot produce that; `GROUPING()`'s rewrite assumes DataFusion's bits.
- **Remap the id in a project above the init** (a `CASE` per set). A node in every rollup plan on
  both engines, every rollup plan golden moves, and the C++ stays wrong underneath.
- **Add `UINT8` arms to both hashers so the id can be hashed.** #189's rejected alternative:
  invents a Spark hash for a type Spark lacks and widens the conformance gate for nothing.
- **Leave it until #23 and #143 land.** Keeps a silent device divergence in the tree, keeps the
  Python model asserting a wrong rule by name, and leaves every rollup as #164's first false
  positive — for a fix that is one C++ helper.

## 5. Minimum corpus query

```sql
SELECT n_regionkey, n_nationkey, GROUPING(n_regionkey, n_nationkey) AS g, count(*)
FROM nation GROUP BY ROLLUP(n_regionkey, n_nationkey);
```

Against `testdata/tpch.sf1`, integer keys and an `Int64` count so neither #183 nor #187 refuses
the export. `GROUPING(a, b)` in key order takes the analyzer's shortcut
(`resolve_grouping_function.rs:207-217`) and becomes `CAST(__grouping_id AS Int32) AS g` — a
plain cast the translator carries (`translator/expr.rs:72-77`) and the device evaluates on the
column path (`expr.cpp:912-934`, `cudf::cast`). Expected: 31 rows — 25 with `g=0`, 5 regional
subtotals with `g=1`, one grand total with `g=3`.

Today: the CPU answers that at every mode (nation is under `SMALL_TABLE_BYTES`, so one lane and
the same shape at all five; run at `tp1-single`). The **device** answers the five subtotal rows
with `g=2` — a silent wrong answer, not a refusal. It is reachable on a device only through the
recipe walk (`test_gpu_recipe_walk.rs`), which drives a plan at one lane against DataFusion; the
corpus device tier is not the vehicle because nothing rollup-shaped is enabled there. Whether
DataFusion 45 plans it is risk 2 below; if it does not, the contract case in section 3 is the
exposure and needs no SQL.

## 6. Cells re-enabled

**None directly.** No `corpus_query!` line and no registry cell is off on #65 alone. What comes
back is agreement: the device's rollup state matches its declared schema and the CPU's values,
which is a precondition for enabling any rollup device cell (q5, q80, rollup_over_join, q77,
q14/q18/q22 — all behind #152, #183, #175 or #163 today) and for q70/q86, which stay off behind
#23 (plan), #143 (`rank() OVER`), and two device gaps this fix does not close: unsigned literals
in `build_scalar`/`build_expr` and `BitwiseShiftLeft/Right` in `expr.cpp`, both of which
`GROUPING(x)` for a single key needs (`resolve_grouping_function.rs:219-241` emits
`(__grouping_id & 1u8) >> k`). q36 is a window query too. Registry rows lose the `65`
co-attribution and are otherwise unchanged.

## 7. Risks and unknowns

1. Not run. cuDF's `make_column_from_scalar` over a `numeric_scalar<uint8_t>`, `groupby` and
   `concatenate` over `UINT8` keys, and `to_arrow_host` exporting `UINT8` as Arrow `UInt8` are
   all standard cuDF surfaces; the export path (`gpu_executor.cpp:49-76`) widens only
   `DECIMAL32/64`, so nothing there re-types the column. High confidence, unverified.
2. Whether DataFusion 45 plans the section-5 query. `ResolveGroupingFunction` is registered
   (`datafusion-optimizer-45.0.0/src/analyzer/mod.rs:107`) and q70/q86's "not planned" is the
   window-embedded form, so a select-list `GROUPING()` should rewrite; not confirmed. The contract
   case does not depend on it.
3. The `UInt16/32/64` widths have no test: the corpus's widest rollup is q67 at eight keys, still
   `UInt8`. The arms are DataFusion's thresholds copied; a device test at nine keys would need a
   nine-column fixture.
4. On the CPU half of the new case, `v` as both a group key and the summed argument is legal SQL
   and legal IR, but no existing case does it; if DataFusion's partial objects, group by `(k, k)`
   would not distinguish the bit order either, and a third fixture column would be the fallback.
5. The device's `SumByKeyAndGroupingId` fixture keeps an `Int64` id. Aligning it to `UInt8` needs
   an unsigned literal on the device, which does not exist; leaving it means the contract has two
   rows about the id at two types, and the doc on the new one should say so.
6. `GpuAggregateBatches`'s compaction concatenates arriving batches; every batch now carries a
   `UINT8` id and the merge's `cudf::groupby` sees one type. If any other device path manufactures
   an id (none found: `grep __grouping_id cpp/` hits `aggregate.cpp:431` only), it would need
   the same width.

## 8. Complexity

**S.** One static helper and two edits in `cpp/src/operators/aggregate.cpp` (~+20/−10 lines);
one contract case across `executor_cases.inc`, `test_cpu_executors.rs` and
`test_gpu_executors/contract.rs` (~70 lines, mostly two `AggregateBody` literals); six lines in
the Python model and its tests; one sentence each in `architecture.md`, `build-test.md`, a test
comment and the C++ comment; nine registry cells and a ticket archival. No C ABI, FlatBuffers,
wire-format or declared-schema change; no golden regenerates. The cost above a Rust-only S is a
device build: the C++ moves and the pin is on shad-gpu, so the proving run is
`test_gpu_executors` there plus `test_cpu_executors` and the exec-model pytest here.
