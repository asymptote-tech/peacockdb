# Review of 23-proposal.md

Read-only, at master 188c23ce. Every DataFusion cite below was opened in
`~/.cargo/registry/src/*/{datafusion-*,arrow-*}-45.0.0|54.x`; every repo cite in the tree.
Paths relative to `/media/data/peacockdb` unless they start with a crate name.

## 1. Verdict

**Needs changes.** All three load-bearing claims hold against the code — (a) q70/q86 are
window queries #143 refuses on any DataFusion, (b) q27 is lost in `calculate_union_binary`'s
constants guard and a pre-`SanityCheckPlan` rule that sorts a merge's input is provably inert
elsewhere, (c) `Date32 + Int64` has no coercion in DataFusion 45 or arrow 54 and a
`FunctionRewrite` runs before `TypeCoercion` with the schema it needs — but the q72 half is
under-specified where a developer meets it first (the execution goldens it authors and the
oracle's own cost on the same FROM-order plan), and several smaller statements are wrong or
over-specific. Nothing blocking.

## 2. Findings

1. **q72's enablement deliverables and the oracle's cost are not priced.** — important.
   §3 "Goldens, registry, corpus lines" lists only the five plan files. Enabling any cpu cell
   for q72 also authors `testdata/goldens/tpcds.sf1/<mode>-mini.cpu.txt`, `.cost.txt` and a
   `mini.result.txt` section (build-test.md, "Golden files" table; `tests/common/corpus.rs:
   339-375` writes all three), and `assert_answer` (`corpus.rs:149,168`) first runs **plain
   DataFusion at `target_partitions=1` on the same session** — the same FROM-order join
   (`catalog_sales ⋈ inventory` first; DataFusion 45 has no join enumerator, #20 at
   `tickets.md:532-537`). The committed DuckDB profile puts inventory at 11,745,000 rows over
   18,000 items — ~650 per item, not the "~1,300" of §7 — so ~9×10⁸ pairs pass through the
   residual before any dimension filter, in the oracle as much as in the engine. §7 prices
   the engine's memory only; the oracle's run time and memory on a 15 GiB CI runner
   (`dataset-matrix`) is the first thing the task will measure and should be named as a
   risk, with the fallback that the tp1 cells stay `disabled` under a #192-style line if the
   oracle itself does not fit. Correction: add the three execution goldens to §3 and §8, and
   the oracle cost to §7; keep the "measure tp1-rowgroup first" rule.

2. **The `count(*)` end-to-end case is justified by the wrong reason.** — minor.
   §3 avoids `count(*)` for the union case "(#180 at tp4)" and then uses it for the lineitem
   case because there is "no shuffle". #180's own text (`tasks/active-tickets.md:262-276`)
   says the merge under the final aggregate is what disagrees, and a keyless aggregate at
   tp4 still merges four lanes (`GpuAggregate → GpuMergePartitions → GpuAggregateBatches`;
   q96, the ticket's carrier, is a keyless `count(*)`). What actually keeps the lineitem case
   green is that every lane holds rows, as
   `a_two_key_group_by_over_many_rows_does_not_emit_a_group_twice`
   (`tests/test_cpu_end_to_end.rs:405-415`, a keyless `count(*)` at all five modes) already
   shows. Correction: say that, or sidestep it with `sum(l_quantity)` so the case cannot be
   confused with #180's shape.

3. **§5's q27 query is one branch bigger than minimal, and the trigger is over-specified.** —
   minor. `try_add_ordering` (`datafusion-physical-expr/src/equivalence/properties.rs:
   2253-2274`) is called with `lhs.constants()` — the accumulator's whole constant set, not
   the intersection — so the direct `ordering_satisfy` check is skipped whenever the
   *left-folded* side carries any constant, shared or not, and the ordering is lost whenever
   the next branch has no ordering of its own (every required key constant in it, so
   `add_sort_above` at `datafusion-physical-optimizer/src/utils.rs:39-45` retains nothing).
   Two branches reproduce it:
   `SELECT r, n, g FROM (SELECT n_regionkey r, n_nationkey n, 0 g FROM nation UNION ALL
   SELECT NULL, NULL, 1 FROM nation) t ORDER BY r, n LIMIT 10`. The three-branch form is
   the q27 shape and fine as the end-to-end case; the `bug_` test wants the two-branch one,
   and its name should say what the code does ("a union branch with no ordering beside an
   accumulated constant") rather than "a shared literal".

4. **`bug_datafusion_loses_a_union_ordering…` "plain `SessionContext::new()` at
   target_partitions=1"** — minor. `SessionContext::new()` takes the CPU count; the defect
   reproduces at any count (the tp4 goldens carry the same error), so either drop the
   qualifier or build the session with `SessionConfig::new().with_target_partitions(1)`.

5. **The asserted analyzed expression `CAST(CAST(d AS Int32) + Int32(5) AS Date32)`** —
   minor. After the analyzer the literal is `CAST(Int64(5) AS Int32)`; `Int32(5)` is
   `SimplifyExpressions`' doing, one stage later. Assert on the optimized plan, or on the
   unfolded form.

6. **Registry ticket cleanup is mis-stated.** — minor. Row 87 (q86) carries `23 32 65 143`
   and no `97` (`testdata/cost-registry.csv:87`); rows 71 (q70) and 73 (q72) do. #97 is
   archived (`archive/archived-tickets.md:427`). Correction: drop `97` from rows 71 and 73;
   row 87 loses only `23`, if the ticket stops naming it.

7. **q72's two LEFT OUTER JOINs will most likely reach the engine as `Right`, not `Left`.** —
   minor. `JoinSelection::should_swap_join_order` (`join_selection.rs:61-85`) swaps on
   estimated bytes, and q80's three left outers land as `join_type=Right` in
   `tp1-single.plans.txt`. The device blocker for a probe-preserved join is then #152's
   "probe-local types after one batch" plus #183 at the unload, not "no device path at all".
   Same cells off; different reason on the line.

8. **`architecture.md` should carry the two rules, not "if a human wants it".** — minor.
   No sentence is falsified (I checked Planning's "What DataFusion is reused for" and the
   wire section's coercion clause), but the session that every planning site builds now
   shapes DataFusion's own plan before translation, and a reader meeting an extra `SortExec`
   under a merge, or a `Date32` cast chain, has nowhere to look. One clause under Planning,
   in the same commit — the shared rule is that code and wiki move together.

9. **First-hour compile facts the proposal omits.** — minor. `PhysicalOptimizerRule: Debug`
   (`datafusion-physical-optimizer/src/optimizer.rs:48`) and `FunctionRewrite: Debug`
   (`datafusion-expr/src/expr_rewriter/mod.rs:46`) — both structs need `#[derive(Debug)]`.
   `SessionStateBuilder::build()` appends only the `with_physical_optimizer_rule` list
   (`session_state.rs:1474-1481`), so the replace-the-list call is the right one, as stated;
   simpler than `PhysicalOptimizer::new().rules` is `base.state().physical_optimizers()
   .to_vec()` (`:811`), which stays in step with whatever the base session carries.

10. **The exec-model corpus is not addressed.** — minor. The ticket row names q27/q72 as
    absent there; `scripts/exec_model/tests/plans_tpcds.py:1-7` scopes that corpus to
    queries the engine runs, by hand-lowering, and nothing checks it against the registry.
    Say it is out of scope (q27 runs nowhere under #163; q72's lowering is an 11-table
    join and a task of its own) rather than leaving the row's gap unanswered.

## 3. Claims verified

- **(a)** `rank() OVER` in `testdata/tpcds-queries/q70.sql:5` and `q86.sql:5`;
  `planner/translator/nodes.rs:159-163` refuses `WindowAggExec | BoundedWindowAggExec`
  naming #143; #143 is archived "Window support is not planned"
  (`archive/archived-tickets.md:290-301`); q36 renders exactly that refusal
  (`tp1-single.plans.txt:3652`). Their plan cells can only ever be `disabled`, never
  `enabled` — the registry mapping at `tests/test_plan_goldens.rs:696-704`.
- **(b) root cause**: the q27 refusal text (`tp1-single.plans.txt:2633-2635`) is an SPM
  `[i_item_id, s_state] fetch=100` over a `UnionExec` whose branches are `SortExec TopK
  [i_item_id, s_state]`, `SortExec TopK [i_item_id]` (`NULL as s_state`) and a bare
  projection of `NULL, NULL, 1` over a keyless aggregate — "Child-0 order: []". Traced
  through `calculate_union` / `calculate_union_binary` (`properties.rs:2112-2191`):
  branch 1+2 fold to `[i_item_id, s_state]` with constants `{g_state}` (the `find` at
  `:2126` matches by expr; 0 ≠ 1 only downgrades `across_partitions`); folding branch 3
  hits `constants.is_empty() && …` at `:2261` with a non-empty set, falls to
  `try_find_augmented_ordering` (`:2280-2305`), which walks branch 3's empty `oeq_class`
  and returns false; the ordering pops to empty. Without `g_state` the direct check runs
  and branch 3 satisfies it through its constants — the proposal's counterfactual holds.
  `projected_constants` (`:1027-1057`) is what makes a literal projection a constant;
  `add_sort_above` (`utils.rs:34-51`) is why branch 3 gets no sort. `SanityCheckPlan`
  (`sanity_checker.rs:131-149`) is last in the list (`optimizer.rs:143`) and asks
  `ordering_satisfy_requirement(LexRequirement::from(expr))`
  (`sort_preserving_merge.rs:230-232`), which is what `ordering_satisfy(&LexOrdering)`
  is (`properties.rs:531-535`) — so the rule fires iff the check would fail at that merge.
  `EnforceSorting`'s order of passes (`enforce_sorting/mod.rs:159-199`), the union arm of
  `pushdown_requirement_to_children` (`sort_pushdown.rs:244-249`) and the fetch propagation
  through `pushdown_sorts_helper` (`:85-140`) are as described.
- **(b) fix mechanics**: `SortExec::{new, with_fetch(&self), with_preserve_partitioning}`
  (`sort.rs:722-798`); `compute_properties` uses `with_reorder` (`sort.rs:851-855`,
  `properties.rs:435-462`), which drops constant keys and otherwise sets the ordering, so
  the new sort satisfies the merge. `SortPreservingMergeExec::with_new_children` keeps
  `fetch` (`sort_preserving_merge.rs:242-250`). `with_physical_optimizer_rules` replaces
  the list (`session_state.rs:1157-1163`); `with_physical_optimizer_rule` appends after the
  gate (`:1168-1175`, `:1474-1481`); `new_from_existing` copies the base's optimizer
  (`:1056`). The schema check between rules (`physical_planner.rs:1884-1893`,
  `:2030-2049`) passes: the sort's schema is the union's. Engine side: the SPM arm takes a
  `SortExec` child through `per_batch_sort` (`nodes.rs:224-243`, `:427-436`), the union
  declares `NotSpecified` order where branches disagree (`plan/union.rs:62-73`),
  `GpuSort` restores `BatchSorted` and `check_merge_keys` (`plan/common.rs:98-128`) is
  satisfied. A GpuAggregate directly above a GpuUnion is an existing shape at tp4
  (`tp4-single.plans.txt`, 27 sites), so an Exec over a union is not new to the estimator.
  The recipes shape is unchanged: `GpuSort: per batch: execute_node(#N CudfSort, batch)`
  and `GpuMergeSortedPartitions: at done: …all lanes` are the same lines whatever sits
  below (`call_shapes`, `test_plan_goldens.rs:450-495`), so the payload cover test does
  not move. `lanes_for` (`nodes.rs:372-388`) confirms §5's 4+4+4 at tp4-single and 1-lane
  `nation` under the other tp4 modes.
- **(c)**: `datafusion-expr-common/src/type_coercion/binary.rs:137-193` tries arrow's
  `add_wrapping` on empty arrays, then temporal, decimal and numeric coercion, then
  `plan_err!` with the golden's exact text; `arrow-arith/src/numeric.rs:233` routes
  `(Date32, _)` to `date_op` (`:645-700`), whose arms are `Date−Date`, `Interval*` and
  `Duration*` only. `FunctionRewrite` (`expr_rewriter/mod.rs:46-60`) is chained first in
  `Analyzer::execute_and_check` (`analyzer/mod.rs:146-166`) with the merged input schema
  (`function_rewrite.rs:42-77`); no default rewrite exists in 45 (`session_state.rs`
  registers none), so the pass is new and `Transformed::no` everywhere else;
  `NamePreserver::restore` aliases only when the name changed (`expr_rewriter/mod.rs:
  339-353`), so it is inert on untouched plans. `register_function_rewrite` is
  `FunctionRegistry` on `SessionState` (`session_state.rs:1876-1881`).
  `arrow-cast/src/cast/mod.rs:252-253, 1370, 1383` — `Int32 ↔ Date32` both directions as
  reinterpret. Engine: `translator/expr.rs:66-71` translates any `CastExpr`;
  `wire/serialize.rs:36-39` carries an `Int32` scalar and `:112-135` both types;
  `cpu_backend/expr_physical.rs:70-74` lowers `Expr::Cast` to `CastExpr`;
  `cpp/src/operators/join.cpp:353-367` evaluates a residual through `build_column`, whose
  cast arm (`cpp/src/expr.cpp:912-935`) is `cudf::cast` for non-string targets. q72 is the
  only `Date32 ± int` in either query set (grep); every other date offset there is a
  literal date or a tpch `interval`.
- Session sites: every planning path goes through `build_session_state` — plan goldens
  (`test_plan_goldens.rs:41`), corpus plan and oracle (`corpus.rs:45,149,168,332-338`),
  end-to-end oracle and engine (`test_cpu_end_to_end.rs:69,88`), recipe walk
  (`test_gpu_recipe_walk.rs:538`), join fixture, CLI (`peacockdb/src/main.rs:43`). The
  `SessionContext::new()` sites are `task_ctx()` / UDAF-registry lookups, not planners.
- Registry rows 28/71/73/87 as stated; `registry.rs:242-251` and
  `test_plan_goldens.rs:685-716` as stated; no `corpus_query!` line and no exec-model entry
  for the four; `hacks-audit.md` names none of them; `tickets.md:282` carries "(q70/q86
  after #23)"; #114 (archived) is the only other text tied to #23 landing.
- §4's `repartition_sorts=false` rejection: `enforce_sorting/mod.rs:166-176` skips
  `parallelize_sorts` under it, so every `SortPreservingMergeExec` that pass produced
  leaves the tp4 plans. Correctly rejected.

## 4. Corrected proposal (sections that change)

### 3. Localized fix — tests, goldens

- `bug_datafusion_loses_a_union_ordering_beside_an_accumulated_constant`: plain
  `SessionContext::new()` over `tpch.minimal`, the two-branch SQL of finding 3, asserts the
  error names `SanityCheckPlan`. Any partition count.
- `a_merge_whose_input_lost_its_order_gets_a_sort_beneath_it`: unchanged, over the same
  two-branch SQL through `build_session_state(1)`.
- `a_date_plus_an_integer_counts_days`: `SELECT DATE '2000-01-01' + 5 = DATE '2000-01-06'`
  is true; the **optimized** plan of `SELECT d + 5 FROM …` carries
  `CAST(CAST(d AS Int32) + Int32(5) AS Date32)`.
- End-to-end: the three-branch union with `sum(n_nationkey)` per branch (unchanged), and
  `SELECT sum(l_quantity) FROM lineitem WHERE l_receiptdate > l_shipdate + 5` — or keep
  `count(*)` with the comment that every lane holds rows, which is what keeps it clear of
  #180's shape.
- Both structs `#[derive(Debug)]`; the rule list from
  `base.state().physical_optimizers().to_vec()`.
- Goldens: the five plan files (q27, q72 sections) **and**, for every cpu mode q72 ends up
  enabled at, `<mode>-mini.cpu.txt`, `<mode>-mini.cost.txt` and the `mini.result.txt`
  section — authored by the corpus run under `UPDATE_CANONICAL=1`, never hand-written.
- Registry: rows 28 and 73 as proposed; `97` leaves rows 71 and 73; row 87 has none.
- `architecture.md`, Planning, one clause after the "What DataFusion is reused for" list:
  the session carries two engine rules — a physical rule that sorts a merge's input where
  DataFusion 45's union calculus loses the order, and an analyzer rewrite giving
  `date ± integer` the DuckDB meaning of days — both named in #23. In the same commit.
- Exec-model corpus: out of scope, and say so in the ticket.

### 5. Minimum corpus query

q27's defect, two branches (see finding 3). q72's as proposed.

### 6. Cells re-enabled

q72 gpu ×5: #183 at the unload for the string columns, and #152 once a probe side exceeds
one batch — the two outer joins arrive as `Right` after DataFusion's swap, as q80's do.

### 7. Risks and unknowns — add

- **The oracle pays the same join order.** `assert_answer` runs plain DataFusion at tp1 on
  the same session, so `catalog_sales ⋈ inventory` first (≈1.44M × 650) is the oracle's
  cost too; the measured run decides both whether the engine fits and whether the corpus
  tier can afford the oracle in CI. If neither, q72's cpu cells stay `disabled` under a
  #192-style line and the plan cells alone go `enabled`.

## 5. Complexity

**M**, agreeing with the proposal, for the reason it gives plus finding 1: the code is S,
and the task's size is set by the q72 measurement — which now includes an oracle run that
may not fit the CI runner — and the three execution goldens that measurement authors.
