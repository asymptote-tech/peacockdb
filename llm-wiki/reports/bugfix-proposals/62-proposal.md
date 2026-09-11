# #62 — proposal: a surviving DISTINCT lowers to two aggregate stages in the translator

Read at master c18e063a, read-only. Paths relative to `/media/data/peacockdb`. DataFusion 45.0.0
sources cited are the ones in `Cargo.lock`, read from the cargo registry.

## 1. Issue

The ticket's title is wrong about where the defect sits. Nothing on either backend "ignores"
`AggregateFuncNode.distinct`: the C++ guard at `cpp/src/operators/aggregate.cpp:143-155` throws
on a set flag, and the wire writer never sets one (`peacockdb-core/src/wire/aggregate_writer.rs:177`,
`distinct: false` unconditionally). The production behaviour is a **plan-time refusal** in the
translator, before any backend is chosen:

    peacockdb-core/src/planner/translator/aggregate.rs:118-124
        if aggregate.is_distinct() {
            return Err(PlanError::Unsupported(format!("DISTINCT inside {} (#62)", aggregate.name())));
        }

It fires for every `AggregateFunctionExpr` DataFusion hands us with `is_distinct() == true`. DataFusion
removes the flag itself only through `SingleDistinctToGroupBy`, whose precondition
(`datafusion-optimizer-45.0.0/src/single_distinct_to_groupby.rs:65-97`) is that every
non-distinct companion is `sum`, `min` or `max` (`:86-91`). A `count` or `avg` companion — tpcds q28's
shape, `avg(x), count(x), count(DISTINCT x)` — leaves the flag on and the query reaches `:119`.

What it disables (00-tickets.md row, confirmed): tpcds/q28 at all five modes, cpu and gpu —
`testdata/goldens/tpcds.sf1/*.plans.txt` carry `== q28` / `refused: unsupported: DISTINCT inside
count(DISTINCT store_sales.ss_list_price) (#62)` in all five files (tp1-single at `:2636-2637`); the
registry row is `testdata/cost-registry.csv:29` (plan cells `disabled`×5, cpu/gpu `na`×10, tickets
`62`); no `corpus_query!` line exists in `peacockdb-core/tests/common/corpus_cases.inc`; q28 is
absent from the exec-model corpus. q28 is the only `(#62)` refusal in any golden (grep over
`testdata/goldens/*/*.plans.txt`: 5 hits, all q28). The registry also stamps `62` on four rows that
run fine — tpcds q16 (`:17`), q94 (`:95`), q95 (`:96`), tpch q16 (`:116`) — because DataFusion
rewrote their single-distinct shape; those tags are stale, not cells.

Pinned by `peacockdb-core/tests/test_planner_join_refusals.rs:104-117`
(`a_distinct_beside_a_companion_datafusion_cannot_rewrite_is_refused_naming_62`), which asserts the
message for exactly q28's shape over the `tiny` fixture.

## 2. Root cause

The engine already has the right model and stops one step short of using it.

- **DISTINCT has no aggregator form in this IR, by design.** `architecture.md:297-323` ("DISTINCT
  lowers to grouping"): the state of a distinct aggregate is the set of its values, the only set
  the IR can hold is the rows of a grouped table, so the distinct argument becomes a group key of an
  inner aggregate. `PlanAgg` (`plan/mod.rs:369-377`) has no distinct variant; `AggregateBody`
  (`:565-580`) carries no flag; both backends run an aggregate node purely from `aggs`
  (`cpu_backend/mod.rs:323-384` via `state_funcs`; `aggregate.cpp` from `aggr_funcs()`).
- **Today the lowering is DataFusion's, not ours.** `decompose` (`aggregate.rs:105-209`) consumes
  the two `AggregateExec`s DataFusion's rewrite produces as two ordinary aggregates. Where DataFusion
  declines to rewrite, no one lowers, so `:119` refuses.
- **Why DataFusion declines is a limit of its rewrite, not of the shape.** Its outer aggregate
  re-applies *the same function* to the inner's result (`single_distinct_to_groupby.rs:192-218`:
  `func` reused with `col(alias)`), which is only sound where `f(f(x)) = f(x)` — true of sum/min/max,
  false of count and avg. This engine has already split every aggregate into `init` and `merge`
  (`plan/aggregates.rs:36-70`: `count` merges by `sum`, `avg` is `[sum, count]` merged by
  `[sum, sum]`), so the outer stage can run each companion's **merge** aggregators over the inner's
  state — exactly what the per-lane merge node does today (`aggregate.rs:348-368`), only grouped on
  one key fewer.
