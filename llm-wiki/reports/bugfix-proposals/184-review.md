# #184 — review of the proposal

Read at master c18e063a, read-only. Every cite below was opened. Paths relative to
`/media/data/peacockdb`.

## 1. Verdict

**Needs changes.** Root cause and mechanism are right and well evidenced; what is missing is a
test that proves the *wire* path places rows where the CPU does, a statement of where this lands
against the approved chain that already plans the same case and moves the same files, and a
16-byte "minimum query" that would be refused one node below the shuffle on a device.

## 2. Findings

### F1 — important — nothing proposed proves placement through the production path

The fix has two halves: the kernel arm (proved by the conformance gate, `test_inc2_conformance.rs:85-128`,
which feeds precision from the Arrow C-data `format` string in `gpu_executor.cpp`) and the wire
threading `attach.rs` → `PlanNode.output_schema` → `node_session.cpp:388-398` → `HashKey`. The only
test on the second half is the recipe-walk query (`test_gpu_recipe_walk.rs`, `TWO_LANES`), and that
walk compares the *result* against DataFusion as a multiset. A wrong width — or a precision read
from the wrong field — still lands every row of one key in one lane, so the aggregate is correct and
the walk stays green. It goes red only where precision arrives as 0 (`CUDF_EXPECTS`), i.e. absence,
not error. No enabled device cell carries a `CudfRepartition` (registry: q6 has no shuffle, q19 is
tp1-single), and q15 stays off behind #183, so nothing in the tree would observe a misplacement.

Correction: one device test that drives a recipe-built scatter on a decimal key and compares lanes
against the CPU. Cheapest shape in `peacockdb-core/tests/test_gpu_executors/join.rs` beside
`a_scatter_answers_with_one_handle_per_lane` (`:316`): `GpuEmitPartitions(GpuProject(source(),
[cast(v@1 → Decimal128(15,2)) as d, k@0]), hash=[0], lanes=4)`, expected per-lane rows from
`create_murmur3_hashes` over the same `Decimal128Array` built in the test (the pattern
`test_inc2_conformance.rs:33` already uses), asserting rows per lane, not just counts. A second
at `Decimal128(38,4)` with a value past i64. Or see F2: `operator-cases.md` plans exactly this
case (`a_decimal_key_places_every_row_the_same`, `operator-cases-impl.md:833`) as `run_both(...)
.same(Order::Any)` per lane — if it lands first, that is the regression test and the fix deletes its
`bug_` form in the same change.

### F2 — important — the fix lands in the middle of an approved chain and the proposal does not say where

`llm-wiki/tasks/tasks.md`, chain `ENS-drop-mode-name`, tasks 3–13 all `approved to build`:

- `test-layout.md` (task 4) moves `test_inc2_conformance.rs` ("plus the murmur gate") and the recipe
  walk into `src/`; the fix edits both files in their current location.
- `operator-cases.md` (task 9) plans the decimal-key scatter case as a `bug_` witness for
  #184/#95 (`operator-cases.md:36`, `operator-cases-impl.md:792-798, 833, 874`). Coding-style
  "Building around a bug": the `bug_` test is deleted in the fixing change.
- `declared-schemas.md` (task 10) keeps `Writer::push`'s `output_schema: None` and says "Putting
  schemas on the wire is a later task's work" (§4); it also names `fb_text.rs`'s `schema_text`
  dropping precision as out-of-scope drift that "gets a ticket". This fix is that later task for
  one node, and its `schema_text` change is that ticket's fix — say so, and render the field the
  way `plan_text`'s `type_text` does (`Decimal128(p,s)`), one spelling.
- `walk-drives-every-plan.md:126` and `-impl.md:106` list #95 among four "known refusals" the
  walk task proves its refusal-reporting on; closing #95 removes one of the four.

Correction: a "Sequencing" paragraph in §7 — land before task 4 (files where they are today) or
after it (in `src/wire/gpu_tests/` and the moved gate), never during; name the `bug_` test to
delete if task 9 has landed; note the two `schema_text`s. None of this changes the fix; all of it
is what a developer hits in the first hour.

### F3 — important — the 16-byte minimum query is refused one node below the shuffle on a device

