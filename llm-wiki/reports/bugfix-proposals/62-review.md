# #62 — review of the proposal

Read at master 188c23ce, read-only. Paths relative to `/media/data/peacockdb`; DataFusion 45.0.0
sources from the cargo registry. Nothing built or run.

## 1. Verdict

**Needs changes.** The root cause, the lowering and its localization are right, and the shuffle
reuse, the NULL handling of `d` and the Σ-merge arithmetic all check out against the code. Two
things a developer hits in the first hour are wrong in the line-level plan: the `avg` companion —
q28's own shape — cannot get through `decompose` as written, because `declared_state`'s tag
lookup does not match our `$sum`/`$count` names; and the outer stage's companion state copies the
inner's nullability while its producer is a `sum`, which is #180's contradiction re-created, and
which turns a keyless `count(x), count(DISTINCT x)` over a filter that keeps nothing into a
refused call (or a NULL where SQL owes 0). Neither reaches a corpus cell; both are cheap to fix.
A third item is for the consolidator: #163's proposal rewrites the same fifteen lines of
`decompose`, and its `arg_type` derivation is wrong under the new `InitFrom::State` arm.

## 2. Findings

### F1 — the `avg` companion fails `declared_state` under `InitFrom::State` — important

The proposal (3.2a) says `declared` for `State(cols)` becomes "the `input_schema` fields at
`cols.positions`" and that lines `:148-157` are "unchanged". Those lines call

    peacockdb-core/src/planner/translator/aggregate.rs:150
        let field = declared_state(&declared, *func, aggregate.name())?;

and `declared_state` (`:71-89`) returns `declared[0]` only when there is one state column;
otherwise it searches for a field whose name ends with `[sum]` / `[count]` — DataFusion's
`format_state_name` tags. The inner's state fields are named by us, `avg(…)$sum` and
`avg(…)$count` (`:151-156`, suffix from `plan/aggregates.rs:63`), so for an `avg` companion the
lookup returns `Invalid("avg(…): DataFusion declares no [sum] state column, so this mode's
decomposition of it has drifted")`. `count`/`sum`/`min`/`max` companions pass (one column). q28's
three aggregates include `avg(ss_list_price)`, so the proposal as written refuses q28 at a
different line.

Correction: under `State(cols)` pair positionally — `declared[i]` for the i-th `(suffix, func)` of
`rule.state`. That is sound because the inner's `decompose` pushed its state in `rule.state` order
(`:148-157` is what proves it) and `cols.positions` are those columns in that order. Keep the
`declared.len() != rule.state.len()` guard (`:130-137`) against `cols.positions.len()`.

### F2 — nullability copied from the inner, and a keyless empty input answers NULL/refuses — important

Two halves of one thing.

(a) The declaration. 3.2(d) types and null-flags the outer's companion state from the inner's
fields. For a `count(x)` companion the inner's field is `Int64, nullable=false`
(`count.rs:163-167`), while the outer's producer of that column is `sum` (`plan/aggregates.rs:58-63`
— "a count merges by SUM"), whose DataFusion state is nullable (`sum.rs:203-207`) and which
answers NULL over no rows. Both the #163 and #180 proposals state the rule the same way: a state
column's nullability is that of the SQL aggregate the executor runs it as. `check_state_layout`
(`cpu_backend/mod.rs:467-491`) compares types only, so nothing catches it at construction.

(b) Where it bites. Keyless `SELECT count(x), count(DISTINCT x) FROM t WHERE <keeps no rows>`
(a predicate DataFusion cannot fold, e.g. `l_quantity < 0`). At tp1: the filter answers an
empty batch (`CpuExec::exec`, `:173-176`); the inner init, grouped on `x`, emits an empty batch;
the inner's final merge compacts it (`cpu_backend/accumulate.rs:351-364`) into `Some(empty)` and
`one_batch(&self.held, &[state])` emits an empty batch (`:138-143`, `held` is non-empty); the
outer shortcut `GpuAggregate` runs DataFusion's no-grouping Partial over it, which emits exactly
one row on exhaustion (`datafusion-physical-plan-45.0.0/src/aggregates/no_grouping.rs:136-148`)
with `sum(count state) = NULL` and `count(x@d) = 0`. `declared_as` (`cpu_backend/mod.rs:239-275`)
then hands arrow a NULL in a column declared non-nullable and `RecordBatch::try_new` refuses —
the exact message #180 is about. Fix (a) alone and the call passes with `count(x) = NULL` where
SQL says 0; the finalize is a rename (`plan/aggregates.rs:83`), and the output column is declared
non-nullable by DataFusion's final schema, so the finalize stage refuses instead. The device does
the same arithmetic: keyless Partial `sum` is `cudf::reduce` (`aggregate.cpp:315-320`), NULL over
nothing, so CPU/GPU agree on the wrong answer. Today's single-stage `count(x)` over the same
input answers 0, because its init counts the empty batch (0) and the merge sums a 0. #180's fix
does not reach this: it supplies the identity where a finalizing keyless `GpuAggregateBatches`
received *no batch*; here a `GpuAggregate` receives an *empty* batch and a sum-form aggregator
runs over zero rows — a new way to reach #180's NULL, at tp1, from SQL.

