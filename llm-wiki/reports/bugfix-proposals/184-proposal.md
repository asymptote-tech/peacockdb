# #184 — a hash repartition of one lane into four fails in cuDF

Read at master c18e063a, read-only. Paths relative to `/media/data/peacockdb`.

## 1. Issue

The ticket asks whether the kernel refuses the 1→N shape or the input carries something it
will not take. It is the input: **the shuffle key is a decimal, and the device hash kernel has
no decimal arm.** `cpp/src/spark_hash_partition.cu:179` is the `CUDF_FAIL` in the `default:`
of the key-type switch (`:163-185`), which lists `STRING`, `INT32`, `INT64` only (after the
`:135-156` normalisation of `DICTIONARY32`, `INT8/16`, `TIMESTAMP_DAYS`). The 1→N shape is
routine — every other corpus shuffle is `1→4` and q13's runs on a device.

The node the ticket names, `#11 CudfRepartition{Hash, 1→4}`, is at
`testdata/goldens/tpch.sf1/tp4-rowgroup.plans.txt:795,819` (and `tp4-sized`; it is `#15` at
`tp4-single.plans.txt:845`): `GpuEmitPartitions: hash=[total_revenue@4]` over a schema whose
column 4 is `total_revenue:Decimal128(38,4)`. Every decimal on the device is cuDF
`DECIMAL128` — the scan widens `DECIMAL32/64` (`cpp/src/operators/scan.cpp:98-110`), `fb_to_type_id`
maps `Decimal128 → DECIMAL128` (`cpp/src/expr.cpp:92`), and the aggregate casts its sum to
`DECIMAL128` (`aggregate.cpp:217-221`) — so the column reaches the switch as `type_id` 27 and
falls into the `default:`. The seq #1 shuffle on `s_suppkey:Int64` at the same modes succeeds
first, which is why the failure is reported at #11 and not #1; the third shuffle, `#22` on
`max(revenue0.total_revenue):Decimal128(38,4)`, would fail the same way.

This is ticket **#95** (`llm-wiki/tickets.md:294-300`, "Decimal partition keys ... `spark_hash_partition.cu`
throws a loud 'decimal partition key unsupported'") reached by a corpus query. The two are one
defect and close together.

**What it disables (correcting `00-tickets.md`, which says "tpch/q15 × 5 modes").** Only the
three tp4 device cells of `tpch/q15` reach a shuffle — at `tp1-single` and `tp1-rowgroup` the
plan has no `GpuEmitPartitions` at all (`tp1-single.plans.txt:531-560`), so those two cells fail
on #183 alone. Registry: `testdata/cost-registry.csv:115` carries `183 184` for the whole row;
`corpus_cases.inc:30` declares `gpu_modes = none`, with the comment at `:26-32` naming #184.

**Latent, not yet attributed.** Seven more corpus queries shuffle on a decimal key at the tp4
modes and would hit the same line the day their named tickets clear (read off
`tp4-single.plans.txt`, hash ordinal against the node schema):

| query | key | type | comet width | registry tickets today |
|---|---|---|---|---|
| tpch q10 | `c_acctbal@2` (in a 7-key group) | `Decimal128(15,2)` | 8 bytes | `152 183` |
| tpch q18 | `o_totalprice@4` | `Decimal128(15,2)` | 8 | `97 152 183` |
| tpch q2 | `ps_supplycost@7`, `min(ps_supplycost)@0` | `Decimal128(15,2)` | 8 | `152 187` |
| tpcds q24 | `i_current_price@6` ×2 | `Decimal128(7,2)` | 8 | `45 163` |
| tpcds q37, q82 | `i_current_price@2` | `Decimal128(7,2)` | 8 | `152 183` |
| tpcds q75 | `sales_amt@6` ×2 | `Decimal128(31,15)` | 16 | `97 152` |

Six of the seven take comet's **8-byte** path, which q15 (`p=38`) never exercises — a fix that
only hashed 16 bytes would pass q15 and misplace every one of them.

## 2. Root cause

**The hash both engines must agree on is comet's, and comet's decimal rule depends on the
declared precision, which cuDF's `data_type` does not carry.**

