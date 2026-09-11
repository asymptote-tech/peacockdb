# #163 — proposal: an aggregate's state is typed by the aggregator that fills it

Read at master c18e063a. Paths are relative to `/media/data/peacockdb`. DataFusion 45.0.0 and
arrow-arith 54.2.1 sources cited are the ones in `Cargo.lock`, read from the cargo registry.

## 1. Issue

The planner declares an `avg`'s two state columns with the types of DataFusion's own `Avg`
accumulator — `[count: UInt64, sum: <input type>]` (`datafusion-functions-aggregate-45.0.0/src/average.rs:154-167`)
— but this engine never runs that accumulator. It decomposes `avg` into a `sum` and a `count`
(`peacockdb-core/src/plan/aggregates.rs:64-67`) and runs each as its own SQL aggregate on both
engines: DataFusion's `count` answers `Int64` (`count.rs:163-167`), cuDF's count is cast to
`INT64` (`cpp/src/operators/aggregate.cpp:819-823`). The CPU backend then refuses at executor
construction, before a row moves — `check_state_layout`
(`peacockdb-core/src/executor/cpu_backend/mod.rs:467-491`, error at `:482-488`):

    column N is UInt64 in the declared state and Int64 in the one DataFusion's accumulators produce

It fires from `CpuExec::aggregate` (`cpu_backend/mod.rs:130-140` → `aggregate_exec` `:323-384`,
the check at `:381-382`) and equally from the merge (`cpu_backend/accumulate.rs:73`), reached
through `executors_for` (`cpu_backend/backend.rs:81-83`). N is wherever the `$count` column sits,
which is why the T19 comments record columns 1 to 10 and why no positional special case works.

What it disables (00-tickets.md row, confirmed against `testdata/cost-registry.csv` lines 2, 7, 8,
10, 14, 15, 18, 19, 23, 25, 27, 31, 33, 36, 40, 66, 82, 86, 93, 101, 117, 122, 138 and
`peacockdb-core/tests/common/corpus_cases.inc` 19-23, 70-72, 83, 101, 174-175, 270-273, 277-280,
282-291, 293-302): 23 queries × 5 modes, cpu and so gpu — tpch q1 q17 q22 shuffle_additive_avg;
tpcds q1 q6 q7 q9 q13 q14 q17 q18 q22 q24 q26 q30 q32 q35 q39 q65 q81 q85 q92. All 23 plan at all
five modes today (`plan_status=ok`, sections in every `<mode>.plans.txt`); the refusal is at run.

A second wall stands directly behind the first and is not written down anywhere. Once the state
types agree, every decimal `avg` (19 of the 23 — only tpcds q17 q22 q35 q39 average nothing but
integers, which `avg` coerces to Float64) fails again on the CPU at the finalize project:
`finalize()` casts the numerator to the output type `Decimal128(p, s+4)` before dividing
(`plan/aggregates.rs:89-93`), the CPU lowers `Expr::Binary` to a bare DataFusion `BinaryExpr`
and drops the IR's `out_type` (`cpu_backend/expr_physical.rs:57-61`), DataFusion types a decimal
divide by arrow's kernel (`datafusion-expr-common-45.0.0/src/type_coercion/binary.rs:137-160`),
and arrow's decimal `Div` answers at scale `s_numerator + 4` (`arrow-arith-54.2.1/src/numeric.rs:791-819`)
— so `Decimal128(19,6) / Decimal128(19,0)` comes back as `Decimal128(23,10)`, and `declared_as`
(`cpu_backend/mod.rs:239-275`) refuses it: `widened_decimal` (`:499-506`) accepts a wider
precision at the *same* scale only, and `RecordBatch::try_new` against the declared `(19,6)`
errors. This is by reading, not by a run — nobody has reached it because #163 refuses first —
but every step is a line cited above. The proposal fixes both, since the ticket's question is
whether the cells come back.

## 2. Root cause

`decompose` (`peacockdb-core/src/planner/translator/aggregate.rs:105-209`) declares state types
at `:127-157`: it calls `aggregate.state_fields()` on DataFusion's `AggregateFunctionExpr` for the
SQL aggregate as written (`:127-129`), pairs its fields to our decomposition by the aggregator's
tag (`declared_state`, `:74-92`) and copies `data_type()` and `is_nullable()` into our state field
(`:150-157`). The doc at `plan/aggregates.rs:6-8` states the design: "State *types* come from
DataFusion's `state_fields()`, so the split cannot drift from the one DataFusion planned."