Not a corpus cell (q28's six filters all keep rows), so important rather than blocking. But it is
a wrong answer the proposal ships unnamed, and §7 does not list it.

Correction, in order of preference:
1. Declare the outer's state nullability per the aggregator that produces it — `Sum`/`Min`/`Max`
   init-form calls are nullable — not per the inner's field. One line in the `State` arm.
2. For a `Count` companion (and `count(*)`), make the outer's finalize
   `CASE WHEN o IS NULL THEN 0 ELSE o END` rather than the rename. It is the one expression both
   engines evaluate, `UnaryOp::IsNull` exists in the IR (`plan/mod.rs:104`) and on both device
   paths (`expr.cpp:267`, `:877`), and the searched-CASE form is what the Welford finalize already
   proves (`plan/aggregates.rs:124-138`). Confined to the new lowering, so no existing golden or
   payload moves. If the consolidator prefers not to, file it as a residual beside #199 with a
   `bug_` test; either way §7 has to say it.
3. Add the empty-filter keyless query to the end-to-end cases (3.6), against DataFusion, red
   before the fix.

### F3 — overlap with #163's proposal on the same lines of `decompose` — important (consolidation)

163-proposal.md §3a replaces `aggregate.rs:148-157` to derive each state column's type by
`state_type(func, &arg_type)` where `arg_type` is
`aggregate.expressions().first().data_type(input_schema)`. Under this proposal's
`InitFrom::State` arm, `aggregate` is the original companion whose expressions index the *raw*
input, and `input_schema` is the inner intermediate — the ordinal is wrong or out of range. The
two must be reconciled explicitly: under `State`, the argument type of the i-th state column is
the inner's declared type at `cols.positions[i]`, and the state type is
`state_type(merge_func_i, that)`. Sequence #163 first (it also sets the `avg$count`/`$sum` types
this lowering inherits, which the proposal rightly leans on); then #62's `State` arm is
positional on both name and type. #180's proposal does not touch the translator, but its identity
rule is the one F2 falls outside of.

### F4 — four-lane translator tests over `nation` need the small-table knob — minor

3.6 (ii)/(iii) want four lanes; `translated()` plans at tp1 (`translator/tests.rs:39-43`), and at
tp4 `nation` in `tpch.minimal` is below `SMALL_TABLE_BYTES`, so the source drops to one lane
unless the test uses `translated_at_tp4(sql, 0)` (`:46-60`). With one row group the four-lane
mapping leaves three lanes empty, which is fine for asserting shape but worth knowing before
reading `partition_groups`.

### F5 — the exec-model prototype already models this exact lowering — minor

`scripts/exec_model/tests/test_end_to_end.py:383`,
`test_a_distinct_beside_non_distinct_aggregates_lowers_the_same_way`: inner grouped on `v` with
`sum`/`count` companions, made global on one lane, outer `count(v)` + Σ over the companions,
checked against pandas. It is the strongest existing evidence for §3.1 and the proposal does not
cite it; it should, and §6's "exec-model corpus: q28 stays absent" should say the lowering itself
is already in the prototype.

### F6 — a coercion cast makes the common non-Column `d`, not `a + 1` — minor

§7 names `count(DISTINCT a + 1)` as the expression-key case the device refuses at run time
(`aggregate.cpp:163`). The ordinary way to get there is a coercion: `avg(DISTINCT int_col)` and
`sum(DISTINCT int32_col)` arrive with the argument already wrapped in DataFusion's `CAST`, so
`d` is an expression and the inner's group key is one. Not a corpus shape and not new, but the
risk should say so, since a reader will otherwise expect only user-written expressions there.

### F7 — the `build-test.md:42` counts move — minor