- CPU: `executor/cpu_backend/spark_partitioning.rs:42` hands the key arrays straight to comet's
  `create_murmur3_hashes`; `hash_keys` (`:56-72`) casts only the view types and passes a
  `Decimal128` through untouched. comet (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:299-304`)
  dispatches on the Arrow type: `Decimal128(p, _) if p <= 18` → `hash_array_small_decimal!`
  (`:108-126`), which hashes `i64::try_from(unscaled).to_le_bytes()` — **8 LE bytes**;
  otherwise `hash_array_decimal!` (`:128-146`), `unscaled.to_le_bytes()` — **16 LE bytes** of
  the i128. Nulls skip the update either way. Both are "no tail" lengths for `fmix(h, len)`.
- GPU: `spark_partition_ids` (`spark_hash_partition.cu:112-196`) takes
  `std::vector<cudf::size_type> key_cols` — ordinals only. `cudf::data_type` is `{type_id,
  scale}`; the precision that decides 8 vs 16 is not on the device at all. So even a kernel
  arm for `DECIMAL128` could not choose the width; the `default:` refuses instead.
- Wire: the recipe writer never writes the one slot that carries a field's precision.
  `PlanNode.output_schema: Schema` exists in `flatbuffers/gpu_plan.fbs:606-609`, and `Field`
  (`:236-248`) has `decimal_precision`, but `wire/writer.rs:102` and `:131` write
  `output_schema: None` for every node. `CudfRepartition` (`gpu_plan.fbs:492-498`) carries
  `hash_exprs: [Expr]` of bare `ColumnRef{index, name}` (`:105-108`), written by
  `wire/node_writer.rs:269-296`. The C++ arm (`cpp/src/node_session.cpp:352-402`) reads only
  the ordinals (`:388-398`) and calls `spark_hash_partition(tv, key_cols, n)` (`:401`). The
  collapse arm's comment at `:275-276` states the invariant as it stands: "the node's own
  output_schema is absent on a recipe plan".

Nothing at plan time constrains a hash key's type (`plan/partition_ops.rs:42-61` checks lane
count and ordinal range), and the plan goldens show the CPU running every decimal-keyed
shuffle — so the defect is device-only and the fix is device + wire, not planner.

## 3. Localized fix

Thread the declared precision from the plan to the kernel through the field the fbs already
has, and add the decimal arm comet has. No fbs change, no new ABI symbol, no plan-shape change.

### 3a. Rust — write the repartition node's `output_schema`

`peacockdb-core/src/wire/writer.rs`
- `push(&mut self, payload)` (`:95-108`) gains a second parameter
  `output_schema: Option<WIPOffset<fb::Schema<'a>>>` and passes it into `PlanNodeArgs`
  (`:102`). `node()` (`:53-66`), `stub()` (`:87`) and `reduce()` (`:134`) pass `None`.
- New method beside `node()`:
  ```rust
  /// A node that also publishes its declared schema. One kind needs it: the repartition,
  /// whose kernel wants a type fact cuDF's `data_type` does not carry — a decimal key's
  /// precision decides how many bytes comet hashes. Written before the payload, so the
  /// bytes are one fixed order.
  pub(crate) fn node_with_schema<F>(&mut self, arity: usize, schema: &SchemaRef, build: F)
      -> Result<Seq, PlanError>
  ```
  body = `node()`'s with `let schema = serialize_schema(&mut self.builder, schema);` between
  `take` and `build`, and `self.push(payload, Some(schema))`. `serialize_schema` is
  `wire/serialize.rs:136-172` and already writes `decimal_precision`/`decimal_scale` per field;
  it is what `CudfUnion.output_schema` (`node_writer.rs:87`) and `aggr_input_schema`
  (`aggregate_writer.rs:74`) use.

`peacockdb-core/src/wire/attach.rs:303-322`, `emit_partitions`: replace `writer.node(1, …)`
(`:313`) with `writer.node_with_schema(1, &node.kind().schema().expect("an emitter is not a
sink").fields, …)`. The emit node's declared output schema equals its input's, so either is
the right thing to write; use the node's own.

`peacockdb-core/src/wire/fb_text.rs:173-185`, the `CudfRepartition` arm: add
`if let Some(schema) = node.output_schema() { field("schema", schema_text(&schema)); }` so the
payload golden shows what the executor now reads. Extend `schema_text` (`:221-233`) to print
`Decimal128(p,s)` for a decimal field (today it prints the bare enum name), since the precision
is the load-bearing value; this also touches the `CudfScan` `schema:` lines in the golden text.

### 3b. C++ — read the precision and hash the decimal

`cpp/include/peacock/partitioning.hpp:28-54`
```cpp
/// One shuffle key. comet hashes a decimal of precision <= 18 as its unscaled i64 (8 LE
/// bytes) and a wider one as the i128 (16 LE bytes); cuDF's data_type carries the scale
/// and not the precision that decides between them, so the plan's figure rides here.
struct HashKey {
  cudf::size_type column;
  uint8_t decimal_precision;  // 0 for a non-decimal column
};
```
Both `spark_partition_ids` and `spark_hash_partition` take `std::vector<HashKey> const& keys`
in place of `key_cols`. Three callers, all updated below.

`cpp/src/spark_hash_partition.cu`
- add `#include <cudf/fixed_point/fixed_point.hpp>`;
- new kernel next to `spark_hash_fixed_col_kernel` (`:87-97`):
  ```cpp
  // A DECIMAL128 key: comet hashes the unscaled value as i64 (8 LE bytes) below precision
  // 19 and as the whole i128 (16 LE bytes) above; on a little-endian device the low bytes
  // of the __int128_t are its LE prefix, so `width` bytes at &v are comet's bytes.
  __global__ void spark_hash_decimal128_col_kernel(cudf::column_device_view col,
                                                   uint32_t* hashes, cudf::size_type n,
                                                   int width) {
    auto const row = ...; if (row >= n) return; if (col.is_null(row)) return;
    __int128_t const v = col.element<numeric::decimal128>(row).value();
    hashes[row] = spark_hash_bytes(reinterpret_cast<char const*>(&v), width, hashes[row]);
  }
  ```
  (`column_device_view::element<T>` has the fixed_point overload at
  `third_party/cudf/cpp/include/cudf/column/column_device_view.cuh:159`; `value()` is
  `fixed_point.hpp:295`.) `spark_hash_bytes` (`:52-67`) already handles 8 and 16: no tail,
  `fmix32(h, len)`, bit-for-bit comet's `hash_bytes_by_int` + `fmix`.
- loop `for (auto const ci : key_cols)` (`:129`) becomes `for (auto const& key : keys)` with
  `input.column(key.column)`; new case in the switch (`:163`):
  ```cpp
  case cudf::type_id::DECIMAL128: {
    CUDF_EXPECTS(key.decimal_precision > 0,
                 "peacock spark_partition_ids: a DECIMAL128 key needs the plan's declared precision");
    int const width = key.decimal_precision <= 18 ? 8 : 16;
    spark_hash_decimal128_col_kernel<<<grid, block, 0, stream.value()>>>(*dcol, hashes.data(), n, width);
    break;
  }
  ```
- `default:` message (`:180-184`): list `DECIMAL128 (with its declared precision)` as
  supported; `DECIMAL32/64` stay unsupported on purpose — nothing produces them on this side
  (scan widening) and comet's Arrow has no such types.
- `spark_hash_partition` (`:198-207`) forwards `keys`.

`cpp/src/node_session.cpp:388-402`, the repartition arm:
```cpp
// The declared type of each key: cuDF has the scale of a decimal and not the precision,
// and the precision is what decides how many bytes comet hashes. The recipe writer puts
// the node's schema here for exactly this node.
const fb::Schema* schema = node->output_schema();
if (!schema || !schema->fields())
  throw std::runtime_error("CudfRepartition: no output_schema — the hash needs each key's declared type");
std::vector<peacock::partitioning::HashKey> keys;
for each hash expr (ColumnRef, as today):
  auto const idx = e->node_as_ColumnRef()->index();
  if (idx >= schema->fields()->size()) throw std::runtime_error("CudfRepartition: hash key past the declared schema");
  const fb::Field* f = schema->fields()->Get(idx);
  keys.push_back({static_cast<cudf::size_type>(idx),
                  f->data_type() == fb::DataType_Decimal128 ? f->decimal_precision() : uint8_t{0}});
```
then `spark_hash_partition(tv, keys, n)`. Same read pattern as `union.cpp:35-47`. Amend the
collapse comment at `:275-276`: the schema is absent on a recipe plan *except at a
repartition*, which is the one node whose kernel needs a declared type.

`cpp/src/gpu_executor.cpp:331-358`, the conformance hook `peacock_spark_partition_ids`: it
receives the Arrow C-data schema, whose child `format` string carries the precision
(`d:P,S` for a decimal128). Add a file-local
`uint8_t decimal_precision_of(const ArrowSchema& child)` — `format` starts with `"d:"` →
`strtoul(format + 2)`, else 0 — and build `std::vector<HashKey>` from
`schema->children[key_cols[i]]` alongside the ordinals (`:341-346`). The ABI signature does
not change. (`cudf::from_arrow` maps Arrow decimal128 → `DECIMAL128`,
`third_party/cudf/cpp/src/interop/arrow_utilities.cpp:70-71`, so the hook then meets the same
arm the executor does.) Do not use `arrow::ImportType` here: it releases the schema cuDF still
reads.

`cpp/tests/gpu/test_cudf.cpp:81-91`: `gpu_partition_ids` builds `HashKey`s (precision 0) from
its `cols`; the two existing tests are otherwise unchanged.

### 3c. What it deliberately does not touch

- `spark_partitioning.rs` and `cpu_backend/emit.rs`: the CPU is already comet and is the
  reference; nothing moves there.
- `gpu_plan.fbs`, the sixteen ABI symbols, `read.rs`'s child walk, seq numbering, plan shape,
  `*.plans.txt`, `*.cpu.txt`, `*.cost.txt`, `*.result.txt` — unchanged. Only
  `recipe-payloads.txt` moves (below).
- The Python model (`scripts/exec_model/operators/partition_ops.py`) uses a crc32 stand-in by
  design and is untouched.
- `PlanNode.output_schema` on every other node stays unwritten (the general form is
  alternative 4).

### 3d. How CPU and GPU stay one engine

Both hash comet's bytes of the same value at the same declared precision: comet reads it off
the Arrow array's type — which `declared_as` (`cpu_backend/mod.rs:239`) relabels to the plan's
declared schema at every stage — and the device reads the same `Field.decimal_precision`
written from `node.kind().schema()`. The live gate (`peacock_spark_partition_ids` vs
`create_murmur3_hashes` over the same bytes, `test_inc2_conformance.rs:85-128`) is what proves
it, and it gets decimal cases (below). No new rule is written on the Rust side, so nothing can
drift there.

### 3e. Tests, goldens, registry rows, comments that move with it

Red before the fix:
- `peacockdb-core/tests/test_inc2_conformance.rs`: three `*_match_comet_live` gates, each
  `#[cfg(not(feature = "rust-only"))]` like `:131`:
  `Decimal128(15,2)` (values incl. 0, negatives, 999999999999999 unscaled, a null),
  `Decimal128(38,4)` (values that do not fit in i64, e.g. ±10²⁰, plus small ones and a null),
  and a composite `(Int64, Decimal128(15,2), Utf8)` for the seed chain. Today the hook returns
  non-zero on the `CUDF_FAIL` and `assert_eq!(rc, 0)` at `:120` fails. Update the header
  `:12-14` ("Covered key shapes … decimal at both widths").
- `cpp/tests/gpu/test_cudf.cpp`: one gtest using `cudf::test::fixed_point_column_wrapper<__int128_t>`
  at the two precisions, expected ids taken from comet (the Rust gate's `eprintln!` prints
  them; `cpu_partition_ids` runs on a CPU host).
- `peacockdb-core/tests/test_gpu_recipe_walk.rs`: a shuffled aggregate on a decimal key at
  `TWO_LANES` beside `SUM_BY_FLAG` (`:634-641`, `:685-700`) — e.g.
  `SELECT l_discount, sum(l_quantity) FROM lineitem GROUP BY l_discount` — the first whole
  plan whose `CudfRepartition` carries a decimal key across the wire to a device. Compares the
  result against DataFusion; kinds are already in `PROVEN` (`:797-812`).

Regenerated:
- `testdata/goldens/recipe-payloads.txt`: every query with a shuffle changes bytes and
  `sha256=` (q13, q15, q21, q22, shuffle-stddev and the tpcds set), and gains a `schema:`
  line under each `CudfRepartition`; scan `schema:` lines change if `schema_text` prints
  precision. Deliberate regen: `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1` with the
  `/tmp/peacock-plan-bytes-root` symlink (build-test.md, Golden files).
- `build-test.md` test counts: the murmur3 row `N` (10 → 13) and the grand total.

Registry and comments:
- `testdata/cost-registry.csv:115`: `183 184` → `183`. No cell flips (see 6).
- `peacockdb-core/tests/common/corpus_cases.inc:26-32`: drop "#184 is a cuDF failure in the
  hash scatter" from the four causes.
- `llm-wiki/build-test.md:44`: drop #184 from "the rest are off against …".
- `llm-wiki/tasks/active-tickets.md:85-92` (#184) and `llm-wiki/tickets.md:294-300` (#95): both
  to `archive/archived-tickets.md` as Done; `tickets.md:19` blocker count 14 → 13; the #195
  bullet at `tickets.md:775-776` ("a shuffle keyed on a decimal: not a query but #95's kernel
  work") now has a query (q15) and a gate — rewrite or drop.
- `llm-wiki/architecture.md`: `:824` (`CudfRepartition` row — "what steers it" gains "and each
  key's declared type from the node's `output_schema`, the precision of a decimal deciding
  its hashed width"); `:960-972` (Rehash section — one sentence on decimal width); the
  `:1064-1076` table — a new row `PlanNode.output_schema` on `CudfRepartition` / `writer.rs`
  via `attach.rs` / the emit node's declared schema / `HashKey.decimal_precision`.

### 3f. hacks-audit scaffolding

None grew around this defect; the audit names nothing on the partition path. Two comments it
would have flagged after the fix are handled above: the `default:` message's type list
(`spark_hash_partition.cu:180-184`) and the collapse arm's "output_schema is absent on a
recipe plan" (`node_session.cpp:275`). Not to be added: a planner refusal of decimal hash keys
(that is building around the bug — the CPU runs them today).

## 4. Alternatives rejected

1. **Hash `DECIMAL128` as 16 bytes always, no precision** — fixes q15 (p=38) and misplaces
   every p ≤ 18 key (q10, q18, q2, tpcds q24/q37/q82) relative to the CPU; the per-lane
   `batch_rows` golden would catch it, and the gate would go red.
2. **Widen p ≤ 18 decimal keys to p=38 on the CPU (hash-only cast in `hash_keys`) and hash 16
   bytes on the device** — the engines agree, but the engine's hash stops being comet's; the
   gate can no longer compare the kernel against raw comet, and #95's stated design is the
   opposite.
3. **Add a precision field to `CudfRepartition` in `gpu_plan.fbs`** — a wire-schema change for
   a fact `PlanNode.output_schema` + `Field.decimal_precision` already model.
4. **Write `PlanNode.output_schema` on every node** — the general form; every payload's bytes
   move and ~15 `attach.rs` call sites change signature; no reader needs it yet (#173's
   collapse is the next candidate, and it can adopt this pattern then).
5. **Planner-inserted cast of the key to Int64/binary before the shuffle** — changes plan shape
   on both engines and every tp4 golden with a decimal key, for a device-only defect.
6. **Extend `peacock_spark_partition_ids` with a precision array** — an ABI change; the Arrow
   C-data schema the hook already receives carries the precision in its format string.
7. **Turn the crash into a named plan-time refusal** — a `bug_` shape, and the CPU answers
   these queries today.

## 5. Minimum corpus query

Not in `testdata/`; tpch sf1.

8-byte path (the one q15 does not exercise):
```sql
SELECT l_discount, count(*) FROM lineitem GROUP BY l_discount
```
Key `l_discount:Decimal128(15,2)`. lineitem is above `SMALL_TABLE_BYTES`, so at **tp4-single**
(also tp4-rowgroup, tp4-sized) the aggregate sequence puts `GpuEmitPartitions:
hash=[l_discount@0]` → `CudfRepartition{Hash, 1→4}` above the per-lane merge. Plans today;
runs and matches DataFusion on the CPU; on a device fails at that node's first call with
`CUDF failure at cpp/src/spark_hash_partition.cu:179: peacock spark_partition_ids: unsupported
key column cuDF type_id=27` (`DECIMAL128`). At tp1-single/tp1-rowgroup the plan has no shuffle
and the query does not expose the issue.

16-byte path (q15's shape, smaller):
```sql
SELECT l_extendedprice * l_discount AS x, count(*) FROM lineitem GROUP BY x
```
Key `x:Decimal128(31,4)`. Same modes, same refusal.

## 6. Cells re-enabled

**None flip today.** `tpch/q15`'s three tp4 device cells lose their #184 wall and stay off
behind #183 (`s_name`, `s_address`, `s_phone` are `Utf8View` at the unload); the two tp1 cells
were #183 only. Registry row 115 becomes `183`. When #183 lands, the tp4 cells are the ones this
fix makes possible — subject to #185 (q15 has `GpuAggregateBatches` and has never completed
on a device, so whether #185 bites it is unmeasured).

Latently unblocked, no registry change now: tpch q10, q18, q2; tpcds q24, q37, q75, q82 at the
tp4 modes (table in section 1), each behind its own named tickets.

## 7. Risks and unknowns

- Not compiled: `col.element<numeric::decimal128>(row)` inside a `__global__` and the include
  set; the fixed_point overload exists in the vendored cuDF and cuDF's own device code uses
  it. Fallback if needed: `col.element<__int128_t>(row)` via `is_rep_layout_compatible`.
- The ticket quotes only the failing line, not the message; DECIMAL128 is the only type at that
  node the switch does not list, so the attribution is by elimination, not by transcript.
- The two-arm width rule assumes a p ≤ 18 column's values fit in i64 — comet `unwrap()`s the
  same assumption. A DataFusion column declared `Decimal128(15,2)` but carrying a wider value
  would panic on the CPU and hash garbage on the device; no corpus column does this.
- CPU-side agreement depends on the Arrow array's precision at the emit node equalling the
  declared one; `declared_as` (`cpu_backend/mod.rs:239`) is what guarantees it and I did not
  trace every stage.
- Whether q15 at tp4, past this node, trips #185 or anything else before the unload's #183 —
  unmeasured; #152 should not fire (each join sees one probe batch per lane by the plan).
- The payload regen needs the `/tmp` symlink environment; the text golden changes more lines if
  `schema_text` gains precision (recommended, optional).
- cuDF 26.02 leg compiles only; `numeric::decimal128` and `column_device_view::element` are
  stable across 25.02/26.02.
- FlatBuffers verifier depth (#169): a `Schema` under a `PlanNode` adds two table levels on a
  side branch, not on the input spine; headroom is 382 of 1024.

## 8. Complexity

**M.** ~10 files, ~200 LOC: `partitioning.hpp` (struct + 2 signatures), `spark_hash_partition.cu`
(kernel + arm), `node_session.cpp` (~15 lines), `gpu_executor.cpp` (hook, ~15), `writer.rs`,
`attach.rs`, `fb_text.rs` (small each), three test files, plus wiki/registry/comment edits.
Frozen surfaces: **no** ABI symbol change, **no** `.fbs` change, **no** declared-schema
contract change; the public C++ header `partitioning.hpp` changes signature (three callers,
none outside the repo). Goldens: only `recipe-payloads.txt` is regenerated (bytes + digest for
every shuffle query, a `schema:` line per repartition); no plan, execution, cost or result
golden moves.