- **Why the fix cannot be a kernel** (the ticket's own suggestion, cuDF `nunique`): a per-batch
  `nunique` has no merge — Σ over batches is not the count of the union — so it would answer only
  where one node sees every row (the one-lane single-batch shortcut, `aggregate.rs:317-333`), and the
  CPU twin would run DataFusion's `count` with `.distinct()` in Partial mode, whose *state* is a
  `List` (`datafusion-functions-aggregate-45.0.0/src/count.rs:154-161`) and fails
  `check_state_layout` (`cpu_backend/mod.rs:467-491`) against the declared `Int64`. It would also
  contradict the design sentence "`DISTINCT` is never a property of an aggregator".

So the mechanism is: the one place the IR can represent a set is a grouped stage, the translator
already knows how to build grouped stages and how to merge state, and `:119` refuses instead of
composing the two.

## 3. Localized fix

**One file carries the change:** `peacockdb-core/src/planner/translator/aggregate.rs`. No new
node kind, no `PlanAgg`/`AggFunc` variant, no wire field, no C++ change, no backend change. The
rewrite emits only nodes both backends already run — `GpuAggregate`, `GpuAggregateBatches`,
`GpuMergePartitions`, `GpuEmitPartitions`, `GpuCoalesceAllBatches` — with bodies of the shapes
they already take.

### 3.1 The lowering

For a partial with group keys `G` and aggregates `A`, some of which are `f(DISTINCT d)` over one
argument expression `d` (all distinct aggregates sharing it):

```
inner stage   group by G ++ [d]         aggs = init of every NON-distinct companion   (no finalize)
outer stage   group by G                aggs = for each companion, its MERGE aggregators over the
                                               inner's state columns (init-form calls: sum/min/max)
                                             + for each f(DISTINCT d), f's INIT over column d
                                        finalize = each aggregate's ordinary finalize
```

Each stage then takes the ordinary sequence (init → per-lane merge → shuffle → final merge). The
inner takes the shuffle DataFusion planned for the un-rewritten aggregate — a hash on `G` co-locates
every `(G, d)` group, and a Collapse co-locates everything — and the outer takes `Shuffle::None`,
because its input is already at the lane count and hash the un-rewritten final would have seen. This
is legal by the rule `GpuAggregateBatches::validate_schemas_and_partitions` already enforces
(`plan/aggregate.rs:236-258`: the hash need only be a *subset* of the group columns) and by
`regrouped_key_distribution` (`:360-383`) carrying the hash on `G` through both stages.

Correctness: `Σ_over_(G,d)-groups sum(x) = sum(x)`; `Σ count(x) = count(x)`; `avg = Σ sum / Σ count`;
`min/max` idempotent; `count(d)` over one row per distinct `d` counts non-null distinct values (the
NULL group is one row, and `count` skips it — the same argument `architecture.md:317-321` already
makes for DataFusion's rewrite). Both engines evaluate identical trees, so they agree by construction.

### 3.2 Line-level changes in `aggregate.rs`

**(a) `decompose` learns where an aggregate's init reads** (`:94-209`). Add

```rust
/// Where an aggregate's init reads. `Values` is the ordinary case. `State` is the outer half of a
/// DISTINCT lowering: the stage below already built this aggregate's state at these positions of
/// its output, and the init here is its merge — sound because init and merge are separate
/// aggregators, which is what DataFusion's own rewrite lacks (it re-applies the same function).
enum InitFrom<'a> { Values, State(&'a AggStateColumns) }
```

and change the signature to `decompose(aggregates: &[(Arc<AggregateFunctionExpr>, InitFrom<'_>)],
input_schema, n_keys)`. In the loop body:

- delete `:118-124` (the refusal moves to `distinct_argument`, below, where it becomes three narrower
  ones);
- `declared` (`:127-129`): `Values` → `aggregate.state_fields()` as today; `State(cols)` → the
  `input_schema` fields at `cols.positions` (already DataFusion-typed by the stage below), and the
  `:130-137` arity check compares `cols.positions.len()` instead of `declared.len()`;
- `args` (`:139-142`): built only for `Values`;
- state fields (`:148-157`): unchanged — names are `aggregate.name() + suffix` either way, so the outer
  state carries the same names as the inner state, which is what `check_merges_the_state_it_was_given`
  (`plan/aggregate.rs:276-350`) and the goldens read;
- init calls (`:159-165`): `Values` → as today; `State(cols)` → `match rule.merge {
  Merge::PerColumn(funcs) => zip(funcs, cols.positions, state) → AggCall { func, args:
  vec![Expr::column(position, input_schema.field(position).name())], outputs: vec![field] },
  Merge::Combined(_) => Err(PlanError::Invalid(..)) }` — the Combined arm is unreachable because
  `distinct_argument` refused it, and stays exhaustive rather than silent;
- merge calls, finalize, annotations, `state.extend` (`:167-205`): unchanged.

Every existing call site passes `InitFrom::Values` for each aggregate.

**(b) Split `aggregate_sequence` (`:242-388`) into "read the exec" and "assemble the nodes".**

```rust
/// One aggregate stage as the sequence builder consumes it.
struct Stage {
    group_by: Vec<Expr>, grouping_sets: Vec<Vec<bool>>, null_exprs: Vec<Expr>,
    key_fields: Vec<Field>, decomposed: Decomposed,
    /// The finalize list under DataFusion's output names, and the finalized schema — `None` for a
    /// stage that hands its state on.
    finished: Option<(Vec<NamedExpr>, Schema)>,
}
fn sequence(input: Box<dyn GpuNode>, stage: Stage, shuffle: Shuffle) -> Box<dyn GpuNode>
```

`sequence` is today's `:279-388` verbatim, reading its inputs from `stage` (intermediate schema and
`keys_through` from `key_fields` + `decomposed.state`, the shortcut, the init node, the per-lane
merge, the shuffle, the final merge). `finished_by(finisher, &decomposed, n_keys)` is today's
`:294-313` as a function. `aggregate_sequence` keeps `:248-276` (input, filter refusal, group_by,
key_fields, null_exprs, grouping_sets) and ends with:

```rust
match distinct_argument(partial, &input_schema)? {
    None => Ok(sequence(input, Stage { .., decomposed: decompose(&values(partial.aggr_expr()), ..)?,
                                       finished: finished_by(finisher, ..) }, shuffle)),
    Some(d) => {
        let (inner, outer) = distinct_stages(partial, finisher, &input_schema, group_by, key_fields, d)?;
        let inner = sequence(input, inner, shuffle);
        Ok(sequence(inner, outer, Shuffle::None))
    }
}
```

One deliberate one-line generalization inside `sequence`: the shortcut condition (`:317-320`)
becomes "one lane and a single batch", with `finalize: stage.finished.map(..)` and the output schema
falling back to the intermediate. Otherwise an inner stage over an accumulator would carry a merge of
one batch. No committed golden moves: no plan golden holds a `GpuAggregateBatches` without `final=`
at `lanes=1`, nor a `GpuAggregate` without `final=` at `lanes=1, batches=single` (grep over
`testdata/goldens/*/*.plans.txt`, both zero). The developer proves it with an unchanged
`test_plan_goldens` outside the q28 sections.

**(c) `distinct_argument(partial, input_schema) -> Result<Option<DistinctArg>, PlanError>`** (new,
~40 lines), where `DistinctArg { expr: Expr, name: String, data_type: DataType, nullable: bool }`:

- no `is_distinct()` aggregate → `Ok(None)`;
- `!partial.group_expr().is_single()` → `Unsupported("DISTINCT under grouping sets (#NEW)")`;
- a distinct aggregate with `expressions().len() != 1` → `Invalid` (DataFusion's own rule,
  `single_distinct_to_groupby.rs:188`);
- `translate_expr` each distinct argument; any two unequal (`Expr: PartialEq`) →
  `Unsupported("DISTINCT over two different arguments … (#144)")` — the message #144 says the
  planner already gives, today only through #62's text;
- any companion whose `decomposition(resolve(fun().name())?.func).merge` is `Merge::Combined` →
  `Unsupported("DISTINCT beside stddev/var (#NEW)")` (see 3.5 for why);
- `name` = the input field's name for a `Column`, else the expression's display; `data_type`,
  `nullable` from `arg.data_type/nullable(&input_schema)`.

**(d) `distinct_stages(partial, finisher, input_schema, group_by, key_fields, d)`** (new, ~60
lines):

- **inner keys**: `group_by ++ [d.expr]` and `key_fields ++ [Field::new(d.name, d.data_type,
  d.nullable)]`, unless `group_by` already contains `d.expr`, in which case nothing is appended and
  `d_at` is that key's position (a `GROUP BY x … count(DISTINCT x)` would otherwise declare two
  columns named `x`). `grouping_sets`/`null_exprs` empty (refused above).