The line says "37 queries at the modes each is correct at" and "thirteen queries are out entirely
on #163". A `corpus_query!(tpcds, 1, q28, none, none, …)` on #163 makes those 38 and fourteen; the
proposal says only that the line "gains the `none` line".

## 3. Claims verified

- The refusal is `aggregate.rs:118-124`, plan-time, before any backend; the C++ guard
  (`aggregate.cpp:143-155`) and `aggregate_writer.rs:177` (`distinct: false`) make the wire flag
  unreachable, as hacks-audit finding 10 says.
- DataFusion's `SingleDistinctToGroupBy`: sum/min/max-only companions at
  `single_distinct_to_groupby.rs:86-91`, one-argument rule `:188`, grouping sets declined `:130`,
  the same `func` re-applied `:192-218`. `count`'s distinct state is a `List` (`count.rs:154-161`),
  its plain state `Int64, false` (`:163-167`); avg's is `[count: UInt64, sum: input type]`
  (`average.rs:154-167`).
- `(#62)` appears in exactly five golden lines, all `== q28`; registry row 29 is
  `disabled×5, na×10, tickets 62`; rows 17, 95, 96, 116 (tpcds q16, q94, q95, tpch q16) carry a
  stale `62`; no `corpus_query!` line for q28; `test_planner_join_refusals.rs:104-117` pins the
  message for q28's shape.
- `decompose` has one caller (`:278`); `Expr: PartialEq` (`plan/mod.rs:110`); the
  `PlanError::Unsupported` doc names "a mixed distinct (#62)" (`:52-53`).
- The subset rule at `plan/aggregate.rs:236-258` runs only on a *finalizing* merge over >1 lane;
  `check_merges_the_state_it_was_given` (`:276-347`) runs only on `GpuAggregateBatches` against
  its input's annotations, so the outer's init (`GpuAggregate`, `body.validate` only,
  `:208-216`) is not checked against the inner's annotations and the outer's final merge is
  checked against the outer's own — both as the proposal needs. `regrouped_key_distribution`
  (`:360-383`) carries a hash on `G` through the inner's merge (group list `G ++ [d]`) and the
  outer's init and merge (group list `G`), so the outer's finalizing merge at n lanes validates.
- Shuffle reuse: DataFusion's hash exprs are the group columns, ordinals `< |G|`
  (`hash_key_ordinals`, `common.rs:46-55`), and `G ++ [d] ++ state` leads with `G`, so the emit
  hashes the right columns; a lane holding all rows of a `G` holds all `(G, d)` groups.
  `GpuEmitPartitions` declares `MultipleBatches` (`partition_ops.rs:121`), the merge above it
  emits `SingleBatch` (`plan/aggregate.rs:479`), so the outer at n lanes is init + a
  one-batch-per-lane finalizing merge — correct, validated, and one merge more than it needs.
- The keyless tp4 shape is `Collapse` (goldens: tpch q6 at tp4-single, `:2178-2186`), so the outer
  is the one-lane shortcut; the tp1 scan declares `MultipleBatches`, so the inner still merges —
  3.3's tree is what the code produces.
- NULL handling of `d`: the device groups under `null_policy::INCLUDE` (`aggregate.cpp:441`);
  grouped `count` is `make_count_aggregation` (default `EXCLUDE`, `:80`) then INT32→INT64
  (`:819-823`); keyless `count` is `size − null_count` (`:264-268`); DataFusion's `count` skips
  NULL. So the NULL-`d` row is one inner group and is not counted; companions counted per
  `(G, d)` include that group's rows, and Σ recovers the total.
- Merging state through init-form `sum`/`min`/`max`: CPU `init_aggregates` resolves
  `agg_name(Sum) = "sum"` and builds a Partial `AggregateExec` (`cpu_backend/mod.rs:323-406`);
  device Partial reads `func->args()` (`:184-200`, `:447-457`) and `make_agg("sum") = SUM`
  (`:86-87`). `Merge`-mode nodes on the device read state *positionally* after the keys
  (`:709-713`, `in_off`), ignoring `args` — both stages' merges keep `[keys…, state…]` in order,
  so that holds; the width recovery (`:503-532`) sees `n_avg = n_std = 0` because merge funcs
  are all `sum`.
- Welford companions cannot be an init: the CPU folds a triple into one `stddev` UDAF over one
  argument (`state_funcs`, `plan/aggregate.rs:20-57`; `init_aggregates`), and the device's Partial
  arm computes Welford over one cast column (`aggregate.cpp:614-631`). Refusing is right.