That holds only where the accumulator DataFusion planned is the one that runs. For `sum`, `min`,
`max`, `count` it is (their single state field is their return type — `sum.rs:203-207`,
`count.rs:163-167`, `min_max.rs:228-230`). For the Welford triple it is (the CPU runs DataFusion's
`stddev`/`var` accumulator as one aggregate, `state_funcs` `plan/aggregate.rs:20-57`, and its state
is exactly `stddev.rs:112-126`). For `avg` it is not: the CPU's `init_aggregates`
(`cpu_backend/mod.rs:388-406`) resolves `agg_name(PlanAgg::Count)` = `"count"` and
`agg_name(PlanAgg::Sum)` = `"sum"` (`plan/aggregate.rs:115-137`) in DataFusion's registry and
builds those; the wire sends the same two names (`wire/aggregate_writer.rs:134-147`, `:159-183`),
which `aggregate.cpp` runs through its generic arm (`:702-736`) with `count_cast` (`:819-823`).
So the produced state is `[sum: Decimal128(p+10, s) | Float64, count: Int64]` on both engines
while the declaration borrows `Avg`'s `[count: UInt64, sum: Decimal128(p, s)]`.

Two consequences, one seen and one hidden. The count mismatch is the refusal. The sum mismatch
(`Decimal128(15,2)` declared, `(25,2)` produced for `avg(l_quantity)`) is accepted by the
`widened_decimal` arm and cast back *down* to the declared precision with `safe: false` at every
init (`cpu_backend/mod.rs:243-267`) — T17's "widening arm", whose comment argues a merge and
which is in fact also covering a declaration borrowed from an accumulator that never runs.

The finalize is the same defect from the other side: `finalize()` is planner-invented IR, so its
`out_type` is DataFusion's `avg` type `(p+4, s+4)`, not the type DataFusion computes for the
expression as written. On the device that is harmless — `build_column_binary`
(`cpp/src/expr.cpp:585-600`) reads `out_decimal_scale` off the wire and re-scales the numerator to
`e_out + e_denominator` itself, so cuDF's `s_l − s_r` lands on `s+4` whatever the numerator was cast
to. On the CPU nothing reads `out_type`, and arrow's own rule (`s_num + 4`) applied to a numerator
already at `s+4` gives `s+8`.