- **inner decomposed** = `decompose(companions paired with InitFrom::Values, input_schema,
  inner_keys.len())`; `finished: None`.
- **inner intermediate Arrow schema** = inner key fields ++ `inner.decomposed.state` — the same
  concatenation `sequence` makes at `:279-285`; this is the outer's `input_schema`.
- **outer keys** = `Expr::column(i, key_fields[i].name())` for `i < |G|`; `key_fields` = the original
  `G` fields.
- **outer decomposed** = `decompose(over the original aggregates in their original order, .., |G|)`
  where a companion pairs with `InitFrom::State(&inner.decomposed.annotations[j])` (its `j`-th
  companion annotation — positions already index the inner's output) and a distinct one pairs with
  `InitFrom::Values` over its **non-distinct twin**:

  ```rust
  AggregateExprBuilder::new(Arc::new(aggregate.fun().clone()),
                            vec![Arc::new(Column::new(&d.name, d_at))])
      .schema(Arc::new(inner_intermediate.clone())).alias(aggregate.name()).build()
  ```

  (`datafusion-physical-expr-45.0.0/src/aggregate.rs:81,94-150,155,160`; the builder needs args,
  schema and alias, all given). The twin's `state_fields()` are `f`'s ordinary ones — `count` →
  `Int64` (`count.rs:163-167`), never the distinct `List` — and `translate_expr` on its argument
  yields `Expr::column(d_at, d.name)`, which `check_column_refs` (`plan/common.rs:29-57`) verifies
  against the inner intermediate. Original order matters: `finished_by` names outputs by position in
  `finisher.schema()`.
- **outer finished** = `finished_by(finisher, &outer.decomposed, |G|)`.

### 3.3 What the plan looks like

tpcds q28, one `B_i` subquery, tp1-single (scan declares `MultipleBatches`, so the inner still
merges; the outer takes the shortcut):

```
GpuAggregate: group_by=[], aggs=[sum(avg(..)$sum@1) as avg(..)$sum, sum(avg(..)$count@2) as avg(..)$count,
                                 sum(count(..)@3) as count(..), count(ss_list_price@0) as count(DISTINCT ..)],
              final=[CAST(avg$sum)/CAST(avg$count) as avg(..), count(..)@.. , count(DISTINCT ..)@..], lanes=1, batches=single
  GpuAggregateBatches: group_by=[ss_list_price@0], aggs=[sum(..$sum@1), sum(..$count@2), sum(count@3)], lanes=1, batches=single
    GpuAggregate: group_by=[ss_list_price@k], aggs=[sum(ss_list_price@k) as avg(..)$sum, count(ss_list_price@k) as avg(..)$count,
                                                    count(ss_list_price@k) as count(..)], lanes=1, batches=multiple
      GpuFilter …  GpuLoadParquet …
```

tp4 (DataFusion plans `Final ← CoalescePartitions ← Partial`, so `Shuffle::Collapse`): inner init at
4 lanes → per-lane `GpuAggregateBatches` (group by `d`) → `GpuMergePartitions` → inner final merge at
1 lane → outer shortcut at 1 lane. A grouped variant with `Shuffle::ByHash{G, n}`: inner init → per-lane
merge → `GpuMergePartitions` + `GpuCoalesceAllBatches` + `GpuEmitPartitions(G)` → inner final merge at
n lanes hashed on `G` → outer init at n lanes → outer final merge + finalize at n lanes, validated by the
subset rule. Where DataFusion planned no repartition because the input was already hashed on `G`
(`SinglePartitioned`, q16's shape), both stages ride that hash.

### 3.4 How CPU and GPU stay one engine

Nothing backend-specific changes. The one new *body shape* is an init node (`Phase::Init`,
`CudfAggregate{Partial}`) whose `sum`/`min`/`max` calls read a state column of the stage below:

- CPU: `init_aggregates` (`cpu_backend/mod.rs:388-406`) resolves `agg_name(PlanAgg::Sum)` = `"sum"`
  and builds DataFusion `sum` over that column; `check_state_layout` (`:467-491`) accepts it exactly
  as it accepts today's merge — a decimal `sum` comes back wider at the same scale
  (`widened_decimal`, `:499-506`) and `declared_as` relabels it, `sum(UInt64)`/`sum(Int64)` keep their
  type, `count(d)` is `Int64`.
- GPU: `aggregate.cpp` Partial mode reads `func->args()` (`:184-200`), `make_agg("sum", false)` is
  `SUM` (`:86-87`), `count` is `COUNT` then cast to `INT64` (`:76-83`, `:819-823`); the keyless path
  does the same with `reduce` and the single-group trick for decimals (`:231-326`). All of it is
  code that runs today for ordinary inits.
- The merges above either stage are ordinary `Merge`-mode nodes with `sum`-only funcs, so the C++
  width recovery (`:503-532`) sees `n_avg = n_std = 0`.

### 3.5 What it deliberately does not touch

- `flatbuffers/gpu_plan.fbs` `AggregateFuncNode.distinct`: stays, still never written. The C++ guard
  (`aggregate.cpp:143-155`) stays as the exhaustive decode of a field no writer sets; only its
  comment changes (3.7).
- A Welford companion (`stddev`/`var` beside a DISTINCT) is refused. Its merge is `merge_m2`, and
  there is no *init*-form `merge_m2` on either engine: the CPU folds a Welford triple into DataFusion's
  one-argument `stddev` in the init path (`plan/aggregate.rs:20-57`, `cpu_backend/mod.rs:388-406`),
  and the C++ Partial arm computes Welford over one column (`aggregate.cpp:615-631`). Lifting it needs
  either a C++ Partial-mode merge arm or the pseudo-state variant in §4. No benchmark query has it.
- DISTINCT under grouping sets is refused (DataFusion's own rewrite also declines it,
  `single_distinct_to_groupby.rs:130`). The extension is mechanical — masks gain a `false` for `d`,
  the outer groups on `G ++ [__grouping_id]` — but nothing needs it.
- The shuffle decision: the inner reuses what DataFusion planned; no invented hash on `d`.
- `memory_estimation.rs`: the stacked stages are priced by the existing per-node arms
  (`:238-240`, one row per input row). `validate.rs::aggregate_width` (`:231-245`) already computes a
  no-finalize node's width from its state.

### 3.6 Tests, goldens, registry, comments that move with it

- **Delete** `test_planner_join_refusals.rs:104-117`. **Add** there, from SQL over `tiny`:
  `two_distinct_arguments_are_refused_naming_144` (`SELECT count(DISTINCT k), count(DISTINCT v) FROM
  tiny`), `a_distinct_beside_a_welford_companion_is_refused_naming_NEW` (`SELECT stddev(v),
  count(DISTINCT v) FROM tiny`), and the grouping-set one if DataFusion plans it
  (`SELECT k, count(v), count(DISTINCT v) FROM tiny GROUP BY ROLLUP(k)`; see §7).
- **Add** to `planner/translator/tests.rs` beside `:302-360`: (i) `a_distinct_beside_a_count_lowers_to_a_dedup_stage_and_a_merge_over_it` —
  `SELECT n_regionkey, count(n_nationkey), count(DISTINCT n_name) FROM nation GROUP BY n_regionkey`
  at one lane: shape `Unload(Aggregate(AggregateBatches(Aggregate(LoadParquet))))`, inner
  `group_by` = `[n_regionkey, n_name]`, inner `aggs` = `[count]`, outer `aggs` = `[Sum, Count]` with
  the `Sum` reading the inner's `count` state ordinal and the `Count` reading `n_name@1`, outer
  finalize `Some`, `validate_all`; (ii) the keyless form at four lanes: inner takes the collapse,
  outer is the shortcut; (iii) a grouped form at four lanes: `GpuEmitPartitions` sits inside the
  inner and the outer's final merge validates (`hashed_on` `G`); (iv) `d` already a group key adds no
  column.
- **Add** to `test_cpu_end_to_end.rs` beside `:406-416`, `Coverage::ModesOnly`, against DataFusion on
  the same SQL: `SELECT ss_store_sk, count(ss_customer_sk), count(DISTINCT ss_customer_sk) FROM
  store_sales WHERE ss_quantity BETWEEN 0 AND 5 GROUP BY ss_store_sk` and its keyless twin —
  `count` companions rather than `avg`, so the case runs before #163 lands, and a column with NULLs so
  the non-null rule is asserted rather than assumed.
- **Add** one recipe-walk case to `test_gpu_recipe_walk.rs` at `TWO_LANES` (shad-gpu): the same grouped
  SQL, which is the first time a device runs a Partial `sum` over another aggregate's state column.
- **Regenerate** `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup,tp4-single,tp4-rowgroup,tp4-sized}.plans.txt`:
  only the `== q28` sections change (refusal → tree + recipes + memory). `recipe-payloads.txt` does not
  change: q28 adds no call shape the cover lacks (init+finalize shortcut, merge with and without
  finalize, cross join are all present; `the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce`
  is the proof). No execution golden moves until q28's cpu cells are enabled (§6).
- **Registry** `testdata/cost-registry.csv`: `:29` → plan cells `enabled`×5, cpu `disabled`×5, gpu
  `disabled`×5, tickets `163 152 187` (the walls behind it, §6); drop `62` from `:17`, `:95`, `:96`,
  `:116`. `the_registry_matches_the_goldens_in_both_directions` (`test_plan_goldens.rs:671-714`) is
  what forces the plan cells.
- **`corpus_cases.inc`**: add `corpus_query!(tpcds, 1, q28, none, none, data_fusion_exact,
  golden_exact);` with a comment naming #163 (the `avg`), following the q1 convention at `:19-23`.
- **Comments**: `plan/mod.rs:52-53` (`PlanError::Unsupported`'s example list names "a mixed distinct
  (#62)"); `aggregate_writer.rs:149-158` gains one clause saying `distinct` is never set because the
  planner lowers DISTINCT to grouping; `aggregate.cpp:143-149` rewritten to name the lowering and the
  writer line as the reason the flag never arrives (hacks-audit finding 10's "keep the guard and
  correct the comment" option).
- **Wiki**: `architecture.md:311-315` is falsified as a description ("Any other companion is refused
  at plan time (#62)"; "closing #62 is a planner rewrite") — rewrite the paragraph to describe the two
  stages and the two residual refusals; `tickets.md` closes #62 (to `archive/archived-tickets.md`) and
  files the residual ticket (≤15 lines: Welford companion or grouping sets beside a DISTINCT); the
  index at `tickets.md:19` drops #62; `build-test.md:42`'s corpus count gains the `none` line.

### 3.7 hacks-audit scaffolding

Finding 10 (`llm-wiki/reports/hacks-audit.md:220-247`) is the only item that names this code. Its
first option — drop the guard with the field — is an fbs change and is rejected; its second — keep
the guard, correct the comment — is what 3.6 does. The name-set sloppiness it also records
(`is_stddev_name` returning `false` on drift) is untouched: not this bug. Nothing else in the audit
grew around #62; the fix leaves no scaffolding of its own (no flag, no variant, no `bug_` test).

## 4. Alternatives rejected

- **Distinct-aware kernel** (the ticket's text: cuDF `nunique` + DataFusion `count` with
  `.distinct()`): per-batch `nunique` cannot be merged, the CPU's distinct state is a `List` that
  fails `check_state_layout`, and it contradicts "DISTINCT is never a property of an aggregator". It
  would re-enable nothing beyond a one-node plan.
- **Pseudo-state so the outer is a pure merge**: inner adds `max(CASE WHEN d IS NULL THEN 0 ELSE 1 END)`
  per `(G, d)`, a project drops `d`, the outer merges everything by its merge funcs (lifts the Welford
  restriction). Costs a project, a merge grouping on a subset of its input's keys, and a CASE on the
  device column path (#57's neighbourhood) — three shapes for a case no query has. The right follow-up
  if a Welford companion ever appears.
- **Invent a hash shuffle on `d` for the inner** (DataFusion's own rewrite shape at tp4): parallelizes
  the inner's final merge but plans a shuffle DataFusion did not, and the keyless outer collapses
  right after it anyway.
- **Wait for DataFusion**: `SingleDistinctToGroupBy`'s limit is inherent to re-applying the same
  function; #23 already owns the upgrade and does not change this.
- **A `CountDistinct` aggregator in the registry**: its state is a set, which the IR cannot declare.
- **Refuse until #163**: the two are independent; `count` companions prove the rewrite today.

## 5. Minimum corpus query

Against tpch sf1:

    SELECT l_returnflag, count(l_quantity), count(DISTINCT l_quantity)
    FROM lineitem WHERE l_shipdate < DATE '1992-02-01' GROUP BY l_returnflag

and the keyless form without the `GROUP BY`. Both must plan at all five modes (tp1-single, tp1-rowgroup,
tp4-single, tp4-rowgroup, tp4-sized — the grouped one exercises the ByHash inner, the keyless one the
Collapse) and run on both backends. Today neither plans at any mode on either backend: `planner::plan`
returns `unsupported: DISTINCT inside count(DISTINCT lineitem.l_quantity) (#62)` from
`aggregate.rs:119`, before a backend exists. The shape is q28's minus the `avg` (which #163 refuses at
run time on the CPU) and minus the six-way cross join. With `avg(l_quantity)` added it is q28's exact
aggregate shape, refused at the same line.

## 6. Cells re-enabled

- **Immediately**: tpcds/q28's five **plan** cells (`tp1_single … tp4_sized`) — the goldens carry a
  tree, `plan_status` stays `ok`.
- **With #163** (the `avg` count-state type, and per 163-proposal its decimal finalize on the CPU):
  tpcds/q28 **cpu** × 5. Nothing else in q28 is refused: its cross joins run on the CPU (tpch/cross_join,
  tpcds/q61 do), `count(ss_list_price)` under a WHERE is not a statistics count (#158), and the root
  `LIMIT 100` is the unload's interval.
- **Stay off**: tpcds/q28 **gpu** × 5 behind #152 (`GpuCrossJoin` copies its build side —
  `test_gpu_recipe_walk.rs:789`), #187 (the `avg` columns are `Decimal128(11,6)` at the unload) and
  #163.
- The `62` tags on tpcds q16/q94/q95 and tpch q16 go; their cells do not move.
- Exec-model corpus: q28 stays absent (hand-lowered set, manual tier); adding it is separate.

## 7. Risks and unknowns

- **Not run.** Everything above is by reading; the developer's first proof is the translator unit
  tests, then `test_plan_goldens` with only q28's sections moving, then the end-to-end case.
- **Reachability of the grouping-set refusal** from SQL is unverified: DataFusion 45 may plan
  `count(DISTINCT)` under `ROLLUP` (then the refusal test is writable) or refuse it itself (then the
  arm is a constructor-only guard, like the three `test_planner_join_refusals.rs:7-10` names). Either
  way the arm stays.
- **`sum` over the `avg$count` state on the CPU** rests on DataFusion `sum(UInt64) → UInt64` — the
  same fact today's merge relies on — and on whatever #163 decides that column's declared type is.
  The rewrite types the outer's companion state by the inner's state fields, so it inherits #163's fix
  rather than needing its own.
- **A non-Column distinct argument** (`count(DISTINCT a + 1)`) becomes an expression group key: the
  CPU evaluates it; the C++ throws `only ColumnRef group exprs supported` (`aggregate.cpp:163`) at run
  time, as it does for any `GROUP BY <expr>` today. Not new, not refused, and no corpus query has it.
- **Name collision** when `d` is already a group key is handled by reusing the key's ordinal; when `d`
  equals a companion's state name — impossible, state names carry the aggregate's own name.
- **cuDF on shad-gpu (25.02)**: groupby `SUM` over `UINT64` and `COUNT_VALID` over `DECIMAL128` are
  assumed supported; the recipe-walk case is where that is proven.
- **The `Stage`/`sequence` restructure** is a refactor of `:279-388`; its proof is byte-identical goldens
  outside q28. Do it in its own commit before the rewrite.

## 8. Complexity

**M.** One production file (`planner/translator/aggregate.rs`, roughly +170/−15: an enum, a struct,
two new functions, one function split), three test files (+7 cases, −1), one device-tier case, five
tpcds plan goldens regenerated for one section each, six registry cells and four tag edits, one
`corpus_cases.inc` line, three comment edits (C++, wire writer, `plan/mod.rs`), and the wiki (one
architecture paragraph, one ticket closed, one filed). No frozen surface changes: no C ABI symbol, no
fbs field, no wire bytes (`recipe-payloads.txt` unchanged), no declared-schema contract. No execution
golden moves until #163 enables the cells. The reason it is not S is the `sequence` split and the
`InitFrom` threading, which touch the one function every aggregate in the corpus goes through and
must be proven by an unchanged golden set.