- Payload cover: the shapes q28 adds — `GpuAggregate: Partial + Project{finalize}`,
  `GpuAggregateBatches: Merge` with and without the project — are in the tp4-rowgroup goldens
  already (the shortcut at tpcds q54 / tpch q15; per-lane merges everywhere), and `call_shapes`
  (`test_plan_goldens.rs:450-490`) strips lanes and seqs, so `recipe-payloads.txt` does not move.
- No plan golden has a `GpuAggregateBatches: group_by=…` line without `final=` at `lanes=1`, nor a
  `GpuAggregate` without `final=` at `lanes=1, batches=single` (0 and 0 over all ten files), so the
  shortcut generalization moves nothing committed.
- `AggregateExprBuilder::new(fun, args).schema(..).alias(..).build()` is the API
  (`datafusion-physical-expr-45.0.0/src/aggregate.rs:81, 94-150, 155, 160`), the same call the CPU
  backend makes; `build()` needs args, schema and alias, all given; `is_distinct` defaults false,
  so the twin's `state_fields()` are the plain ones.
- Registry conventions match q1's row (`cost-registry.csv:101`: plan `enabled`, cpu/gpu
  `disabled`, tickets naming the blocker) and the plan-cell rule at `test_plan_goldens.rs:686-692`.
- q28 (`testdata/tpcds-queries/q28.sql`) is six keyless `avg, count, count(DISTINCT)` subqueries
  over `store_sales`, cross-joined, `LIMIT 100`; the cross join keeps the gpu cells off (#152,
  `test_gpu_recipe_walk.rs:789`) and `avg` keeps the cpu cells off (#163) — §6 is right.
- `architecture.md:311-315` are the two sentences the fix falsifies; nothing else on that page.

## 4. Corrected proposal — sections that change

### 3.2 (a), the `State` arm of `decompose`

- `declared`: `Values` → `aggregate.state_fields()`; `State(cols)` → the `input_schema` fields at
  `cols.positions`, **paired positionally with `rule.state`** — no `declared_state` call — with the
  arity guard against `cols.positions.len()`.
- State field per column i: name `aggregate.name() + suffix_i` as today; type: the inner's field
  type at `cols.positions[i]` today, and `state_type(merge_func_i, that type)` once #163 lands
  (F3); **nullability `true`** — the producer is a `Sum`/`Min`/`Max` init-form call, and the
  declaration follows the aggregator that fills the column (#180's rule), not the field it reads.
- Finalize per aggregate: `finalize(spec, …)` as today, except a `State` companion with
  `AggFunc::Count` emits `CASE WHEN o IS NULL THEN 0 ELSE o END` over `o` typed as the output —
  the identity a sum-form merge cannot supply over zero rows (F2). Its comment names #180.

### 3.6 tests, added

- `test_cpu_end_to_end.rs`, `Coverage::ModesOnly`: the keyless case with a filter that keeps no
  rows — `SELECT count(ss_customer_sk), count(DISTINCT ss_customer_sk) FROM store_sales WHERE
  ss_quantity < 0` — asserting `0, 0` against DataFusion. Red before the CASE finalize (arrow's
  non-nullable refusal on the CPU).
- `planner/translator/tests.rs`: the `avg` companion explicitly —
  `SELECT avg(n_regionkey), count(n_regionkey), count(DISTINCT n_nationkey) FROM nation` — asserting
  the outer's `aggs` are `[Sum, Sum, Sum, Count]` reading the inner's `$sum`, `$count`, `count`
  ordinals and `n_nationkey@k`; this is the case F1 fails. Four-lane cases via
  `translated_at_tp4(sql, 0)`.

### 7. Risks, added

- The keyless empty-input identity (F2) and why the CASE finalize is where it lives.
- Coercion casts as the ordinary source of an expression `d` (F6).
- Ordering against #163: same lines of `decompose`; the `State` arm derives its types from the
  inner's declared state, never from `aggregate.expressions()`.

## 5. Complexity

**M**, agreeing with the proposal. The corrections add perhaps twenty lines (positional pairing,
a nullability constant, one CASE finalize arm) and two tests; none changes the shape of the work.
What keeps it at M rather than S is unchanged: the `sequence` split and the `InitFrom` threading go
through the one function every corpus aggregate uses, and after #163 the same fifteen lines are
edited twice, so the branch order matters more than the line count.
