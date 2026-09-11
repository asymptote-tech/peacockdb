# #23 — DataFusion 45 refuses q27/q70/q72/q86: localized alternatives to the upgrade

Read-only proposal, at master c18e063a. Paths relative to `/media/data/peacockdb`; DataFusion
sources read from `~/.cargo/registry/src/*/datafusion-*-45.0.0` (only 45 is cached, so every
claim about 46+ below is the ticket's, not mine — see §7).

## 1. Issue

Four TPC-DS queries never reach the engine's planner: `ctx.sql(..).create_physical_plan()`
fails, and `peacockdb-core/tests/test_plan_goldens.rs:60-74` renders `refused by datafusion:`
where the tree would be. `the_registry_matches_the_goldens_in_both_directions`
(`test_plan_goldens.rs:686-690`) maps that line to `na` in every plan column, and
`tests/common/registry.rs:242-251` refuses any cell but `disabled`/`na` on a
`plan_status=fail` row. Registry rows 28, 71, 73, 87 of `testdata/cost-registry.csv`: all
fifteen cells `na`, `plan_status=fail`. No `corpus_query!` line exists for any of the four
(`tests/common/corpus_cases.inc`), and none is in the exec-model corpus.

The four are three different defects, and the ticket's grouping hides that two of them
are not worth fixing at all:

| query | what DataFusion says (`testdata/goldens/tpcds.sf1/tp1-single.plans.txt`) | line |
|---|---|---|
| q27 | `SanityCheckPlan … SortPreservingMergeExec [i_item_id, s_state] … does not satisfy order requirements … Child-0 order: []` — the child is a `UnionExec` | 2633 |
| q72 | `type_coercion … Cannot coerce arithmetic expression Date32 + Int64` (`d3.d_date > d1.d_date + 5`) | 7431 |
| q70, q86 | `Physical plan does not support logical expression AggregateFunction(Grouping …)` | 7320, 9549 |

Same text at all five modes (tp4-single 4730/13455/13635/17499, tp4-sized 4343/12255/
12420/15930): none of the three is mode-dependent.

**q70 and q86 are window queries** (`rank() OVER (…)` in both texts,
`testdata/tpcds-queries/q70.sql:5`, `q86.sql:5`), and the engine refuses every window plan
by decision — `planner/translator/nodes.rs:159-163`, ticket #143 archived as "window support
is not planned". Whatever DataFusion version plans their `grouping()`, they move from
`refused by datafusion:` to `refused: unsupported: BoundedWindowAggExec: window functions
(#143)` like q36/q67 today, and their cpu/gpu cells stay `na`. The ticket's "unblock
q70/q86" is wrong for this engine, and #65's "(q70/q86 after #23)" (`tickets.md:282`)
with it. Their registry `tickets` column also still carries closed #97.

So what #23 can re-enable is q27 and q72, and for both a localized fix exists that needs no
upgrade, no ABI symbol and no wire change. What each buys afterwards is bounded by other
tickets (§6): q27 carries four `avg`s (#163 takes every cpu cell), q72 a shuffled `count(*)`
(#180 takes the tp4 cells) and a join order that may not fit host memory (§7).

## 2. Root cause

### q27 — DataFusion 45 drops a union's ordering beside a shared literal column

The query is a hand-expanded rollup: `UNION ALL` of three aggregates, each carrying a
literal `g_state` column, under `ORDER BY i_item_id, s_state LIMIT 100`. Physical planning:

1. `EnforceSorting` (`datafusion-physical-optimizer-45.0.0/src/enforce_sorting/mod.rs:159-199`)
   turns `Sort(fetch=100) → CoalescePartitions → Union` into `SortPreservingMerge(fetch=100)
   → Sort(preserve_partitioning) → Union` (`parallelize_sorts`, on by default), then
   `pushdown_sorts` pushes that sort through the union to every branch
   (`sort_pushdown.rs:244-249`: "UnionExec does not have real sort requirements").
2. Per branch (`sort_pushdown.rs:85-140`): branch 1 gets `SortExec TopK [i_item_id,
   s_state]`; branch 2 (`NULL AS s_state`) gets `SortExec TopK [i_item_id]` — the constant
   was retained out; branch 3 (`NULL, NULL` over a keyless aggregate) is *all constants*, so
   `ordering_satisfy_requirement` normalizes the requirement to nothing and no sort is added.
   Every branch is, in fact, sorted on `[i_item_id, s_state]`.
3. The union's own properties are recomputed by `calculate_union`
   (`datafusion-physical-expr-45.0.0/src/equivalence/properties.rs:2169-2191`) folding
   branches left to right through `calculate_union_binary` (`:2112-2167`). After branches
   1+2 the accumulator holds ordering `[i_item_id, s_state]` and constants `{g_state}` —
   the literal every branch carries, kept because `find` matches the constant by column, not
   by value. Folding branch 3 calls `UnionEquivalentOrderingBuilder::try_add_ordering`
   (`:2258-2274`):

       } else if constants.is_empty() && properties.ordering_satisfy(ordering.as_ref()) {

   `constants` is `{g_state}`, so the direct check is skipped and it falls to
   `try_find_augmented_ordering` (`:2280-2305`), which can only walk an *existing* ordering
   of branch 3 — and branch 3 has none. The ordering is popped to empty. Union ordering: `[]`.
4. `SanityCheckPlan` (`sanity_checker.rs:138-149`), the last rule
   (`optimizer.rs:82-144`), checks the merge's `required_input_ordering`
   (`sorts/sort_preserving_merge.rs:230-232`) against that and refuses.

The plan is semantically valid; DataFusion's union-ordering calculus loses it. Without the
shared literal column, `constants` is empty after branches 1+2, the direct
`ordering_satisfy` runs, branch 3 satisfies it through its own constants, and the plan
passes — which is why no other union in the corpus hits this.

### q72 — `Date32 + Int64` has no coercion, in DataFusion or in arrow

`d3.d_date > d1.d_date + 5`. `TypeCoercion` asks `datafusion-expr-common-45.0.0/src/
type_coercion/binary.rs:140-193` for an arithmetic signature: it first tries arrow's own
kernel on empty arrays (`add_wrapping(Date32, Int64)`), then temporal coercion, decimal,
numeric, and refuses at `:191`. arrow's date arithmetic (`arrow-arith-54.2.1/src/
numeric.rs:233`, `date_op` at `:650-700`) takes `Interval*`, `Duration*` and `Date − Date`
only — no integer arm. I found no version of either crate that adds one, so the ticket's
claim that 46+ plans q72 is unsupported by anything I can read; the honest statement is that
`date + int` is DuckDB/Postgres dialect (days), and the engine has to say so itself.

### q70/q86 — `grouping()` inside ORDER BY, never rewritten

`ResolveGroupingFunction` (`datafusion-optimizer-45.0.0/src/analyzer/
resolve_grouping_function.rs:131-149`) rewrites `grouping()` only where it sits in an
`Aggregate`'s `aggr_expr`. The SQL planner's aggregate haystack is select + having only
(`datafusion-sql-45.0.0/src/select.rs:144-146`); the ORDER BY's `CASE WHEN grouping(s_state)
+ grouping(s_county) = 0 THEN s_state END` is planned against the combined schema and lands
raw in the `Sort` (`select.rs:287`), where `create_physical_expr` meets an
`Expr::AggregateFunction` and refuses. Consistent with the error naming the first
`grouping()` of each query's ORDER BY. Not pursued — §1.

## 3. Localized fix

Two rules on the one session every planning site builds — `build_session_state`
(`peacockdb-core/src/lib.rs:25-36`), used by the CLI, every test tier, the corpus oracle
(`tests/common/corpus.rs:332-338`) and the recipe walk — so the CPU oracle, the engine's
translator and the device recipe all see the same DataFusion plan. Neither touches
`plan/`, `wire/`, `executor/` or C++.

### 3a. `MergeInputSort` — a physical optimizer rule, before `SanityCheckPlan`

New file `peacockdb-core/src/planner/merge_input_sort.rs` (~40 lines + tests). Declared in
`planner/mod.rs` as `pub struct MergeInputSort;` with `mod merge_input_sort;` (the facade
declares the type, the implementation module implements the trait — `coding-style.md`
Visibility). Body:

    impl PhysicalOptimizerRule for MergeInputSort {
        fn optimize(&self, plan: Arc<dyn ExecutionPlan>, _: &ConfigOptions)
            -> Result<Arc<dyn ExecutionPlan>> {
            plan.transform_up(|node| {
                let Some(merge) = node.as_any().downcast_ref::<SortPreservingMergeExec>() else {
                    return Ok(Transformed::no(node));
                };
                let input = merge.input();
                if input.equivalence_properties().ordering_satisfy(merge.expr()) {
                    return Ok(Transformed::no(node));
                }
                let sort = SortExec::new(merge.expr().clone(), Arc::clone(input))
                    .with_fetch(merge.fetch())
                    .with_preserve_partitioning(
                        input.output_partitioning().partition_count() > 1,
                    );
                Ok(Transformed::yes(node.with_new_children(vec![Arc::new(sort)])?))
            })
            .data()
        }
        fn name(&self) -> &str { "MergeInputSort" }
        fn schema_check(&self) -> bool { true }
    }

`ordering_satisfy(&LexOrdering)` is exactly what `SanityCheckPlan` will ask
(`LexRequirement::from(expr)`), so the rule fires iff the sanity check would have failed at
that merge: it is provably inert on every plan that plans today. `with_new_children` keeps
the merge's `fetch` and round-robin flag. APIs checked in 45: `SortPreservingMergeExec::
{input, expr, fetch}` (`sort_preserving_merge.rs:141-152`), `SortExec::{new,
with_fetch, with_preserve_partitioning}` (`sort.rs:722-798`), `DynTreeNode for dyn
ExecutionPlan` (`physical-plan/src/tree_node.rs:28`).

Registration, `lib.rs::build_session_state`:

    use datafusion::physical_optimizer::optimizer::PhysicalOptimizer;
    let mut rules = PhysicalOptimizer::new().rules;          // `rules` is pub (optimizer.rs:70)
    let gate = rules.iter().position(|r| r.name() == "SanityCheckPlan")
        .expect("DataFusion's physical optimizer ends with SanityCheckPlan");
    rules.insert(gate, Arc::new(planner::MergeInputSort));
    let state = SessionStateBuilder::new_from_existing(base.state())
        .with_config(config)
        .with_physical_optimizer_rules(rules)                 // session_state.rs:1157
        .build();

`with_physical_optimizer_rule` (`:1168`) appends *after* the sanity check and is the wrong
call — the plan has already been refused by then. Inserting right before the gate, after
`LimitPushdown`, means the merge's `fetch` is final when the sort copies it.

What the translator then sees for q27, unchanged code: `SortPreservingMergeExec →
SortExec(preserve_partitioning, fetch=100) → UnionExec`. `nodes.rs:236-257`
(`sort_preserving_merge`) takes the `SortExec` child through `per_batch_sort` and emits
`GpuMergeSortedPartitions(fetch=100) → GpuSort(lanes = Σ branches) → GpuUnion`; the
branches keep their pushed-down top-100 sorts. At tp1 the union is 1+1+1 lanes, at tp4
4+4+1. A `GpuSort` over a multi-lane `GpuUnion` is a new plan shape for the goldens (today
multi-lane unions sit under `GpuMergePartitions` or `GpuAggregate`) but not a new node, rule
or recipe: Exec over a multi-lane multi-batch input, one `CudfSort` call per batch.

CPU and GPU agree by construction: both consume the plan the session produced; the CPU relay
already builds `SortExec` per batch (`executor/cpu_backend/accumulate.rs:55,239`), the
device recipe is `CudfSort` + `CudfSortPreservingMerge`, both exercised by every sorted
query today.

### 3b. `DateDayArithmetic` — a `FunctionRewrite` on the analyzer

New file `peacockdb-core/src/planner/date_arithmetic.rs` (~40 lines + tests), declared
`pub struct DateDayArithmetic;` in `planner/mod.rs`. A `FunctionRewrite`
(`datafusion-expr-45.0.0/src/expr_rewriter/mod.rs:46-60`) runs before every analyzer rule,
`TypeCoercion` included (`datafusion-optimizer-45.0.0/src/analyzer/mod.rs:157-166`), on
every expression with the merged input schema (`function_rewrite.rs:44-77`) — which is what
makes the type test possible before the coercion that refuses.

    impl FunctionRewrite for DateDayArithmetic {
        fn name(&self) -> &str { "date_day_arithmetic" }
        fn rewrite(&self, expr: Expr, schema: &DFSchema, _: &ConfigOptions)
            -> Result<Transformed<Expr>> {
            let Expr::BinaryExpr(BinaryExpr { left, op, right }) = &expr else {
                return Ok(Transformed::no(expr));
            };
            // A type the merged schema cannot resolve is TypeCoercion's to refuse, not ours.
            let (Ok(lt), Ok(rt)) = (left.get_type(schema), right.get_type(schema)) else {
                return Ok(Transformed::no(expr));
            };
            let (date, days) = match (op, lt, rt) {
                (Operator::Plus | Operator::Minus, DataType::Date32, t) if t.is_integer() => (left, right),
                (Operator::Plus, t, DataType::Date32) if t.is_integer() => (right, left),
                _ => return Ok(Transformed::no(expr)),
            };
            let as_days = |e: &Expr| Expr::Cast(Cast::new(Box::new(e.clone()), DataType::Int32));
            let shifted = Expr::BinaryExpr(BinaryExpr::new(Box::new(as_days(date)), *op, Box::new(as_days(days))));
            Ok(Transformed::yes(Expr::Cast(Cast::new(Box::new(shifted), DataType::Date32))))
        }
    }

Registration, after `build()`: `state.register_function_rewrite(Arc::new(planner::
DateDayArithmetic))?` (`session_state.rs:1876`, needs `use datafusion::execution::
FunctionRegistry`, already imported elsewhere in the crate). `build_session_state` then
returns `Result` or `expect`s — the call cannot fail, so `expect` with the reason is the
smaller change.

Why `Int32 ± Int32 → Date32` rather than an interval: `Date32 + IntervalDayTime` is what
DataFusion would accept natively, but an interval literal has no wire variant (#168,
`wire/serialize.rs:120-142` `convert_data_type`) and would leave the plan `not runnable` on
a device. `Date32 ↔ Int32` is an arrow reinterpret both ways (`arrow-cast-54.2.1/src/cast/
mod.rs:1370,1383`), `Int32 + Int32` needs no widening, `SimplifyExpressions` folds
`CAST(5 AS Int32)` to a literal, and every piece crosses the wire and the C++ as it is:
`translator/expr.rs:66-71` translates any `CastExpr`, `convert_data_type` carries `Int32`
and `Date32`, `cpp/src/expr.cpp:912-935` routes a non-INT64/FLOAT64 cast to `cudf::cast`,
and a hash join's residual takes the column path (`cpp/src/operators/join.cpp:353-367`).
Semantics: days, as DuckDB evaluates the same text (its q72 profile shows `d_date > (d_date
+ 5)`), so the cost oracle and the exec-model oracle agree with the engine. The physical
expression for q72 becomes `CAST(CAST(d_date@x AS Int32) + 5 AS Date32)` inside an Inner
join's residual filter, a shape the capability matrix already accepts.

Equal-sized alternative hook: `ExprPlanner::plan_binary_op` (`datafusion-expr-45.0.0/src/
planner.rs:101-107`, called from `datafusion-sql-45.0.0/src/expr/mod.rs:129`), the SQL
dialect layer. Either is one struct; the analyzer rewrite is what DataFusion's own
array-operator rewrites use and is testable without SQL.

### What it deliberately does not touch

q70/q86 (window queries, §1). The translator, the IR, the wire, the executors, the C++.
`SanityCheckPlan` stays in the list. No query text changes. No config knob moves. `#163`,
`#180`, `#152`, `#183` stay what they are: the fix makes q27 and q72 *plan*; whether they
*run* is those tickets' business (§6).

### Tests, and what pins the scaffolding

Both rules are code built around a dependency's defect, so `coding-style.md`'s rule applies:
each carries the ticket in its doc and a `bug_` test that goes red the day DataFusion no
longer needs it — the signal to delete the rule, its registration and the test together.

- `bug_datafusion_loses_a_union_ordering_beside_a_shared_literal` (in `merge_input_sort.rs`
  tests, `#[cfg(test)]`): a plain `SessionContext::new()` at `target_partitions=1` over
  `testdata/tpch.minimal` `nation`, the §5 SQL, asserts the error names `SanityCheckPlan`.
- `bug_datafusion_refuses_a_date_plus_an_integer`: plain session, `SELECT DATE '2000-01-01'
  + 5`, asserts `Cannot coerce arithmetic expression Date32 + Int64`.
- `a_merge_whose_input_lost_its_order_gets_a_sort_beneath_it`: through
  `build_session_state(1)`, the same SQL plans, the plan is `SortPreservingMergeExec →
  SortExec(preserve_partitioning=true, fetch) → UnionExec`, and `Translator::new(1,
  Batching::Off).translate` gives `GpuMergeSortedPartitions → GpuSort → GpuUnion`
  (pattern: `planner/translator/tests.rs:22-40`, `plan_at`).
- `a_date_plus_an_integer_counts_days`: through the session, `SELECT DATE '2000-01-01' + 5
  = DATE '2000-01-06'` returns true, and the analyzed plan of `SELECT d + 5 FROM …` carries
  `CAST(CAST(d AS Int32) + Int32(5) AS Date32)`. No dataset needed.
- Two inline-SQL cases in `tests/test_cpu_end_to_end.rs` via `sql_answers_match_datafusion`
  (pattern at `:405-415`), all five modes against DataFusion: the §5 union query with
  `sum(n_nationkey)` per branch (not `count(*)` — #180 at tp4 — and not `avg` — #163), and
  `SELECT count(*) FROM lineitem WHERE l_receiptdate > l_shipdate + 5`. The first is the
  only plan in the tier with a per-batch sort over a 9-lane union.
- `test_planner_join_refusals.rs` has no pin for #23 and needs none.

### Goldens, registry, corpus lines, wiki

- `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup,tp4-single,tp4-rowgroup,tp4-sized}
  .plans.txt`: sections `== q27` and `== q72` regenerate from a refusal line to a tree
  (`UPDATE_CANONICAL=1`, `test_plan_goldens`). Every other section is byte-identical — the
  merge rule is inert where the sanity check passes, and the only `Date32 ± int` in either
  corpus is q72 (grep of `testdata/*-queries`). `recipe-payloads.txt` is a cover over fb
  kinds and call shapes at tp4-rowgroup; q27/q72 add no kind, so it should not move — the
  test says so if it does.
- `testdata/cost-registry.csv` rows 28 and 73: five plan cells `enabled`, `plan_status=ok`,
  cpu/gpu cells `disabled` with `tickets` `163 152 183` (q27) and `180 152 183` plus
  whatever a first cpu run at tp1 decides (q72, §6/§7). Rows 71 and 87 (q70/q86): keep
  `fail`, drop closed `97`, and keep `23` only if the ticket keeps naming them.
- `tests/common/corpus_cases.inc`: `corpus_query!(tpcds, 1, q27, none, none,
  data_fusion_exact, golden_exact);` with a comment naming #163, and the q72 line with the
  modes the run decides, comment naming #180 for the tp4 modes. `every_device_cell_has_a_cpu_
  cell_at_the_same_mode` and the registry↔inventory test stay green by construction.
- `llm-wiki/tickets.md#t23`: rewrite — the two session rules are the scaffolding, the
  upgrade is what removes `MergeInputSort` (if 46+ fixes `try_add_ordering`) and *may not*
  remove `DateDayArithmetic` (arrow has no date+int); q70/q86 are #143's; the ticket leaves
  the "Blockers for disabled coverage" row at `:19`. `#t65`: drop "(q70/q86 after #23)".
- `hacks-audit.md`: names nothing under #23; the two `bug_` tests are the register the
  audit asks for, so the new scaffolding is findable by grep from day one.
- `architecture.md`: no sentence falsified. If a human wants it recorded, one clause under
  Planning ("what DataFusion is reused for…") naming the two session rules and #23.

## 4. Alternatives rejected

- **Upgrade DataFusion 45→46+ now** — the ticket's path; costs in §7. It removes at most one
  of the two rules and buys nothing the local fix does not, at the price of a whole-corpus
  regen.
- **`datafusion.optimizer.repartition_sorts = false`** — stops `parallelize_sorts`, so q27
  keeps `Sort → CoalescePartitions → Union` and plans; but every multi-lane sorted plan at
  tp4 loses its `SortPreservingMergeExec`, `GpuMergeSortedPartitions` leaves the corpus, and
  every tp4 golden moves. A global knob for one query.
- **Drop `SanityCheckPlan` and let the translator add the sort** — loses DataFusion's
  invariant check for every plan and duplicates its ordering calculus in `nodes.rs`.
- **`[patch.crates-io]` fork of `datafusion-physical-expr` 45 deleting the
  `constants.is_empty() &&` guard** — a one-line true fix, but a forked dependency the CI
  containers must fetch, kept in step with fourteen sibling 45.0.0 crates.
- **Rewrite `Date32 + int` to `Date32 + INTERVAL`** — DataFusion-native, but #168: no
  interval on the wire, so q72 would be `not runnable` on a device for a second reason.
- **Edit the query texts** (`+ INTERVAL '5' DAY`; drop `g_state`) — the texts are DuckDB's
  verbatim (`testdata/tpcds-queries/NOTICE`), the committed DuckDB profiles and dynamic
  filters were captured from them, and the exec-model oracle runs "the query's own text".
- **A `grouping()`-in-ORDER-BY analyzer rewrite for q70/q86** — correct in principle,
  pointless here: both are refused by #143's decision the moment they plan.
- **Do the date rewrite in the translator** — too late; `TypeCoercion` refuses before a
  physical plan exists.

## 5. Minimum corpus query

Both refused before the engine's planner runs, at every mode, on both backends alike (the
refusal is DataFusion's; `test_plan_goldens.rs:66-73` renders it, nothing of ours is reached).

**q27's defect** — tpch sf1 (or `tpch.minimal`), `nation`:

    SELECT r, n, g FROM (
        SELECT n_regionkey AS r, n_nationkey AS n, 0 AS g FROM nation
        UNION ALL SELECT n_regionkey, NULL, 1 FROM nation
        UNION ALL SELECT NULL, NULL, 1 FROM nation
    ) t ORDER BY r, n LIMIT 10;

`g` must be selected: projected away, the optimizer prunes it from the branches, the union
has no shared constant, and the plan passes (§2). Today: `refused by datafusion:
SanityCheckPlan …`. Expected after the fix: plans at all five modes; at tp1 the union is
three lanes, at tp4-single 4+4+4 (`nation` is below the small-table threshold only when
batching is on), and the answer matches DataFusion's.

**q72's defect** — tpch sf1, `lineitem`:

    SELECT count(*) FROM lineitem WHERE l_receiptdate > l_shipdate + 5;

Today: `refused by datafusion: type_coercion … Cannot coerce arithmetic expression Date32 +
Int64`. After: plans at all five modes as a filter over the scan, keyless aggregate, no
shuffle; the count equals the same query written with `+ INTERVAL '5' DAY` run through
DataFusion, which is the semantic pin.

## 6. Cells re-enabled

- **Plan columns**: q27 and q72, five cells each, `na → enabled`, `plan_status fail → ok`.
  These are the only cells #23 itself holds.
- **cpu/gpu columns**, none by this fix, each behind a ticket that already exists:
  - q27 cpu ×5: #163 (four `avg`s; the same reason tpch q1/q17 and 21 tpcds queries are
    out). gpu ×5: #152 (store_sales probes in more than one batch at four modes) and #183
    (`Utf8View` at the unload).
  - q72 cpu tp4-single/tp4-rowgroup/tp4-sized: #180 (`count(*)` under a shuffle, as q96/q90/
    q88). cpu tp1-single/tp1-rowgroup: decided by a measured run (§7) — enable if it fits,
    else a #192-style line. gpu ×5: #152 (two Left outers — no device path at all) and #183.
- q70, q86: nothing, ever, under #143's decision; their rows should say so.
- The end-to-end tier gains two inline cases at five modes (§3), which is the execution
  coverage the fix can honestly claim.

## 7. Risks and unknowns

- **Whether 46+ fixes q27 at all.** Only 45 is in the cargo cache; the ticket's "46+" is not
  something I could verify. The mechanism (§2) says the fix would have to be in
  `try_add_ordering`'s guard; the `bug_` test is what settles it at upgrade time.
- **q72 will not be fixed by any upgrade**, by reading: DataFusion defers to arrow's
  `add_wrapping`, and arrow's date arm has no integer case. Then `DateDayArithmetic` is a
  dialect rule, not scaffolding, and its `bug_` test stays green indefinitely — say so in
  the ticket rather than letting it read as a workaround.
- **q72's host memory.** DataFusion 45 orders joins by FROM clause (#20, `tickets.md:534`),
  so its first join is `catalog_sales ⋈ inventory ON cs_item_sk = inv_item_sk` with
  `inv_quantity_on_hand < cs_quantity` as a residual: 1.44M × ~1,300 inventory rows per
  item, roughly 10⁸–10⁹ pairs before the week-sequence and dimension filters that DuckDB
  applies first (its profile joins inventory last, 2,024 rows out). At tp1-single the probe
  side is one batch, so one call materializes the product — tens of GB, a q64-class
  failure (#192). At the rowgroup modes it streams in ~100 probe batches. Not verifiable by
  reading; the task must run tp1-rowgroup first and decide the cells from the measurement.
- **`ordering_satisfy` vs the sanity check** use the same requirement, so the rule is inert
  elsewhere — asserted by the unchanged goldens, not just argued.
- **`get_type` on the merged schema** can fail for shapes `TypeCoercion` handles with its
  own context (outer references in subqueries). The rewrite declines rather than errors in
  that case, so nothing that plans today stops planning; a `date + int` inside a correlated
  subquery would still be refused — no corpus query has one.
- **DuckDB's semantics for `date + int`** are days; verified from the committed q72 profile,
  not by running DuckDB.
- **cuDF `cast` Int32 ↔ TIMESTAMP_DAYS** on the column path: `cudf::cast` handles numeric ↔
  timestamp by my reading of the C++ side's use of it, but no test in `cpp/tests` exercises
  that pair. Moot for q72's device cells (#152/#183) and worth one gtest case if the shape
  ever reaches a device.

**What the upgrade itself costs this tree** (for the ticket's own path; from the code, not
from a build):

- `Cargo.toml`: `datafusion = "45"` → `"46"` (arrow stays 54.x; 47 moves to arrow 55) and
  `datafusion-comet-spark-expr = "0.6.0"` → the release pinned to 46 (`Cargo.toml` comment:
  0.10 forces 49; 0.6.0's own `Cargo.toml` pins `datafusion = "45.0.0"`, arrow 54.1).
  `create_murmur3_hashes(&[ArrayRef], &mut [u32])` is the one symbol used.
- `ParquetExec` is deprecated in 46 and the planner emits `DataSourceExec` instead: four
  sites downcast it — `planner/translator/nodes.rs:52-53,392`, `scan_mapping/mod.rs:15-25`,
  `parquet_meta.rs:11-32,142,152,276`, `rowgroup_prune.rs:29,94-136,137-175` — reading
  `base_config()` (`file_groups`, `projection`, `file_schema`, `limit`) and `predicate()`.
  All become `DataSourceExec::data_source()` → `FileScanConfig` → `ParquetSource::
  predicate()`. Roughly 60 lines across four files, plus `PruningPredicate` unchanged.
- The CPU relay constructs DataFusion nodes (`cpu_backend/join.rs:117,140,307,355`,
  `mod.rs:80,97,115,372`, `accumulate.rs:55,239`, `merge_m2.rs:71,79` with
  `StateFieldsArgs`/`AccumulatorArgs`, `single_node.rs:82` `PlanProperties::new`): 46 keeps
  these signatures; 47/48 change `LexOrdering`, `null_equals_null → NullEquality`,
  `return_type → return_field`, `state_fields → Vec<FieldRef>`. Stopping at 46 is the cheap
  step; each later major is another pass over ~15 sites.
- **Every golden is at risk, not just q27/q72's.** A major DataFusion bump moves optimizer
  output (projection/limit pushdown, join projections, coalesce fetch parking, union
  ordering itself) — plan goldens (10 files), `recipe-payloads.txt` (bytes and digests),
  `.cpu.txt`/`.cost.txt`/`.result.txt` per mode, and the `refused by datafusion:` text of
  q27 that embeds a plan dump with `ParquetExec` in it. The regen is mechanical; reviewing
  what moved is not, and the two CI images build the C++ side on both cuDF legs regardless.
- `#166` names #14406's fix as "in 46.0.0" and calls the upgrade "the experiment"; that is
  the one concrete reason to upgrade that this ticket does not supply.

## 8. Complexity

**S** for the code, **M** for the task: two new ~40-line files under `planner/`, two
`pub struct` lines and two `mod` lines in `planner/mod.rs`, ~10 lines in `lib.rs`, two
end-to-end cases, two `corpus_query!` lines, two registry rows (plus two rows' ticket
cleanup), ten golden sections regenerated across five plan files, and `tickets.md` edits to
#23 and #65. No frozen surface changes — no ABI symbol, no fbs, no wire byte, no
declared-schema contract; `recipe-payloads.txt` should not move. What makes it M rather
than S is the measured q72 run needed to set its tp1 cells honestly, and that the fix's
own value is plan coverage for two queries whose execution stays behind #163, #180 and
possibly #192.