§5's second query, `SELECT l_extendedprice * l_discount AS x, count(*) FROM lineitem GROUP BY x`:
DataFusion resolves the alias and puts the `BinaryExpr` inline in `AggregateExec`'s group-by (CSE
extracts only *common* sub-expressions, `datafusion-optimizer-45.0.0/src/common_subexpr_eliminate.rs:240-290`,
so a single occurrence stays inline). The device's aggregate arm takes `ColumnRef` group keys only
(`cpp/src/operators/aggregate.cpp:162-163`, `"CudfAggregate: only ColumnRef group exprs supported"`),
and the Partial aggregate runs before the shuffle. The query fails at `#N CudfAggregate{Partial}`,
never reaching `spark_hash_partition.cu:179`. The corpus knows this: `testdata/tpcds-queries/q99.sql:28`
was rewritten to compute `SUBSTRING(...) w_substr` in a subquery so the group key is a column, and
every `group_by=[…]` in every plan golden is a column reference (checked over both `tp1-single.plans.txt`).

Correction: `SELECT x, sum(l_quantity) FROM (SELECT l_extendedprice * l_discount AS x, l_quantity
FROM lineitem) GROUP BY x` — key `x:Decimal128(31,4)` as a `ColumnRef` over a `GpuProject`, same
tp4 modes. `SELECT DISTINCT l_extendedprice * l_discount FROM lineitem` also works (Distinct lowers
to an aggregate over the projection's columns). Prefer `sum(l_quantity)` over `count(*)` in both
queries: #180 (`active-tickets.md:263`) is a shuffled `count(*)` failing on the CPU at exactly the
tp4 modes, and the recipe-walk query already uses `sum(l_quantity)`.

### F4 — important — a scale mismatch between the device column and the declaration is a silent misplacement

comet hashes the *unscaled* value. The device arm hashes the `__int128_t` the column holds; if its
`scale()` is not `-decimal_scale` of the declared field, the unscaled integer differs from the CPU's
by a power of ten and every row lands in a different lane with nothing red — the conformance gate
cannot see it (Arrow arrays always carry the right scale) and the recipe walk cannot either (F1).
`union.cpp:31-33` records cuDF drifting fixed_point scale per branch, and `expr.cpp:562-606`'s
scale rules are ours, so the mismatch is not an impossible scenario.

Correction: in the `node_session.cpp` arm, beside the precision, check the column's scale against
the field's: `CUDF_EXPECTS(tv.column(idx).type().scale() == -f->decimal_scale(), "...")` — one
line, and the failure mode becomes a named refusal. (A hash-only `cudf::cast` to the declared
scale is the alternative; a throw is better because the same column is also a join or group key
above, where the wrong scale is a wrong answer too.)

### F5 — minor — mis-cites and an unverifiable claim in §1

- "the aggregate casts its sum to `DECIMAL128` (`aggregate.cpp:217-221`)": those lines are the
  `avg` input cast (`is_avg && …`). No cast makes the sum DECIMAL128; cuDF's groupby SUM over a
  fixed_point column returns the input type. Conclusion (type_id 27 reaches the switch) holds;
  the cite does not.
- "q13's [1→4 shuffle] runs on a device": q13's five device cells are off on #152
  (`cost-registry.csv:113`), which refuses at executor construction "by name rather than by
  crash" (`corpus_cases.inc:28`), so no q13 node has run on a device in the corpus tier. The
  1→N shape *is* proved on a device — `a_scatter_answers_with_one_handle_per_lane` (1→3, Int64)
  and `SUM_BY_FLAG` at `TWO_LANES` (1→2, string) — cite those.
- §3f "the audit names nothing on the partition path": true because `spark_partitioning.rs` is
  in the audit's "What I did not read" (`hacks-audit.md:364`), not because it was read and found
  clean.

### F6 — minor — the count changes in `build-test.md` are three rows, not one

The murmur3 row (10 → 13) is named. The new gtest moves "cuDF GPU smoke (C++)" (5 → 6) and the
new walk query moves "Recipe walk on a device (Rust)" (10 → 11); the walk's `PROVEN` cover list
at `test_gpu_recipe_walk.rs:826` also needs the new `(query, knobs)` pair or its
`PROVEN claims … and no query here produces it` assertion is unchanged but the new test is not
in the cover.

### F7 — minor — `node_with_schema` as a copy of `node`

§3a writes `node_with_schema` with "body = `node()`'s with one line inserted". Coding-style's
first rule: make `node()` delegate — `node(arity, build)` → `node_with_schema(arity, None, build)`
(or give `node` an `Option<&SchemaRef>`). Ten lines, but two copies of the take/number/build/push
sequence is how the three post-order rules `writer.rs:1-9` describe get one edit and not the other.

### F8 — minor — the i64 overflow risk in §7 is smaller than stated

"A DataFusion column declared `Decimal128(15,2)` but carrying a wider value would panic on the
CPU": `declared_as` casts with `safe: false` (`cpu_backend/mod.rs:254-263`) and *errors* on a value
that does not fit the declared precision, before the batch reaches the emitter; a parquet-declared
(15,2) column cannot carry one. comet's `unwrap()` is unreachable through this engine. Drop or
reword the risk.

## 3. Claims verified

- `spark_hash_partition.cu:179` is the `CUDF_FAIL` in the `default:` of the key-type switch
  (`:163-185`); arms are STRING/INT32/INT64 after the `:135-156` normalisation; DECIMAL128 is
  cuDF `type_id` 27 (`third_party/cudf/cpp/include/cudf/types.hpp:203-235`).
- `tp4-rowgroup.plans.txt:789-862`: `#11 CudfRepartition{Hash, 1→4}` is
  `GpuEmitPartitions: hash=[total_revenue@4]` over `total_revenue:Decimal128(38,4)`; `#1` is on
  `s_suppkey:Int64` and is the build subtree, so it runs first; `#22` is on
  `max(revenue0.total_revenue):Decimal128(38,4)`. `#15`/`#32` at tp4-single. No
  `GpuEmitPartitions` in q15 at tp1-single or tp1-rowgroup (0 hits in both files).
- The seven latent queries and their key types match a scan of every `GpuEmitPartitions: hash=`
  line against its schema in both `tp4-single.plans.txt`: q10 (15,2), q18 (15,2), q2 (15,2) ×2,
  tpcds q24 (7,2) ×2, q37 (7,2), q82 (7,2), q75 (31,15) ×2. Six of seven are p ≤ 18.
- comet: `Decimal128(p, _) if p <= 18` → `hash_array_small_decimal!` (i64 LE, 8 bytes), else
  `hash_array_decimal!` (i128 LE, 16 bytes), nulls skipped
  (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/utils.rs:107-146, 299-304`); 8 and 16 have
  no tail and `fmix(h1, len)` (`murmur3.rs:70-134`). `spark_hash_bytes` matches.
- CPU: `spark_partitioning.rs:42` hands arrays to `create_murmur3_hashes`; `hash_keys` (`:56-72`)
  casts view types only. Every emit input passes `declared_as` or `as_declared`
  (`cpu_backend/mod.rs:188`, `join.rs:250, 265`, `accumulate.rs:431`, `source.rs:117`), so the
  CPU hashes the declared precision.
- `cudf::data_type` is `{id, scale}`; the device has no precision and exports every decimal at 38
  (#187, `declared-schemas.md` §3) — the precision must come from the plan.
- `writer.rs:102, :131` write `output_schema: None`; `PlanNode.output_schema: Schema`
  (`gpu_plan.fbs:605-609`); `Field.decimal_precision: uint8` (`:236-248`); `serialize_schema`
  (`serialize.rs:136-164`) writes it; `node_session.cpp:388-398` reads ordinals only and calls
  `spark_hash_partition(tv, key_cols, n)` at `:401`; `:275-276` states the "absent on a recipe plan"
  invariant; `union.cpp:35-47` is the read pattern.
- Three callers of `spark_partition_ids`/`spark_hash_partition`: `node_session.cpp`,
  `gpu_executor.cpp:347`, `test_cudf.cpp:86`. `multi_gpu.cpp:160` `hash_shuffle` uses
  `cudf::hash_partition`, not ours. `cpp/install/` is untracked.
- `column_device_view::element<T>` has the fixed_point overload
  (`column_device_view.cuh:159-162`); `from_arrow` maps arrow decimal128 → `DECIMAL128`
  (`arrow_utilities.cpp:70-71`); the hook catches and returns 1 (`gpu_executor.cpp:355-359`), so
  `assert_eq!(rc, 0)` at `test_inc2_conformance.rs:120` is what goes red today.
- `payload_text` takes `&fb::PlanNode` (`fb_text.rs:14`), so `node.output_schema()` is in reach;
  `schema_text` (`:221-233`) prints the bare enum.
- Registry `cost-registry.csv:115` `183 184`; `corpus_cases.inc:30` `none`, comment `:26-32`;
  `build-test.md:44`; `tickets.md:19` count 14; #95 at `tickets.md:294-300` and its text already
  says "thread precision through the partition FFI"; the #195 bullet at `:775-776`;
  `architecture.md:824, 960-972, 1072`. `partition_ops.rs:42-61` checks lane count and ordinal
  range only. `partition_ops.py` is crc32 by design.
- No enabled device cell carries a `CudfRepartition`, so the arm change cannot regress the six
  cells; the recipe walk at `TWO_LANES` exercises the new `output_schema` read with a string key.
- Ten tests in `test_inc2_conformance.rs`, so 10 → 13 is right for that row.

## 4. Corrected proposal

Only the sections that change.

### 3b (addition)

In the `node_session.cpp` arm, for a `Decimal128` field also check the column's scale:
```cpp
CUDF_EXPECTS(tv.column(idx).type().scale() == -static_cast<int32_t>(f->decimal_scale()),
             "CudfRepartition: a decimal key's device scale differs from the plan's — the hash "
             "is over the unscaled value and would place rows differently from the CPU");
```

### 3a (shape)

`Writer::node` delegates to `node_with_schema(arity, None, build)` rather than the two carrying
one body each. `fb_text::schema_text` prints `Decimal128(p,s)` for a decimal field — the same text
`plan_text`'s `type_text` produces — and the branch names this as the fix for the drift
`declared-schemas.md` §4 defers.

### 3e (tests) — replace the device-side list with

Red before the fix, on a device:
- the three conformance gates as proposed (hook path; precision from the Arrow `format`);
- **one placement test through the wire**: `test_gpu_executors/join.rs`, a recipe-built
  `GpuEmitPartitions` over `GpuProject(source(), [cast(v → Decimal128(15,2)) as d, k])`, hash on
  `d`, four lanes; expected rows per lane from `create_murmur3_hashes` over the same
  `Decimal128Array` in the test; a second at `Decimal128(38,4)` with a value past i64. This is
  the only test in the set that goes red on a wrong width or a wrong field read;
- the recipe-walk query as proposed (proves the arm runs end to end; add it to `PROVEN`'s cover
  list at `:826`);
- the gtest as proposed.

If `operator-cases.md` has landed, `a_decimal_key_places_every_row_the_same` exists as a `bug_`
test: the fix flips it to the green assertion and deletes the `bug_` form in the same change.

`build-test.md`: murmur3 10 → 13, cuDF GPU smoke 5 → 6, recipe walk 10 → 11, executors on a
device 31 → 32 (or 33), and the grand total.

### 5. Minimum corpus query — replace the 16-byte query with

```sql
SELECT x, sum(l_quantity)
FROM (SELECT l_extendedprice * l_discount AS x, l_quantity FROM lineitem)
GROUP BY x
```
Key `x:Decimal128(31,4)` as a `ColumnRef` over a `GpuProject`; at the tp4 modes the shuffle hashes
it. The inline form (`GROUP BY l_extendedprice * l_discount`) is refused at the Partial aggregate
(`aggregate.cpp:162`) before any shuffle runs. Use `sum(l_quantity)` in the 8-byte query too
(#180).

### 7. Risks and unknowns — add

**Sequencing.** Chain `ENS-drop-mode-name` (tasks 3–13 approved) moves `test_inc2_conformance.rs`
and the recipe walk into `src/` (task 4), plans this exact scatter case as a `bug_` test (task 9),
and defers "schemas on the wire" and the `fb_text::schema_text` drift to a later task (task 10).
Land this before task 4, or after it in the moved files; never during. Closing #95 removes one of
the four known refusals `walk-drives-every-plan.md` relies on — tell that task. #189 (UInt8 gid,
refused by comet on the CPU) is the same switch's neighbour and is not bundled here.

Drop the i64-overflow risk (F8).

## 5. Complexity

**M**, as proposed. The additions here are one device test (~40 lines), one `CUDF_EXPECTS`, and
prose; the shad-gpu round trips (three test binaries plus the payload regen) are what make it M
rather than S, not the line count.