Why both engines nevertheless agree with DataFusion's `avg` once the numerator is left at the
sum's own scale: DataFusion's `DecimalAverager::avg` computes `(sum × 10^(4)) div count`,
truncating toward zero (`datafusion-functions-aggregate-common-45.0.0/src/utils.rs:194-210`);
arrow's `Div` computes `(l × 10^mul_pow) div_checked r` with `mul_pow = (s_l + 4) − s_l + s_r = 4`
when `s_r = 0`, truncating (`numeric.rs:794-819`); cuDF's fixed-point `DIV` at output scale
`s_l − s_r` after the C++ pre-scale truncates (`test_gpu_recipe_walk.rs:700-713` proves the device
against the oracle's digits today). Three truncations of the same integer quotient at the same
scale are the same digits.

## 3. Localized fix

Two changes, both in the plan/planner layer, no C++, no ABI, no `.fbs`, no wire-format change.
Recipe *bytes* change (a type enum and a decimal precision inside the merge nodes'
`aggr_input_schema`, and one fewer `CastExpr` in each avg finalize), which is content, not format.

### 3a. State types come from the aggregator that fills the column

**`peacockdb-core/src/plan/aggregate.rs`** — beside `agg_name` (`:115-137`), add:

```rust
/// The type a state column holds: what the SQL aggregate the executor runs it as
/// answers over its argument — `count`'s Int64, `sum`'s decimal ten digits wider than
/// its input. Asked of DataFusion's own registry by the name `state_funcs` hands the CPU,
/// so the declaration is the produced type by construction rather than by a second table.
pub(crate) fn state_type(func: PlanAgg, arg: &DataType) -> Result<DataType, PlanError> {
    let name = agg_name(func)?;
    let udaf = all_default_aggregate_functions()
        .into_iter()
        .find(|udaf| udaf.name() == name)
        .ok_or_else(|| PlanError::Unsupported(format!("`{name}` is not a DataFusion aggregate")))?;
    udaf.return_type(&[arg.clone()])
        .map_err(|error| PlanError::Invalid(format!("{name} over {arg}: {error}")))
}
```

(`datafusion::functions_aggregate::all_default_aggregate_functions`, `lib.rs:135`; the same
UDAF objects `init_aggregates` finds in a `SessionContext`. A `SessionContext::new()` lookup as
`init_aggregates` does is equally acceptable.) `return_type` is the single state field's type for
all four names this can be asked for — verified at `sum.rs:203-207`, `count.rs:163-167`,
`min_max.rs:228-230`/`1055-1057`; `Mean`/`M2`/`MergeM2` never reach it (below). Delegate from
`plan/mod.rs` beside `state_funcs` (`:1236-1241`), `pub(crate)`.

**`peacockdb-core/src/planner/translator/aggregate.rs`**, `decompose`, replace `:148-157` with:

```rust
let state_at = n_keys + decomposed.state.len();
// A per-column decomposition runs each aggregator as its own SQL aggregate on both
// engines, so the column's type is that aggregate's answer and `avg`'s own accumulator,
// which never runs, has no say. A Welford triple is one accumulator on the CPU, and its
// state is what its SQL aggregate declares. Nullability stays the SQL aggregate's in both
// cases: the merge reads this schema too, and a merge over an empty lane answers NULL.
let arg_type = match aggregate.expressions().first() {
    Some(expr) => expr.data_type(input_schema).map_err(|e| PlanError::Invalid(format!("{}: {e}", aggregate.name())))?,
    None => DataType::Null,
};
let mut state = Vec::with_capacity(rule.state.len());
for (suffix, func) in rule.state {
    let field = declared_state(&declared, *func, aggregate.name())?;
    let data_type = match rule.merge {
        Merge::Combined(_) => field.data_type().clone(),
        Merge::PerColumn(_) => state_type(*func, &arg_type)?,
    };
    state.push(Field::new(format!("{}{suffix}", aggregate.name()), data_type, field.is_nullable()));
}
```

`Merge::Combined` is already the Welford discriminator the merge validator keys on
(`plan/aggregate.rs:290-347`); no new flag. `declared_state` keeps its job for the name suffix
pairing and the nullability. The `declared.len() != rule.state.len()` drift guard (`:130-137`)
stays.

**Nullability is deliberately not derived.** `Count::state_fields` says `nullable = false`;
`Avg`'s says `true`. The state schema is read by the merge as well as the init, and a keyless
merge over a lane that received nothing runs over no input and answers a NULL sum
(`cpu_backend/accumulate.rs:374-389`, `:354-364` through `declared_as`), which
`RecordBatch::try_new` refuses under a non-nullable field — that is #180's shape
(`llm-wiki/tasks/active-tickets.md:263-276`), and declaring `avg`'s count non-nullable would
import it into every keyless `avg`. So the type moves and the nullability stays the SQL
aggregate's, exactly as today. Plain `count(*)`'s state (`Int64`, non-nullable, from `Count`)
is unchanged by this fix; #180 owns it.

Effect on declarations: every `avg(x)$count` `UInt64` → `Int64`; every decimal `avg(x)$sum`
`Decimal128(p, s)` → `Decimal128(min(p+10, 38), s)`; float avgs' `$sum` stays `Float64`; `sum`,
`min`, `max`, `count`, stddev/var state unchanged (derived type equals today's). The init-stage
narrowing cast in `declared_as` no longer fires for `avg`; the merge-stage widening
(`(25,2)` summed → `(35,2)`, cast back) still does, which is what `widened_decimal` was written
for. Widths in `common.rs:30` are 8 bytes for both `Int64` and `UInt64` and 16 for any
`Decimal128`, so no `--- memory ---` figure and no execution-golden byte moves.

### 3b. The avg finalize divides the sum as it is

**`peacockdb-core/src/plan/aggregates.rs`**, `finalize`, `AggFunc::Avg` arm (`:80-101`): for a
`Decimal128` output, drop the numerator cast — `column(0)` bare; keep the denominator cast to
`Decimal128(p, 0)`; keep the float arm as it is (`Cast(sum → Float64)` is an identity there and
harmless). Comment to carry, in place of `:81-82`:

```rust
// The denominator is an exact integer-valued decimal and the numerator is the sum at its
// own scale. Each engine then lands on DataFusion's avg scale s+4 by its own divide rule —
// arrow's s_num + 4, cuDF's s_l − s_r after `expr.cpp` pre-scales the numerator by the
// declared out scale — and all three truncate, so the digits are DataFusion's avg's.
```

Result: CPU `Decimal128(25,2) / Decimal128(19,0)` → arrow `Decimal128(29, 6)` → `declared_as`
widening arm casts to the declared `(19,6)` (same scale, wider precision — the case it accepts);
device unchanged (`expr.cpp:589-600` recasts the numerator to scale 6 regardless, so the removed
cast was one redundant cuDF kernel). Broaden the comment above `declared_as` (`cpu_backend/mod.rs:243-248`):
DataFusion widens a decimal's precision at a merge's sum *and* at a divide.

### What it does not touch

- `cpp/` — nothing. The device's count is already `INT64`; its divide already pre-scales.
- The Welford count: the CPU produces `UInt64` (DataFusion's accumulator), the device `INT64`
  (`aggregate.cpp:615-632`, `:764-771`), the declaration `UInt64`. No cell is disabled on it
  (`shuffle_stddev`'s device cells are off on #183, registry line 139) and it never crosses an
  export, since a finalize always stands between state and sink. It stays with the narrowed
  ticket; the two-line device-side fix (cast those two counts to `UINT64`) is the follow-up.
- `check_state_layout`, `widened_decimal`, `declared_as` — kept. The first is the guard that
  went red and stays the guard; the other two serve the merge.
- `AggregateMode`, `mergeable_agg_state`, `aggr_input_schema` — unchanged fields; the C++ reads
  neither the schema nor a type from it (no `aggr_input_schema` consumer in `cpp/src`).
- The general "derive every expression's type and compare" validator the ticket's Fix line asks
  for. Not needed for the cells; stays as the ticket's remainder.

### CPU and GPU agreement

The declaration now names what both engines already produce: `count` → `Int64` on both
(`count.rs:146-148`; `aggregate.cpp:821-823`, and the merge's `sum` over it `:582-597`,
`INT64` in cuDF); `sum` → the same type family on both (the device carries no precision; its
export widening is #187, unchanged). The finalize is one expression both evaluate, and section 2
shows the three truncations agree. Proof on the device is the existing
`an_average_finalizes_to_the_digits_the_oracle_computes` (`test_gpu_recipe_walk.rs:700-713`),
which drives `AVG_BY_FLAG` at two lanes against DataFusion and must stay green with the new
payload; on the CPU the new test below.

### Tests, goldens, registry, comments that move

Tests (red first, then green):
- New, `cpu_backend/tests/exec.rs` (or `plan/tests/aggregate.rs`): a decimal `avg` through
  `CpuExec::aggregate` — init and finalize on one node over a `Decimal128(15,2)` column with state
  declared as the planner now declares it — compared digit for digit against DataFusion's own
  `avg` over the same rows. Red today twice over (the refusal, then the scale). This is the test
  that pins the arrow `+4` / avg `+4` coincidence, which is the one assumption 3b rests on.
- New, `planner/translator/schema_tests.rs`: `avg` over `p_retailprice` declares
  `$sum: Decimal128(25,2)`, `$count: Int64`, both nullable.
- Change `avgs_state_columns_are_typed_by_what_they_hold_and_not_by_position`
  (`schema_tests.rs:146-181`): `(15,2)` → `(25,2)`, `UInt64` → `Int64`; its doc keeps the by-tag
  point for nullability.
- Change `the_divide_that_finishes_an_avg_hits_the_scale_datafusion_declared`
  (`schema_tests.rs:183-221`): the left side is the bare state column, only the right side is a
  cast to `Decimal128(19, 0)`; rewrite the comment at `:206-208` per 3b.
- Hand-built fixtures already declare `avg(v)$count: Int64` on both engines
  (`cpu_backend/tests/exec.rs:265-303`, `tests/test_gpu_executors/exec.rs:149-180`,
  `cpu_backend/tests/accumulate.rs:188-230`) — unchanged, and evidence the declaration was the
  odd one out.
- Welford fixtures (`plan/tests/aggregate.rs:155-161`, `cpu_backend/tests/exec.rs:339`,
  `accumulate.rs:382`, `test_gpu_executors/accumulate.rs:332-341`) — unchanged; narrow the doc at
  `test_gpu_executors/accumulate.rs:336-341` from "wherever a plan declares it" to the Welford
  triple.

Goldens:
- `<mode>.plans.txt`, both benches (`UPDATE_CANONICAL=1`, `test_plan_goldens`): every node line
  carrying an avg state schema or an avg finalize — the lines holding `UInt64` today: tpch 5/5/23/21/21,
  tpcds 32/32/120/116/116 (tp1-single/tp1-rowgroup/tp4-single/tp4-rowgroup/tp4-sized). Shape
  unchanged; types and one cast move.
- `recipe-payloads.txt` (`PEACOCK_REWRITE_RECIPE_BYTES=1`, fixed `/tmp` symlink): the sections
  with an avg — tpch q22, tpcds q14, tpcds q39 — new digests, rendered text differs only in the
  finalize line. The regen is legitimate: content moved, not statement order.
- `<mode>-mini.cpu.txt` ×5, `<mode>-mini.cost.txt` ×5, `mini.result.txt`: 23 new sections each,
  authored by the corpus cpu tier under `UPDATE_CANONICAL=1` as T19 did.
- `testdata/cost-registry.csv`: the 23 rows' `cpu_*` cells to `enabled` where the run passes,
  `163` dropped from `tickets`; `65`/`45`/`57`/`63`/`59 80` stay on their rows (device or latent).
- `corpus_cases.inc`: 23 lines' `cpu_modes`; the comments at 19-23, 70-72, 91, 144-145, 158,
  168-169, 178, 270-273, 277-280, 282-291, 293-302 rewritten to what the run found.

Comments and pages:
- `plan/aggregates.rs:6-8`, `plan/mod.rs:406-407` (`PlanAgg::tag` "how our state columns find
  their types" → nullability, and types for a Welford triple), `planner/translator/aggregate.rs:71-73`
  (`declared_state` doc), `plan/aggregate.rs:274-275` ("Nor are the state column TYPES, which no
  rule anywhere derives" → derived at translation by `state_type`, not re-checked here).
- `llm-wiki/architecture.md:48-50` ("read off `AggregateExpr::state_fields()`" → off the state
  fields of the aggregate the executor runs each column as: `count` and `sum` for `avg`, the
  Welford accumulator for stddev/var); `:356-360` (the seven coercions: "avg's decimal input and its
  finalize divide" → the denominator of avg's finalize divide); `:1149-1154` stays true.
- `llm-wiki/build-test.md:42` ("thirteen queries are out entirely on #163" — 23 today, none
  after); `:21` stays true (the Welford device count).
- `llm-wiki/tickets.md` #163: narrowed to the Welford count on the device and the absent
  validator; the "signed arm" paragraph replaced by what closed it.

Hacks-audit scaffolding: none named for #163. The relevant neighbours — `widened_decimal`
(audit "What I found nothing for", #187) and finding 12 (`schema_digest` names only, #183) — are
left standing on purpose; this fix neither removes nor fights them. No `bug_` test exists for
#163 to delete.

## 4. Alternatives rejected

- Accept `UInt64`↔`Int64` in `check_state_layout`/`declared_as` with a cast — the escape the
  ticket forbids; masks the device's Welford divergence; a cast per batch for a wrong declaration.
- Special-case `PlanAgg::Count → Int64` in `decompose` — fixes the refusal, keeps `$sum` borrowed
  from an accumulator that never runs and the unsafe init-stage narrowing; same golden churn.
- A custom `avg` UDAF whose `state_fields` say `Int64` — a second avg to keep in step.
- Cast the device's count to `UINT64` for `avg` — the device would match a declaration the CPU
  still cannot produce.
- For 3b, make `expr_physical` wrap `Binary` in a `CastExpr(out_type)` — arrow's scale-reducing
  cast rounds half away from zero where DataFusion's avg and cuDF truncate: `data_fusion_exact`
  fails on roughly half the groups and the two engines disagree.
- For 3b, replicate `expr.cpp`'s pre-scale in `expr_physical` — a Rust model of the C++ and of
  arrow's `+4` in one function (hacks-audit finding 5's shape).
- The ticket's general validator (an `Expr::data_type(&Schema)` over the IR plus a check per
  computing node) — right guard, not needed for a single cell, and a task of its own.

## 5. Minimum corpus query

    SELECT avg(s_acctbal) FROM supplier

tpch sf1, `s_acctbal` is `Decimal128(15,2)`, 10,000 rows, no join. Plans at all five modes today
(a `GpuAggregate` init under a `GpuAggregateBatches` that merges and finalizes; at tp4 a per-lane
merge, a merge of lanes, then the finalizer — keyless, so no shuffle). Run it at **tp1-single**
first (one lane, the shortest sequence) and at **tp4-single** (the merge path). CPU backend.

Today, CPU: refused at executor construction — `executors_for` → `CpuExec::aggregate` →
`check_state_layout`: "column 1 is UInt64 in the declared state and Int64 in the one DataFusion's
accumulators produce". After 3a alone: refused at the finalize project — `declared_as`: "the node
declares … avg(supplier.s_acctbal): Decimal128(19, 6) … and DataFusion answered with …
Decimal128(23, 10)" (predicted from the cited lines; the new CPU test is where it is observed).
After 3a and 3b: one row, equal to DataFusion's `avg` to the digit. Device: the plan crosses and
runs; the export refuses the decimal at the unload (#187), as for every decimal-valued query, so
the gpu cell stays off — this query is a cpu claim.

For 3a alone, smaller still: `SELECT avg(r_regionkey) FROM region` (5 rows, integer → Float64)
refuses today on the count column and passes after 3a with no decimal involved.

## 6. Cells re-enabled

Come back (cpu, expected; each proven by its run as T19 did): tpch q1, q17, q22,
shuffle_additive_avg at all five modes; tpcds q1 q6 q7 q9 q13 q17 q24 q26 q30 q32 q35 q39 q65
q81 q85 q92 at all five; tpcds q14, q18, q22 at tp1-single and tp1-rowgroup only — their ROLLUP's
`UInt8` grouping id cannot be hashed at the three tp4 modes (#189, the same wall as
`rollup_over_join`, q5, q80). Up to 106 of the 115 cpu cells.

Stay off: those 9 tp4 rollup cells (#189); every gpu cell of the 23 — the settled pattern from
the corpus comments is #152 (a join with more than one probe batch) at the four multi-batch
modes and #183/#187 (a string or decimal in the export) at tp1-single, and all 23 answer a
decimal or a string. Each still needs a device run to name its ticket.

Possible third walls the run may find and that would keep a query off under its own ticket:
#180's shape for the keyless `count(*)` scalar subqueries in tpcds q9 at the tp4 modes; anything
in these 23 nobody has run on this engine yet.

## 7. Risks and unknowns

- 3b is diagnosed by reading, not observed: the scale-mismatch refusal depends on
  `expr_physical.rs:57-61` dropping `out_type`, DataFusion typing the divide by arrow's kernel,
  and arrow's `s+4` — each cited, none run. If a run shows the CPU already lands on `(19,6)`, 3b
  is dropped and only the test stays.
- 3b rests on arrow's `Div` scale (`s_num + 4`) equalling DataFusion's `avg` scale (`s + 4`) —
  the same constant written twice in two crates, both capped at 38. A version bump can break it;
  the new digit-for-digit CPU test is what would say so.
- The finalize's CPU result `(29,6)` narrows to `(19,6)` through `declared_as`'s `safe: false`
  cast: an average whose magnitude exceeds 10^13 errors rather than rounds. Not reachable at sf1.
- `data_fusion_exact` on the 19 decimal-avg queries assumes the three truncations agree for every
  group; the device leg is proven today, the CPU leg only by the argument in section 2 until the
  new test runs.
- `all_default_aggregate_functions()` vs a `SessionContext` registry lookup: I read that `sum`,
  `count`, `min`, `max` are primary names there, not aliases; a name resolved through an alias
  would make `find` miss. The developer's first compile-and-run of the schema test settles it.
- The corpus authoring pass for 23 queries × 5 modes is the long pole (verda or a local run), and
  it is where unknown third walls show. Per T19's rule each such query is disabled with a ticket
  and the fix still stands.
- Welford on the device stays declared `UInt64` and exported `Int64`; invisible today, and a
  later change that exports state (a `SELECT` over a partial) would meet it. Named in the
  narrowed ticket, not fixed here.

## 8. Complexity

**M.** Logic: three files (`planner/translator/aggregate.rs` ~20 lines, `plan/aggregate.rs` +
`plan/mod.rs` ~20 lines, `plan/aggregates.rs` ~5 lines) plus comment edits in three more; two
tests changed, two added. No frozen surface changes: no C ABI symbol, no `.fbs`, no wire format,
no declared-schema contract — only the *content* of recipe bytes (a type in an unread schema
field and one cast fewer), which needs the deliberate `PEACOCK_REWRITE_RECIPE_BYTES=1` regen for
three payload sections. Plan goldens regenerate for every avg query (about 490 lines across ten
files, shape-preserving). The size is in the corpus enablement — 23 queries × 5 modes authored,
compared and triaged — not in the code.
