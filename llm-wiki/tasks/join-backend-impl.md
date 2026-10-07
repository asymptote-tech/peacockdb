# join-backend implementation plan

> **For agentic workers:** this chain is run by the ensemble coordinator; the developer works
> the tasks below in order, one commit (or a few) per task, and the reviewer gates each round.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** joins run on the C++ join session on the device and as long-lived DataFusion streams on
the cpu, with no recipe and no join refusal left; the `NOT IN` rewrite, #137's NULL-key filters
and the explicit `__rowmarker__` placeholder in the planner; every corpus and pbench cell whose last
ticket closes here turned on and compared with DuckDB and with the committed estimate — closing
#155, #152, #173, #212, #159, #59, #80, #137, #220, #190, #207, #208, and (C++ half already landed
in join-session-cpp) #136, #153, #160, #215, #63 and #154.

**Architecture:** planner pieces first, each additive and green on today's executors (tasks 1–3
and 5); then the executor traits and driver (task 6), the device onto the session with the wire's
`CudfJoin` (task 7), the cpu onto one DataFusion stream per lane (task 8); then `__rowmarker__`
(task 4, **run after task 8**: its placeholder is dropped through join projections, which the cpu
honours only from task 8 on); then the refusals lifted, now that both engines answer every shape
(task 9); then tests, goldens, exec_model, cells, cleanup and the record (tasks 10–16). Run order:
1, 2, 3, 5, 6, 7, 8, 4, 9, 10, …, 16 — the numbers are kept for the references between tasks.

**Every commit is green.** A golden that a code change moves is regenerated in that commit; a
`bug_` pin whose behaviour a commit changes is flipped in that commit (task 10 lists every pin,
and each task below says which of them it flips).

**Tech stack:** Rust (planner, wire, executors, driver), DataFusion 45 (an `OptimizerRule`, a
channel-fed `ExecutionPlan`), the join session's C ABI from join-session-cpp, the operator harness,
the corpus and pbench tiers, Python exec_model.

**Spec:** [`join-backend.md`](join-backend.md) — frozen. Design:
[`join-rewrite-design.md`](join-rewrite-design.md) §1.3, §3.5, §4, §5.7 (committed `9e563348`).
Estimate: [`../reports/join-rewrite-cell-estimate.md`](../reports/join-rewrite-cell-estimate.md).

## Global constraints

- Joins, and the planner pieces the design names. No change to non-join nodes' recipes. No
  broadcast (#140). No float key normalization (#243).
- Every `corpus_query!` line names its `duckdb_oracle` explicitly (duckdb-oracle's rule); a cell
  is enabled only with its DuckDB case green.
- A `bug_` pin flips to the positive case with the same script; it is not deleted.
- Device cycles are foreground (`scripts/build-test-shadgpu.sh`, never backgrounded); a build in
  a workspace uses that workspace's own `CARGO_TARGET_DIR`.
- Every golden a commit moves is regenerated in that commit (plan, cpu per-node, cost, payload),
  each kind reviewed as its own part of the diff; no commit leaves a golden stale or a tier red.
- Nothing of the old join path survives (spec step 9): each removal is checked by the grep
  quoted in task 15, and both builds are free of dead-code warnings.

## Review Focus

1. **A lane with no build batch under Right, Full or RightAnti** (pbench `sparse-build-*` at tp4):
   it owes every probe row (padded for Right/Full). Pinned in task 6 (driver) and task 10
   (harness `a_right_join_with_no_build_batch_pads_every_probe_row` and its Full/RightAnti twins).
2. **A zero-row probe batch between two with rows, every type:** one answer per call — a zero-row
   table for the per-probe types, nothing for LeftSemi/LeftAnti/LeftMark, on both engines. Task 8
   (`the_build_side_semi_family_answers_nothing_per_probe`) and task 10 (`*_with_a_zero_row_probe_between_two_with_rows_agrees`, nine types).
3. **NULL keys under `null_equals_null = true` for anti and mark** (set semantics), not only the SQL
   default. Task 10: `null_equals_null_is_honoured_by_a_left_anti_join` and the mark twin.
4. **A preserved-side residual that is NULL** (`NOT EXISTS (… AND d_kb)` with `d_kb` NULL keeps the
   row; the mark is `false`). Task 10: `a_left_anti_join_keeps_a_row_whose_preserved_condition_is_null`.
5. **A `NOT IN` whose operand the nullability trace cannot follow** (an expression, a column through
   an aggregate): the rule rewrites, conservatively, and the answer still equals DuckDB. Task 2:
   `not_in_over_an_untraceable_operand_is_rewritten`.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/planner/nullability.rs` (from `nulls.rs`) | `can_be_null` over the GpuNode tree (unchanged), `logical_can_be_null` over a DataFusion `LogicalPlan`, and the narrowed refusal: an IN or NOT IN off a WHERE's AND/OR spine that can meet a NULL (#250) |
| `peacockdb-core/src/planner/parquet_nulls.rs` (new) | one footer reader: does a column hold a NULL in any row group |
| `peacockdb-core/src/planner/not_in.rs` (new) | the `NOT IN` `OptimizerRule` |
| `peacockdb-core/src/lib.rs` | `build_session_state` registers the rule |
| `peacockdb-core/src/planner/translator/nodes.rs`, `common.rs` | #137's filter; the predicate-free nested loop; the placeholder; the lifts; the `EmptyExec` arm; the IS NOT DISTINCT FROM promotion |
| `peacockdb-core/src/plan/mod.rs` (`GpuEmpty`), `plan_text/node_text.rs`, `wire/attach.rs`, `executor/{cpu,gpu}_backend/backend.rs` | the empty leaf (task 9b) |
| `cpp/src/expr.cpp` | IS [NOT] DISTINCT FROM: AST `NULL_EQUAL`, column path `NULL_EQUALS` (task 9c) |
| `peacockdb-core/src/plan/join.rs`, `plan/mod.rs`, `plan/validate.rs` | the capability as two facts; `GpuNestedLoopJoin.join_type: JoinType`; no zero-column schema |
| `peacockdb-core/src/wire/join.rs`, `attach.rs`, `mod.rs`, `recipes.rs` | `CudfJoin`; the session recipe line |
| `peacockdb-core/src/executor/mod.rs` | the trait change |
| `peacockdb-core/src/executor/gpu_backend/{join.rs,mod.rs,backend.rs}` | the session typestate |
| `peacockdb-core/src/executor/cpu_backend/{join.rs,mod.rs,backend.rs,channel.rs (new)}` | the stream per lane |
| `peacockdb-core/src/executor/driver/{single_partition.rs,partitioned.rs,index.rs}`, `driver/tests/mock.rs` | `set_build(None)`, `owes_nothing`, scaffolding gone |
| `peacockdb-core/src/planner/memory_estimation.rs` | the session's pricing |
| `peacockdb-core/src/tests/**`, `planner/tests/**`, `plan/tests/**`, `executor/**/tests*`, `wire/tests.rs`, `wire/gpu_tests/mod.rs`, `tests/end_to_end/dimensions.rs`, `tests/injection.rs` | §5.7 |
| `cpp/src/operators/{join.cpp,dispatch.cpp,project.cpp}`, `cpp/src/node_session.cpp`, `flatbuffers/gpu_plan.fbs`, `cpp/tests/gpu/test_plan_executor.cpp` | the old paths removed; the four gtests onto the session |
| `scripts/exec_model/` | §4.6 |
| `testdata/goldens/**`, `testdata/goldens/recipe-payloads.txt`, `testdata/cost-registry.csv`, `peacockdb-core/tests/common/corpus_cases.inc` | goldens and cells |
| `llm-wiki/` | architecture, build-test, tickets, hacks-audit, the record |

---

### Task 1: One footer reader, and nullability on both trees

**Files:**
- Create: `peacockdb-core/src/planner/parquet_nulls.rs`
- Rename: `peacockdb-core/src/planner/nulls.rs` → `peacockdb-core/src/planner/nullability.rs` (the physical refusal stays in it until task 9 narrows it to task 2's logical one)
- Modify: `peacockdb-core/src/planner/mod.rs:10,108-114`, `planner/translator/scan_mapping/parquet_meta.rs:95-97`,
  `peacockdb-core/Cargo.toml` (`url = "2"`, already in the tree under DataFusion: a listing URL to a file path)
- Test: `peacockdb-core/src/planner/tests/null_analysis.rs` (moves with the module), a new `planner/tests/logical_nullability.rs`

**Interfaces:**
- Produces: `pub(crate) fn column_may_hold_null(path: &str, column: &str) -> Result<bool, PlanError>`
  (`parquet_nulls.rs`); `pub(crate) fn logical_can_be_null(plan: &LogicalPlan, column: &Column) -> bool`
  (`nullability.rs`); `can_be_null(node: &dyn GpuNode) -> Vec<bool>` unchanged.

- [ ] **Step 1: The failing tests.** In `planner/tests/logical_nullability.rs`, over the tpch sf1
  session (`build_session_state(4)` + `register_tables_for`, the way `planner/tests/join_capability.rs`
  builds its fixture):

```rust
#[tokio::test]
async fn a_key_whose_row_groups_hold_no_null_is_not_nullable() {
    let ctx = tpch_ctx().await;
    let plan = ctx.sql("SELECT o_custkey FROM orders").await.unwrap().into_optimized_plan().unwrap();
    assert!(!logical_can_be_null(&plan, &Column::from_name("o_custkey")));
}

#[tokio::test]
async fn a_column_through_an_aggregate_counts_as_nullable() {
    let ctx = tpch_ctx().await;
    let plan = ctx.sql("SELECT max(o_custkey) AS m FROM orders").await.unwrap()
        .into_optimized_plan().unwrap();
    assert!(logical_can_be_null(&plan, &Column::from_name("m")));
}

#[tokio::test]
async fn a_key_read_through_a_table_alias_is_traced() {
    // `FROM orders o` puts a SubqueryAlias over the scan; it must not end the trace.
    let ctx = tpch_ctx().await;
    let plan = ctx.sql("SELECT o.o_custkey FROM orders o").await.unwrap().into_optimized_plan().unwrap();
    assert!(!logical_can_be_null(&plan, &Column::from_name("o_custkey")));
}

#[tokio::test]
async fn an_outer_join_pads_and_so_nullable() {
    let ctx = tpch_ctx().await;
    let plan = ctx.sql("SELECT c_custkey, o_custkey FROM customer LEFT JOIN orders ON c_custkey = o_custkey")
        .await.unwrap().into_optimized_plan().unwrap();
    assert!(logical_can_be_null(&plan, &Column::from_name("o_custkey")));
    assert!(!logical_can_be_null(&plan, &Column::from_name("c_custkey")));
}
```

- [ ] **Step 2: Run red.** `cargo test -p peacockdb-core --lib --features rust-only logical_nullability` —
  fails: `logical_can_be_null` not found.
- [ ] **Step 3: The reader.** `parquet_nulls.rs` opens the file with `SerializedFileReader` (as
  `parquet_meta.rs:56-59` does), finds the column by name in the schema descriptor, and answers
  `true` if any row group's `statistics().and_then(|s| s.null_count_opt())` is `None` or `> 0` —
  the same rule as `parquet_meta.rs:95-97` ("no statistic is not a promise of no nulls").
  `parquet_meta.rs` keeps its per-group loop (it needs bytes too) but calls the same predicate
  helper `fn nulls_possible(stats: Option<&Statistics>) -> bool`, defined once in `parquet_nulls.rs`.
- [ ] **Step 4: The tracer.** In `nullability.rs`:

```rust
/// Whether `column` of `plan`'s output can hold a NULL, read off the data where the column
/// traces to a parquet scan. Anything it cannot follow is possibly-NULL: a false positive
/// costs a rewrite the query did not need, a false negative a wrong answer.
pub(crate) fn logical_can_be_null(plan: &LogicalPlan, column: &Column) -> bool {
    match plan {
        LogicalPlan::TableScan(scan) => scan_column_may_hold_null(scan, &column.name).unwrap_or(true),
        LogicalPlan::Projection(p) => match p.schema.index_of_column(column).ok().map(|i| &p.expr[i]) {
            Some(Expr::Column(inner)) => logical_can_be_null(&p.input, inner),
            Some(Expr::Alias(alias)) => match alias.expr.as_ref() {
                Expr::Column(inner) => logical_can_be_null(&p.input, inner),
                _ => true,
            },
            _ => true,
        },
        LogicalPlan::Filter(f) => logical_can_be_null(&f.input, column),
        LogicalPlan::Sort(s) => logical_can_be_null(&s.input, column),
        LogicalPlan::Limit(l) => logical_can_be_null(&l.input, column),
        // An alias renames the relation, not the column: the input answers for the bare name.
        LogicalPlan::SubqueryAlias(a) => logical_can_be_null(&a.input, &Column::from_name(column.name.clone())),
        LogicalPlan::Join(j) if j.join_type == JoinType::Inner => {
            let side = if j.left.schema().has_column(column) { &j.left } else { &j.right };
            logical_can_be_null(side, column)
        }
        _ => true,
    }
}

fn scan_column_may_hold_null(scan: &TableScan, name: &str) -> Result<bool, PlanError> {
    let source = source_as_provider(&scan.source).map_err(|e| PlanError::Invalid(e.to_string()))?;
    let listing = source.as_any().downcast_ref::<ListingTable>()
        .ok_or_else(|| PlanError::Unsupported("not a listing table".into()))?;
    let mut any = false;
    for table_url in listing.table_paths() {
        // `prefix()` is an object-store `Path` with no leading '/', not a filesystem path; the
        // URL (`file:///…`) is, through `Url::to_file_path`.
        let path = url::Url::parse(table_url.as_str())
            .ok()
            .and_then(|u| u.to_file_path().ok())
            .ok_or_else(|| PlanError::Unsupported(format!("not a local file: {}", table_url.as_str())))?;
        any |= column_may_hold_null(&path.to_string_lossy(), name)?;   // one file per table here
    }
    Ok(any)
}
```

  `source_as_provider` is `datafusion::datasource::source_as_provider`; a path is the
  `ListingTableUrl` the table was registered with (`lib.rs:77`), a local file. The first test is
  also the path's own: with `prefix()` the file never opens, `unwrap_or(true)` answers nullable,
  and it stays red. Re-export
  `can_be_null` from `nullability.rs` under the same name so the GpuNode callers do not move.
- [ ] **Step 5: Run green**, then `cargo test -p peacockdb-core --lib --features rust-only null_analysis`
  (the moved tests) green unchanged.
- [ ] **Step 6: Commit.** `git commit -m "nullability: one footer reader, and a tracer over the logical plan"`.

### Task 2: The `NOT IN` rule (#80)

**Files:**
- Create: `peacockdb-core/src/planner/not_in.rs`
- Modify: `peacockdb-core/src/lib.rs:39-55` (`build_session_state`), `peacockdb-core/src/planner/mod.rs` (module),
  `peacockdb-core/src/planner/nullability.rs` (`in_subquery_may_meet_null`, `refuse_nullable_in_off_the_spine`)
- Test: `peacockdb-core/src/planner/tests/not_in.rs` (new)

**Interfaces:**
- Consumes: `logical_can_be_null` (task 1).
- Produces: `pub(crate) struct NullAwareNotIn;` implementing `datafusion::optimizer::OptimizerRule`,
  named `"null_aware_not_in"`; `build_session_state` returns a context whose optimizer runs it
  immediately before `"decorrelate_predicate_subquery"`; in `nullability.rs`,
  `pub(crate) fn in_subquery_may_meet_null(x: &Expr, sub: &Subquery, outer: &LogicalPlan) -> bool`
  and the narrowed refusal `pub(crate) fn refuse_nullable_in_off_the_spine(e: &Expr, outer: &LogicalPlan) -> Result<()>`.

- [ ] **Step 1: The failing tests** — the scratch probe's four forms (`notin-probe`, measured
  2026-10-07: correlated 3 rows, uncorrelated 2, both under `OR` the same). The fixture registers
  two in-memory tables exactly as the probe did:

```rust
const SETUP: &[&str] = &[
    "create table o(w int, x int) as values (1,1),(1,2),(1,null),(2,1),(2,null),(3,5),(null,1),(4,null)",
    "create table s(z int, y int) as values (1,1),(1,3),(2,null),(2,7),(4,8)",
];

async fn rows(sql: &str) -> Vec<(Option<i32>, Option<i32>)> {
    let ctx = crate::build_session_state(1);
    for q in SETUP {
        ctx.sql(q).await.unwrap().collect().await.unwrap();
    }
    let batches = ctx.sql(sql).await.unwrap().collect().await.unwrap();
    let mut out = Vec::new();
    for b in &batches {
        let w = b.column(0).as_primitive::<Int32Type>();
        let x = b.column(1).as_primitive::<Int32Type>();
        for i in 0..b.num_rows() {
            out.push((w.is_valid(i).then(|| w.value(i)), x.is_valid(i).then(|| x.value(i))));
        }
    }
    out.sort(); // Option orders None first
    out
}

#[tokio::test]
async fn correlated_not_in_answers_as_sql() {
    assert_eq!(rows("select * from o where x not in (select y from s where s.z = o.w)").await,
               vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]);
}
#[tokio::test]
async fn uncorrelated_not_in_answers_as_sql() {
    assert_eq!(rows("select * from o where x not in (select y from s where z <> 2)").await,
               vec![(Some(1), Some(2)), (Some(3), Some(5))]);
}
#[tokio::test]
async fn correlated_not_in_under_or_answers_as_sql() {
    // no row has w = 99, so the OR's other arm decides alone: the same three rows
    assert_eq!(rows("select * from o where w = 99 or x not in (select y from s where s.z = o.w)").await,
               vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]);
}
#[tokio::test]
async fn uncorrelated_not_in_under_or_answers_as_sql() {
    assert_eq!(rows("select * from o where w = 99 or x not in (select y from s where z <> 2)").await,
               vec![(Some(1), Some(2)), (Some(3), Some(5))]);
}
async fn refusal(sql: &str) -> String {
    let ctx = crate::build_session_state(1);
    for q in SETUP {
        ctx.sql(q).await.unwrap().collect().await.unwrap();
    }
    match ctx.sql(sql).await {
        Err(e) => e.to_string(),
        Ok(df) => df.collect().await.expect_err("refused at planning").to_string(),
    }
}

// The spine is the AND/OR tree from a WHERE's root: there a row is kept on true alone, so NULL
// and false agree, and the two-valued rewrite is exact. Expected rows are DuckDB 1.5.4's over
// the same two tables (run 2026-10-07).
#[tokio::test]
async fn a_negated_positive_in_on_the_spine_answers_as_sql() {
    // NOT (x IN S) reaches DataFusion as Not(InSubquery{negated: false}); the rule normalizes it
    assert_eq!(rows("select * from o where not (x in (select y from s where z <> 2))").await,
               vec![(Some(1), Some(2)), (Some(3), Some(5))]);
}
#[tokio::test]
async fn a_negated_positive_in_under_or_answers_as_sql() {
    assert_eq!(rows("select * from o where w = 0 or not (x in (select y from s where s.z = o.w))").await,
               vec![(None, Some(1)), (Some(1), Some(2)), (Some(3), Some(5))]);
}
#[tokio::test]
async fn a_not_in_under_not_folds_to_a_positive_in() {
    // NNF: NOT (x NOT IN S) is x IN S, a semi join that needs no rewrite. DuckDB 1.5.4: these three.
    assert_eq!(rows("select * from o where not (x not in (select y from s where z <> 2))").await,
               vec![(None, Some(1)), (Some(1), Some(1)), (Some(2), Some(1))]);
}
#[tokio::test]
async fn de_morgan_reaches_a_not_in_under_a_negated_or() {
    // NOT (w = 1 OR x NOT IN S) is w <> 1 AND x IN S. S (z <> 2) = {1, 3, 8}. The row with w NULL
    // is dropped: NULL AND true is NULL. DuckDB 1.5.4 answers (2, 1) alone.
    assert_eq!(rows("select * from o where not (w = 1 or x not in (select y from s where z <> 2))").await,
               vec![(Some(2), Some(1))]);
}
#[tokio::test]
async fn bug_an_in_whose_null_is_read_is_refused() {   // #250
    // a mark join never yields NULL, so `(x IN S) IS NULL` cannot be answered by one
    let why = refusal("select * from o where (x in (select y from s)) is null").await;
    assert!(why.contains("#250"), "{why}");
}
#[tokio::test]
async fn a_not_in_nested_in_an_exists_is_rewritten_too() {
    // the rule walks subqueries' plans (transform_down_with_subqueries): the inner NOT IN's
    // operand s.y holds a NULL, so it is rewritten before DataFusion decorrelates the EXISTS
    assert_eq!(rows("select * from o where exists (select 1 from s where s.z = o.w and s.y not in (select o2.x from o o2 where o2.w = 3))").await,
               vec![(Some(1), None), (Some(1), Some(1)), (Some(1), Some(2)),
                    (Some(2), None), (Some(2), Some(1)), (Some(4), None)]);
}
#[tokio::test]
async fn an_uncorrelated_rewrite_over_an_unfiltered_subquery_counts_rows_itself() {
    // The rewrite's `(SELECT count(..) FROM S)` must not become count(*): DataFusion's
    // AggregateStatistics answers an unfiltered count(*) from statistics with a
    // PlaceholderRowExec, which the translator refuses (#158).
    let ctx = crate::build_session_state(1);
    for q in SETUP {
        ctx.sql(q).await.unwrap().collect().await.unwrap();
    }
    let plan = ctx.sql("select * from o where x not in (select y from s)").await.unwrap()
        .create_physical_plan().await.unwrap();
    let text = format!("{}", datafusion::physical_plan::displayable(plan.as_ref()).indent(true));
    assert!(!text.contains("PlaceholderRowExec"), "{text}");
}
#[tokio::test]
async fn not_in_over_keys_that_hold_no_null_is_left_alone() {
    // tpch anti-join: o_custkey and c_custkey hold no NULL in any row group (task 1's reader)
    let plan = tpch_plan("anti-join").await;
    assert!(!format!("{}", plan.display_indent()).contains("count("));
}
#[tokio::test]
async fn not_in_over_an_untraceable_operand_is_rewritten() {
    // Review Focus 5: x + 0 cannot be traced, so the rule rewrites, and the answer is SQL's
    assert_eq!(rows("select * from o where x + 0 not in (select y from s where z <> 2)").await,
               vec![(Some(1), Some(2)), (Some(3), Some(5))]);
}
```

  The expected rows are DuckDB's answers over the same tables (`scratchpad/notin-probe` for the
  first four; the rest run 2026-10-07 with DuckDB 1.5.4). If DataFusion 45 cannot plan the
  rewritten form inside the EXISTS (a scalar count subquery under a correlated subquery), the rule
  refuses a `NOT IN` that is not in the outermost query's WHERE instead, with `#250` (#80 closes
  here, so a refusal cannot cite it; #250's text gains the nested form), and
  `a_not_in_nested_in_an_exists_is_rewritten_too` becomes `bug_a_not_in_nested_in_an_exists_is_refused`, asserting `#250` —
  recorded in the detail file with the planner's error.
- [ ] **Step 1b: The pins for what stays refused.** One `bug_` test per ticketed refusal this
  chain leaves, each asserting today's refusal and citing its ticket:

```rust
// #247 — DataFusion 45's own limits: each is refused while planning, on both engines.
#[tokio::test]
async fn bug_datafusion_45_refuses_seven_subquery_shapes() {
    for sql in [
        "select * from o where exists (select 1 from s)",                                  // uncorrelated EXISTS
        "select x in (select y from s) from o",                                            // IN as a value
        "select * from o where (w, x) not in (select z, y from s)",                       // tuple NOT IN
        "select * from o where x <> all (select y from s)",                               // ALL
        "select * from o, lateral (select * from s where s.z = o.w) l",                   // LATERAL
        "select * from o where x in (select y from s where s.z = o.w limit 1)",           // correlation under LIMIT
    ] {
        assert!(refusal(sql).await.len() > 0, "{sql} planned: #247 may be fixed");
    }
    // the seventh: a scalar subquery of several rows answers instead of erroring
    assert!(!rows_any("select (select y from s) from o").await.is_empty(), "#247: no single-row check");
}
```
  (`rows_any` collects any batches without a fixed shape; `refusal` returns the plan error's text.)
  #246 is pinned in `nested_cases.rs`, beside the other nested-loop cases:

```rust
// #246 — a LIKE whose pattern is a column: the cpu answers, the device refuses at expr.cpp's guard.
operator_case! {
    GpuNestedLoopJoin,
    fn bug_a_like_against_a_column_pattern_is_refused_on_the_device() {
        let node = nested_with(NestedLoopJoinType::Inner, like_column_residual(), None);
        gpu_refuses_with(&run_both(&node, one_probe()), "LIKE pattern must be a string literal");
    }
}
```
  `like_column_residual()` builds `Expr::Like { expr: build "s", pattern: probe "s" }` with its
  `JoinFilterColumn`s, as `decimal_residual()` builds its cast (`nested_cases.rs:98`); the
  message is `expr.cpp:873-877`'s.
- [ ] **Step 2: Run red**: today's DataFusion answers 7 and 5 rows for the first two, keeps the
  NULL-keyed rows under the negated positive form, and plans the two off-spine forms.
- [ ] **Step 3: The rule, on the spine only.** `rewrite` visits every `LogicalPlan::Filter`, the
  ones inside subqueries included, and rewrites its predicate's **spine** — the AND/OR tree from the
  root — never below a `NOT`, an `IS NULL`, a comparison or a function:

```rust
impl OptimizerRule for NullAwareNotIn {
    fn name(&self) -> &str { "null_aware_not_in" }
    fn apply_order(&self) -> Option<ApplyOrder> { None }   // we walk, subqueries included
    fn supports_rewrite(&self) -> bool { true }
    fn rewrite(&self, plan: LogicalPlan, _: &dyn OptimizerConfig) -> Result<Transformed<LogicalPlan>> {
        plan.transform_down_with_subqueries(|node| match node {
            LogicalPlan::Filter(f) => {
                let input = Arc::clone(&f.input);
                let nnf = nnf(f.predicate.clone(), false);       // NOTs down to leaves; exact in 3VL
                let folded = nnf.transformed;
                let rewritten = spine(nnf.data, &input)?;
                let rewritten = if folded { Transformed::yes(rewritten.data) } else { rewritten };
                if !rewritten.transformed { return Ok(Transformed::no(LogicalPlan::Filter(f))); }
                Ok(Transformed::yes(LogicalPlan::Filter(Filter::try_new(rewritten.data, input)?)))
            }
            other => Ok(Transformed::no(other)),
        })
    }
}

/// Negation normal form: every NOT pushed to a leaf. De Morgan and double negation hold in SQL's
/// three-valued logic, so the predicate's meaning is unchanged; a NOT reaching an IN or EXISTS leaf
/// flips its `negated` flag. Afterwards only AND and OR sit above an IN that was on the spine.
fn nnf(e: Expr, negated: bool) -> Transformed<Expr> {
    let flip = |e: Expr| Transformed::yes(e);
    match e {
        Expr::Not(inner) => flip(nnf(*inner, !negated).data),
        Expr::BinaryExpr(BinaryExpr { left, op: op @ (Operator::And | Operator::Or), right }) => {
            let l = nnf(*left, negated);
            let r = nnf(*right, negated);
            let op = match (op, negated) { (Operator::And, true) => Operator::Or,
                                           (Operator::Or, true) => Operator::And, (op, _) => op };
            let changed = negated || l.transformed || r.transformed;
            let e = Expr::BinaryExpr(BinaryExpr::new(Box::new(l.data), op, Box::new(r.data)));
            if changed { Transformed::yes(e) } else { Transformed::no(e) }
        }
        Expr::InSubquery(s) if negated => flip(Expr::InSubquery(InSubquery { negated: !s.negated, ..s })),
        Expr::Exists(x) if negated => flip(Expr::Exists(Exists { negated: !x.negated, ..x })),
        leaf if negated => flip(Expr::Not(Box::new(leaf))),
        leaf => Transformed::no(leaf),
    }
}

/// The AND/OR spine: where a row is kept on true alone, so a NULL and a false agree.
fn spine(e: Expr, outer: &Arc<LogicalPlan>) -> Result<Transformed<Expr>> {
    match e {
        Expr::BinaryExpr(BinaryExpr { left, op: op @ (Operator::And | Operator::Or), right }) => {
            let l = spine(*left, outer)?;
            let r = spine(*right, outer)?;
            let changed = l.transformed || r.transformed;
            let e = Expr::BinaryExpr(BinaryExpr::new(Box::new(l.data), op, Box::new(r.data)));
            Ok(if changed { Transformed::yes(e) } else { Transformed::no(e) })
        }
        Expr::InSubquery(InSubquery { expr, subquery, negated: true })
            if in_subquery_may_meet_null(&expr, &subquery, outer) =>
        {
            Ok(Transformed::yes(rewrite_not_in(*expr, subquery, outer)?))
        }
        // A positive IN on the spine is left alone: a semi or mark join's false and SQL's NULL drop
        // a row alike there — never refused, nullable or not.
        leaf @ Expr::InSubquery(_) => Ok(Transformed::no(leaf)),
        // Anything else is a leaf of the spine: a nullable IN read inside it is off the spine (#250).
        other => {
            refuse_nullable_in_off_the_spine(&other, outer)?;
            Ok(Transformed::no(other))
        }
    }
}
```

  In `nullability.rs`, the analysis and the narrowed refusal (what is left of `nulls.rs`'s
  refusal once task 9 deletes the physical one):

```rust
/// Whether `x [NOT] IN (S)` can meet a NULL: x on the outer side, or S's one output column.
pub(crate) fn in_subquery_may_meet_null(x: &Expr, sub: &Subquery, outer: &LogicalPlan) -> bool {
    let x_null = match x {
        Expr::Column(c) => logical_can_be_null(outer, c),
        _ => true,                                     // an expression: possibly-NULL
    };
    let y = &sub.subquery.schema().columns()[0];
    x_null || logical_can_be_null(&sub.subquery, y)
}

/// An IN or NOT IN below a NOT, an IS [NOT] NULL, a comparison or a function reads three-valued
/// truth, and the joins DataFusion decorrelates it into answer two-valued (a mark is never NULL).
/// Where it can meet a NULL, it is refused (#250) rather than answered wrong.
pub(crate) fn refuse_nullable_in_off_the_spine(e: &Expr, outer: &LogicalPlan) -> Result<()> {
    let mut refused = None;
    e.apply(|sub| {
        if let Expr::InSubquery(InSubquery { expr, subquery, .. }) = sub {
            if in_subquery_may_meet_null(expr, subquery, outer) {
                refused = Some(sub.to_string());
                return Ok(TreeNodeRecursion::Stop);
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    match refused {
        Some(what) => plan_err!("an IN or NOT IN that can meet a NULL, read outside a WHERE's AND/OR \
                                 spine, is refused (#250): {what}"),
        None => Ok(()),
    }
}
```

  `rewrite_not_in` (`x` the outer operand, `y` the subquery's single output column):

```rust
fn rewrite_not_in(x: Expr, sub: Subquery, outer: &LogicalPlan) -> Result<Expr> {
    let y = Expr::Column(sub.subquery.schema().columns()[0].clone());
    let s = LogicalPlanBuilder::from(sub.subquery.as_ref().clone());
    let outer_x = to_outer_refs(&x, outer.schema())?;                // Column → OuterReferenceColumn
    let exists = |pred: Expr, plan: LogicalPlanBuilder| -> Result<Expr> {
        let p = plan.filter(pred)?.build()?;
        Ok(Expr::Exists(Exists { subquery: Subquery { outer_ref_columns: outer_columns(&p), subquery: Arc::new(p) }, negated: true }))
    };
    if !sub.outer_ref_columns.is_empty() {
        // correlated: NOT EXISTS (S AND y = x) AND NOT EXISTS (S AND y IS NULL) AND NOT EXISTS (S AND x IS NULL)
        Ok(exists(y.clone().eq(outer_x.clone()), s.clone())?
            .and(exists(y.clone().is_null(), s.clone())?)
            .and(exists(outer_x.is_null(), s)?))
    } else {
        // uncorrelated: DataFusion 45 cannot plan an uncorrelated NOT EXISTS, hence the counts
        // count(lit(1i32)), never count(*): DataFusion's count(*) is count(Int64(1))
        // (COUNT_STAR_EXPANSION), which AggregateStatistics answers from statistics over an
        // unfiltered S with a PlaceholderRowExec — refused by the translator (#158). An Int32
        // literal counts the same rows and is not recognized as count(*); the test
        // `an_uncorrelated_rewrite_over_an_unfiltered_subquery_counts_rows_itself` holds it.
        let count = |plan: LogicalPlanBuilder| -> Result<Expr> {
            let p = plan.aggregate(Vec::<Expr>::new(), vec![count(lit(1i32))])?.build()?;
            Ok(Expr::ScalarSubquery(Subquery { subquery: Arc::new(p), outer_ref_columns: vec![] }))
        };
        Ok(exists(y.clone().eq(outer_x), s.clone())?
            .and(count(s.clone().filter(y.is_null())?)?.eq(lit(0i64)))
            .and(x.is_not_null().or(count(s)?.eq(lit(0i64)))))
    }
}
```

  `to_outer_refs` maps every `Expr::Column` of `x` to `Expr::OuterReferenceColumn(type, column)`;
  `outer_columns(&p)` collects the plan's outer references (`LogicalPlan::all_out_ref_exprs`).
  `apply_order` is `None` and `rewrite` walks with `LogicalPlan::transform_down_with_subqueries`
  (DataFusion 45, `logical_plan/tree_node.rs:740`), so a filter under a join, and one inside an
  `EXISTS` or `IN` subquery's plan, is reached before `decorrelate_predicate_subquery` runs.
- [ ] **Step 4: Register it.** In `build_session_state`:

```rust
let mut rules = datafusion::optimizer::Optimizer::new().rules;
let at = rules.iter().position(|r| r.name() == "decorrelate_predicate_subquery")
    .expect("DataFusion 45 has decorrelate_predicate_subquery");
rules.insert(at, Arc::new(planner::NullAwareNotIn));
let state = SessionStateBuilder::new_from_existing(base.state())
    .with_config(config)
    .with_optimizer_rules(rules)
    .build();
```

- [ ] **Step 5: Run green** — the thirteen cases (fourteen with Step 1b's #247 pin); then `test_cpu_corpus` for tpch q16 and anti-join
  (their plans do not move: the reader finds no NULL), `cargo test --lib plan_goldens` unchanged.
- [ ] **Step 6: Commit.** `git commit -m "NOT IN answers as SQL: a logical rewrite, where the data can hold a NULL (#80)"`.

### Task 3: #137 — no NULL key crosses a shuffle it cannot match through

**Files:**
- Modify: `peacockdb-core/src/planner/translator/nodes.rs:246-301` (`hash_join`); `peacockdb-core/src/plan/mod.rs`
  (`GpuEmitPartitions::into_parts`, the `IntoAnyBox` supertrait of `GpuNode`)
- Test: `peacockdb-core/src/planner/tests/join_capability.rs` (new cases), pbench plan goldens

**Interfaces:**
- Consumes: `can_be_null` (task 1).
- Produces: `fn drop_null_keys_below_shuffle(side: Box<dyn GpuNode>, keys: &[u32]) -> Box<dyn GpuNode>`
  in `nodes.rs`; `GpuEmitPartitions::into_parts(self) -> (Box<dyn GpuNode>, Vec<u32>)`.

- [ ] **Step 1: The failing tests** (pbench is committed by then, chain task 3; the cases plan
  over `pbench.sf1` with pbench's own knobs — `MODES[2].knobs_for("pbench")`, tp4-single with the
  small-table rule off, so `fact` and `dim` really shuffle):

```rust
/// The plan text of `sql` over a dataset at tp4-single, with that dataset's knobs.
async fn text_at_tp4(dataset: &str, sql: &str) -> String {
    let mode = &crate::test_support::MODES[2];                    // tp4-single
    let ctx = crate::build_session_state(mode.target_partitions);
    let ctx = crate::register_tables_for(ctx, &crate::test_support::data_dir_for(dataset, "1")).await.unwrap();
    let physical = ctx.sql(sql).await.unwrap().create_physical_plan().await.unwrap();
    let root = crate::planner::plan(&physical, mode.knobs_for(dataset)).expect("plans");
    crate::plan_text::render(root.as_ref())
}

#[tokio::test]
async fn an_inner_join_drops_null_keys_below_both_shuffles_where_the_data_holds_them() {
    let plan = text_at_tp4("pbench", "SELECT f_id, d_id FROM fact JOIN dim ON f_k = d_k").await;
    assert_eq!(plan.matches("IS NOT NULL").count(), 2, "{plan}");
    assert!(plan.contains("f_k@") && plan.contains("d_k@"), "{plan}");
}
#[tokio::test]
async fn a_left_join_drops_null_keys_on_its_probe_side_only() {
    // Left preserves the build (dim): its NULL-key rows are owed padded, so only fact's are dropped
    let plan = text_at_tp4("pbench", "SELECT d_id, f_id FROM dim LEFT JOIN fact ON d_k = f_k").await;
    let filters: Vec<&str> = plan.lines().filter(|l| l.contains("IS NOT NULL")).collect();
    assert_eq!(filters.len(), 1, "{plan}");
    assert!(filters[0].contains("f_k@"), "{plan}");
}
#[tokio::test]
async fn a_full_join_drops_no_null_key() {
    let plan = text_at_tp4("pbench", "SELECT d_id, f_id FROM dim FULL JOIN fact ON d_k = f_k").await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}
#[tokio::test]
async fn a_tpch_join_gains_no_filter() {
    // o_custkey and c_custkey hold no NULL in any row group, so q3's shuffles stay as they are
    let sql = std::fs::read_to_string(crate::test_support::queries_dir_for("tpch").join("q3.sql")).unwrap();
    let plan = text_at_tp4("tpch", &sql).await;
    assert!(!plan.contains("IS NOT NULL"), "{plan}");
}
```

  (`data_dir_for`, `queries_dir_for`, `MODES` and `knobs_for` are test_support's — the last pbench's; if a
  helper's name differs on master when this runs, use the one the corpus tier calls.)

- [ ] **Step 2: Run red.**
- [ ] **Step 3: The sides per type** (design §4.1): a function in `plan/join.rs`

```rust
/// The sides whose unmatched rows are never emitted: there a NULL key can match nothing
/// under SQL equality, so it is dropped before the shuffle rather than skewing one lane (#137).
pub(crate) fn null_key_droppable(join_type: JoinType) -> (bool, bool) /* (build, probe) */ {
    match join_type {
        JoinType::Inner | JoinType::LeftSemi | JoinType::RightSemi => (true, true),
        JoinType::Left | JoinType::LeftAnti | JoinType::LeftMark => (false, true),
        JoinType::Right | JoinType::RightAnti => (true, false),
        JoinType::Full => (false, false),
    }
}
```

  The decision is made on the **translated** side, not on DataFusion's: a hash `RepartitionExec`
  always arrives wrapped in a `CoalesceBatchesExec` (`coalesce_batches.rs:57-80`), which the
  translator drops, so a side is shuffled exactly when its translated top is a
  `GpuEmitPartitions`. In `hash_join`, right after each side is translated and before the build's
  `GpuCoalesceAllBatches` is added, when `!join.null_equals_null()` and the side's flag is set:

```rust
/// #137: a NULL key matches nothing on a side whose unmatched rows are never emitted, so where the
/// data can hold one it is dropped before the shuffle instead of skewing the lane it hashes to.
fn drop_null_keys_below_shuffle(side: Box<dyn GpuNode>, keys: &[u32]) -> Box<dyn GpuNode> {
    if side.as_any().downcast_ref::<GpuEmitPartitions>().is_none() {
        return side;                                   // not shuffled here: nothing to skew
    }
    let n = lanes(side.as_ref());
    let emit = side.into_any_box().downcast::<GpuEmitPartitions>().expect("checked above");
    let (input, hash_keys) = emit.into_parts();
    let nullable = can_be_null(input.as_ref());
    let schema = input.kind().schema().cloned().expect("a shuffle's input declares a schema");
    let tests: Vec<Expr> = keys
        .iter()
        .filter(|k| nullable[**k as usize])
        .map(|k| Expr::unary(UnaryOp::IsNotNull,
                             Expr::column(*k, schema.fields.field(*k as usize).name())))
        .collect();
    let Some(predicate) = tests.into_iter().reduce(|a, b| Expr::binary(a, BinaryOp::And, b, DataType::Boolean)) else {
        return Box::new(GpuEmitPartitions::new(input, hash_keys, n));
    };
    let filtered = Box::new(GpuFilter::new(input, predicate, None, schema));
    Box::new(GpuEmitPartitions::new(filtered, hash_keys, n))
}
```

  The join's key ordinals index the side's output, which is the emit's input schema. In
  `plan/mod.rs`, so a node can be taken apart once it is known what it is:

```rust
/// `Box<dyn GpuNode>` → `Box<dyn Any>`, so a translated node known by `as_any` can be taken apart.
pub(crate) trait IntoAnyBox { fn into_any_box(self: Box<Self>) -> Box<dyn std::any::Any>; }
impl<T: std::any::Any> IntoAnyBox for T {
    fn into_any_box(self: Box<Self>) -> Box<dyn std::any::Any> { self }
}
// GpuNode gains the supertrait: `pub trait GpuNode: std::fmt::Debug + IntoAnyBox { … }`

impl GpuEmitPartitions {
    pub(crate) fn into_parts(self) -> (Box<dyn GpuNode>, Vec<u32>) { (self.input, self.hash_keys) }
}
```

  (`schema.fields.field(k).name()` is the `Schema` wrapper's Arrow field; if master spells the
  wrapper's accessor differently, use the one `new_filter`'s callers use.)
- [ ] **Step 4: Run green**; then regenerate every golden the filters move, **in the same
  commit** as the code, so no commit leaves a tier red: `UPDATE_CANONICAL=1 cargo test --lib
  plan_goldens` (about 73 tpcds plans gain the filter at the three tp4 modes), the cpu per-node
  goldens over the corpus cpu tier at the three tp4 modes (`UPDATE_CANONICAL=1 cargo test --test
  test_cpu_corpus -- tp4`), and the cost goldens (`UPDATE_CANONICAL=1 cargo test --test
  test_cost_model`). Review the diff: `GpuFilter … IS NOT NULL` lines, the counts and row figures
  under them, and nothing in `mini.result.txt`.
- [ ] **Step 5: Commit.** `git commit -m "#137: a NULL key is dropped before a shuffle where it can match nothing"`.

### Task 4: `__rowmarker__` is a column the plan declares (#63) — run after task 8

Run order: after task 8, before task 9. The placeholder is dropped through a join's projection;
the cpu applies a nested-loop or cross join's projection only from task 8 (#190, #207), so landing
this earlier turns tpcds q9's five cpu cells — on today — red until then.

**Files:**
- Modify: `peacockdb-core/src/planner/translator/nodes.rs` (the project arm, the scan arm),
  `peacockdb-core/src/plan/validate.rs`, `cpp/src/operators/project.cpp:20-31`,
  `peacockdb-core/src/executor/cpu_backend/mod.rs` (the scan's empty projection);
  `flatbuffers/gpu_plan.fbs` (`CudfScan.rows_only`), `peacockdb-core/src/wire/node_writer.rs` (the scan
  payload), `cpp/src/operators/scan.cpp`
- Test: `peacockdb-core/src/planner/translator/tests.rs`, `plan/tests/`, `tests/gpu_tests/nested_cases.rs`,
  `tests/gpu_tests/source_cases.rs`, `cpp/tests/gpu/test_plan_executor.cpp`

**Interfaces:**
- Produces: `pub(crate) const ROWMARKER: &str = "__rowmarker__";` in `plan/mod.rs`; plan validation
  refuses a zero-column schema with `PlanError::Invalid("… declares no column …")`.

- [ ] **Step 1: The failing tests.** (a) `translated_at_tp4("SELECT count(*) FROM lineitem", 0)`
  has a `GpuProject: exprs=[Int8(0) AS __rowmarker__]` and no `exprs=[]`; (b) tpch `nested-limits`'
  zero-column scan declares `__rowmarker__`; (c) validation refuses a hand-built `GpuProject` with
  no expression; (d) in `nested_cases.rs`, `a_cross_join_of_two_rowmarker_sides_keeps_one_placeholder`
  — both backends answer `|B|×|P|` rows of one `__rowmarker__` column.
- [ ] **Step 2: Run red.**
- [ ] **Step 3: The project arm.** Where DataFusion's `ProjectionExec` has no expression, plan
  `GpuProject::new(input, vec![NamedExpr { expr: Expr::Literal(ScalarValue::Int8(Some(0))), name: ROWMARKER.into() }], Schema::new(Arc::new(ArrowSchema::new(vec![Field::new(ROWMARKER, DataType::Int8, false)]))))`.
  Consumers: `count(1)` over it reads rows, not the column; a join above drops it through its
  projection (task 9 makes the projection apply on every type); a join whose kept columns would
  be none keeps one placeholder — its projection is `[ordinal of one placeholder]` and its schema
  `[__rowmarker__]`.
- [ ] **Step 4: The scan arm.** A `ParquetExec` projecting no column declares `[__rowmarker__]`.
  The wire says so explicitly: today an empty `projection` means *all* columns
  (`scan.cpp:45-53`), so the signal is a new field, appended to `CudfScan`:

```
  /// Read no column, only how many rows the chosen row groups hold, answered as one non-null
  /// Int8 column `__rowmarker__` of zeros (#63). `projection` is ignored when set.
  rows_only: bool = false;
```

  `node_writer.rs` sets it for a `GpuLoadParquet` whose declared schema is `[__rowmarker__]`. The
  device's arm, at the top of `execute_scan` after the paths are read:

```cpp
  if (scan->rows_only()) {
    auto meta = cudf::io::read_parquet_metadata(cudf::io::source_info{paths});
    auto const& groups = meta.rowgroup_metadata();     // one map per row group, "num_rows" in each
    int64_t rows = 0;
    if (rgs.empty()) {
      rows = meta.num_rows();
    } else {
      for (auto g : rgs) rows += groups.at(static_cast<std::size_t>(g)).at("num_rows");
    }
    if (scan->limit() > 0) rows = std::min<int64_t>(rows, static_cast<int64_t>(scan->limit()));
    cudf::numeric_scalar<int8_t> zero(0, true);
    std::vector<std::unique_ptr<cudf::column>> cols;
    cols.push_back(cudf::make_column_from_scalar(zero, static_cast<cudf::size_type>(rows)));
    return TableResult::owning(std::make_unique<cudf::table>(std::move(cols)), {"__rowmarker__"});
  }
```

  (`rgs` is computed first — move the row-group block above this arm; `read_parquet_metadata`
  and `rowgroup_metadata()` exist in 25.02 and 26.02, `cudf/io/parquet_metadata.hpp`.) The cpu's
  scan executor answers a batch of one Int8 zero column of the row count it read. Tests: a gtest
  in `test_plan_executor.cpp`, `ScanRowsOnlyAnswersTheRowCount` (the nation scan with `rows_only`,
  25 rows of one INT8 column named `__rowmarker__`, and with one row group chosen, that group's
  count); a harness case in `source_cases.rs`, `a_rows_only_scan_agrees_on_both_backends`.
  `recipe-payloads.txt` moves for the scans that set the field (tpch nested-limits'), regenerated
  in this task's commit (`PEACOCK_REWRITE_RECIPE_BYTES=1`).
- [ ] **Step 5: The refusal.** `project.cpp`'s empty-expression arm throws
  `"CudfProject with no expression: the plan declares __rowmarker__ (#63)"`; validation refuses a
  zero-column schema at every node.
- [ ] **Step 6: Run green**; in the same commit, every golden the placeholder moves: plan goldens
  (tpcds q38, q87, q88, q9, q90, q96: the 19 nodes; tpch nested-limits' scan), the cpu per-node and
  cost goldens of those queries, and the payload golden. tpcds q9's cpu cells and the nested-limits
  cpu cells stay green (the cpu applies the projections since task 8).
- [ ] **Step 7: Commit.** `git commit -m "#63: __rowmarker__ is a column the plan declares"`.

### Task 5: A predicate-free nested loop is a cross join only when Inner

**Files:**
- Modify: `peacockdb-core/src/planner/translator/nodes.rs:465-514`; `peacockdb-core/src/plan/mod.rs:672-737`
- Test: `planner/translator/tests.rs`

**Interfaces:**
- Produces: `GpuNestedLoopJoin.join_type: datafusion::common::JoinType` (replacing
  `NestedLoopJoinType`, deleted in task 15); `GpuNestedLoopJoin::new(build, probe, join_type, filter: Expr, filter_columns, projection, schema)`.

- [ ] **Step 1: The failing tests:** tpcds q9's 15 predicate-free `Left` joins plan as
  `GpuNestedLoopJoin{Left, filter=true}` (not `GpuCrossJoin`); `tiny LEFT JOIN empty ON true`
  (pbench `outer-on-true-empty`, planned `Right` by DataFusion) is refused with `#160` until task
  9 — a `bug_` pin, `bug_a_predicate_free_outer_nested_loop_is_refused` (#160), flipped in Task 9
  Step 1 to `a_predicate_free_outer_nested_loop_pads_its_preserved_side`; an Inner predicate-free
  nested loop is still `GpuCrossJoin`.
- [ ] **Step 2: Run red.**
- [ ] **Step 3:** In `nested_loop_join`, `let Some(filter) = join.filter() else { … }` becomes:
  Inner → `GpuCrossJoin` with **merged** probe (`merged(node(t, join.right())?)` — today's arm
  does not merge, design §4.1); any other type → the nested-loop arm with
  `predicate = Expr::Literal(ScalarValue::Boolean(Some(true)))` and `filter_columns = vec![]`.
  The type switch accepts every `JoinType` but still refuses non-Inner/Left with `#160` until task 9.
- [ ] **Step 4: Run green**; in the same commit, tpcds q9's plan, cpu per-node and cost goldens
  (its Left nested loops over `true`; the answer, `mini.result.txt`, unchanged). Both engines run a
  Left nested loop over `true` today (the device's AST path, the cpu's `NestedLoopJoinExec`), so q9's
  cells stay green.
- [ ] **Step 5: Commit.** `git commit -m "a predicate-free nested loop is a cross join only when Inner"`.

### Task 6: The join traits, the driver, the mock and the harness

**Files:**
- Modify: `peacockdb-core/src/executor/mod.rs:313-330`; `executor/driver/single_partition.rs:198-216,322-388`;
  `executor/driver/partitioned.rs:426-432`; `executor/driver/index.rs:34-43,129-153`;
  `executor/driver/tests/mock.rs:85-95,510-560`; `tests/gpu_tests/script.rs:265-281`; `tests/injection.rs:305-330`;
  `executor/gpu_backend/backend.rs` and `cpu_backend/backend.rs` (adapters for now)
- Test: `executor/driver/tests/flow.rs`, `driver/single_partition/tests.rs:300`, `driver/index/tests.rs:178`

**Interfaces:**
- Produces:

```rust
pub trait JoinExecutor<B: Backend>: Executor {
    type Probing: ProbingJoin<B>;
    /// `None`: the build side finished without a batch (#212) — the session answers it.
    fn set_build(self, batch: Option<B::Batch>) -> CallResult<Self::Probing>;
}
pub trait ProbingJoin<B: Backend>: Executor {
    /// At most one batch per call: the one-batch rule, in the type (#220).
    fn probe_and_fetch(&mut self, batch: B::Batch) -> CallResult<Option<B::Batch>>;
    fn finish_and_fetch(self) -> CallResult<Option<B::Batch>>;
    /// An empty build under Inner, Left, LeftSemi, LeftAnti, LeftMark or RightSemi: the lane
    /// owes nothing, so the driver drops its probe batches without a call.
    fn owes_nothing(&self) -> bool;
}
```

- [ ] **Step 1: The failing driver tests.** In `flow.rs`, replace the mock's
  `empty_build_owes_its_probe` with a `JoinRule.owes_nothing_when_empty: bool` the mock's
  `owes_nothing()` reads; rewrite :211, :235, :288, :307, :341, :371, :391 to the new calls:
  `a_lane_with_no_build_batch_calls_set_build_none` (counts `CallKind::SetBuild` with no input = 1,
  no `NoBuild` kind exists), `a_lane_that_owes_its_probe_side_probes_it` (owes_nothing false: every
  probe batch is a `Probe` call, then `Finish`), `a_lane_that_owes_nothing_drains_its_probe_side`
  (owes_nothing true: `DropProbe` per batch, no `Finish`), and every scatter output of zero rows is
  dropped (`a_zero_row_scatter_output_is_dropped_under_every_join_type`).
  `single_partition/tests.rs:300` asserts `LaneCall::SetBuild` for a build side done without a
  batch. `index/tests.rs:178` is deleted with `feeds_owing_build`.
- [ ] **Step 2: Run red** (`cargo test --lib --features rust-only executor::driver`).
- [ ] **Step 3: The driver.** `LaneCall::NoBuild` is removed; `select` answers `SetBuild` both when
  the build slot has a batch and when the build side is done without one; the `SetBuild` arm takes
  `input.map(|h| h.batch)`, and after the call moves to `LaneState::Draining` if
  `probing.owes_nothing()`, else `LaneState::Probe(probing)`. `Probe` and `Finish` wrap the
  `Option` into the outputs list: `hold_all(out.into_iter().collect(), …)`. `partitioned.rs:430`
  drops every zero-row scatter output (`if out.num_rows() == 0 { continue; }`); `index.rs`'s
  `feeds_owing_build` field and climb go.
- [ ] **Step 4: Adapters**, so both backends compile against the new traits before tasks 7 and 8
  rewrite them: gpu and cpu `set_build(None)` call today's `without_build` and, on `Ok`, return a
  probing join with `owes_nothing() == true`; `Vec` returns map to `Option` by **concatenating**:
  none → `None`, one → it, several → `concat_batches` over the output schema (the cpu's chunks; the
  device answers at most one already). That is #220's fix in miniature, so every cpu corpus join
  cell whose call yields more than `batch_size` rows stays green through tasks 6–7; task 8 replaces
  the adapter. No pin changes behaviour in this task.
- [ ] **Step 5: The harness.** `script.rs`'s join arm:

```rust
(NodeExecutors::Join(join), Script::Join { build, probe }) => {
    let (mut probing, _) = join.set_build(build.as_ref().map(|b| up(b)))?;
    if !probing.owes_nothing() {
        for batch in probe {
            let (out, _) = probing.probe_and_fetch(up(batch))?;
            slots.push(lower(out.into_iter().collect())?);
        }
    }
    let (out, _) = probing.finish_and_fetch()?;
    slots.push(lower(out.into_iter().collect())?);
}
```

  The `"a join with no build side is never probed"` assertion goes. `tests/injection.rs`'s
  wrapper follows the trait.
- [ ] **Step 6: Run green** — driver tests, `--lib` rust-only.
- [ ] **Step 7: Commit.** `git commit -m "join traits: set_build(Option), one batch per call, owes_nothing; the driver asks no more"`.

### Task 7: The device on the session, and `CudfJoin` on the wire

**Files:**
- Modify: `peacockdb-core/src/wire/join.rs` (all), `wire/attach.rs:95-97,330-341`, `wire/mod.rs:95-118,197-216`, `wire/recipes.rs:150-160`,
  `wire/serialize.rs:127-160` (`serialize_join_schema`), the `Writer` (`with_join_scratch`) and
  `planner/pipeline.rs` (passing the budget);
  `peacockdb-core/src/executor/gpu_backend/{join.rs,mod.rs:80-102,backend.rs:130-154,280-310}`;
  `peacockdb-core/src/planner/memory_estimation.rs:235-275`
- Test: `executor/gpu_backend/gpu_tests/join.rs` (rewritten), `wire/tests.rs` (12 join-recipe tests replaced)

**Interfaces:**
- Consumes: the four FFI symbols join-session-cpp declared in `peacockdb-ffi/src/lib.rs`:
  `peacock_join_build(exec, seq, build, *mut join, *mut PeacockNodeStats) -> i32`,
  `peacock_join_probe(exec, join, probe, *mut handle, *mut stats) -> i32`,
  `peacock_join_finish(exec, join, *mut handle, *mut stats) -> i32`, `peacock_join_release(exec, join)`.
- Produces: `FbKind::Join` (one kind for the three nodes); `CallPattern::{JoinBuild, PerProbeBatch, AtDone}`
  rendered `join_build(#N), per probe batch: join_probe, at done: join_finish`;
  `GpuJoin { site, seq, join_type, output }`, `GpuProbingJoin { join: u64 /*session id*/, … }`.

- [ ] **Step 1: The failing tests.** `wire/tests.rs`: `every_join_node_writes_one_cudf_join_leaf`
  (three cases: hash, nested loop, cross; the payload is `CudfJoin` with `build_schema`,
  `probe_schema`, `projection` and `null_equals_null` as the node says, and no child stub);
  `a_join_recipe_line_is_the_session` (the rendered line for Left: `join_build(#4), per probe batch:
  join_probe, at done: join_finish`; for Inner: no `at done`). `gpu_backend/gpu_tests/join.rs`:
  `a_session_probed_three_times_answers_each`, `dropping_a_probing_join_releases_its_session`
  (the executor's live-join count back to zero), `an_empty_build_under_inner_owes_nothing`,
  `a_build_preserving_finish_answers_once`.
- [ ] **Step 2: Run red.**
- [ ] **Step 3: The writer.** `wire/join.rs` keeps `wire_join_type`, `filter_column`, `column` and
  gains one writer for the three nodes:

```rust
pub(crate) fn join(node: JoinNode<'_>, inputs: &[&Schema], writer: &mut Writer) -> Result<Option<Recipe>, PlanError> {
    let [build, probe] = inputs else { unreachable!("a join declares two inputs") };
    let seq = writer.node(0, |b, _| cudf_join(b, &node, build, probe))?;   // a leaf: no child stubs
    let mut calls = vec![
        Call::join(seq, CallPattern::JoinBuild),
        Call::join(seq, CallPattern::PerProbeBatch),
    ];
    if needs_finish(node.join_type()) {
        calls.push(Call::join(seq, CallPattern::AtDone));
    }
    Ok(Some(Recipe::of(calls)))
}
```

  `cudf_join` writes `fb::CudfJoin` (join-session-cpp's table) with `build_schema`/`probe_schema`
  through `serialize.rs:120-160`'s schema writer (made `pub(super)`), keys as `JoinKey`
  ColumnRef pairs, the residual and its `filter_columns`, the projection (present-but-empty is
  written as an empty vector, absent as no field — design D15), and `null_equals_null`.
  `JoinNode` is an enum over `&GpuHashJoin | &GpuNestedLoopJoin | &GpuCrossJoin`.
- [ ] **Step 4: The executor.** `GpuJoin::set_build`:

```rust
pub(crate) fn set_build(self, build: Option<GpuBatch>) -> CallResult<GpuProbingJoin> {
    let mut calls = AbiCalls::armed(node_timing_on());
    let (handle, taken) = match build {
        Some(batch) => { let t = Consumed::of(&batch); (batch.consume().1, t) }
        None => (0, Consumed::default()),
    };
    let mut join = 0u64;
    let mut stats = PeacockNodeStats::default();
    let rc = unsafe { peacock_join_build(self.site.executor, self.seq as u64, handle, &mut join, &mut stats) };
    if rc != 0 { return Err(BackendError::new(format!("join_build(#{}) node {} lane {}: {}", self.seq, self.site.node, self.site.lane, last_error(self.site.executor)))); }
    calls.record(AbiCall { seq: self.seq, target: AbiTarget::Node(FbKind::Join), call_index: 0,
        in_rows: taken.rows, in_bytes: taken.bytes, out_rows: 0, out_bytes: 0 });
    let owes_nothing = taken.rows == 0 && empty_build_owes_nothing(self.join_type);
    Ok((GpuProbingJoin { join: self, session: join, build_rows: taken.rows, build_bytes: taken.bytes, owes_nothing }, CallStats { scratch_bytes: None, calls }))
}
```

  `probe_and_fetch` calls `peacock_join_probe` and returns `None` for handle 0, else
  `produced(…)` priced by the output schema; `finish_and_fetch` the same with `peacock_join_finish`;
  `impl Drop for GpuProbingJoin { fn drop(&mut self) { unsafe { peacock_join_release(self.join.site.executor, self.session) } } }`.
  `empty_build_owes_nothing(t)` is today's `empty_build_answers_nothing` renamed, in `plan/join.rs`,
  with a nested loop or cross reading as Inner/its own type. The `per_probe`/`at_done` lists,
  `make`, `copy_of`, `build_copy`, `finish_without_keys`, `JoinCall` go.
- [ ] **Step 5: Pricing** (design §4.3): `resident_bytes` = build bytes + 16 × build rows (the
  hash table) + build rows (the `matched` column) for the build-preserving types; `scratch_bytes`
  = `n_bytes` + the output estimate + 8 B per key-match pair (bounded by the build's rows × the
  batch's rows for a nested loop). `memory_estimation.rs`'s `Join` arm drops the accumulated-key
  term and adds the 16 B/row and the `matched` column;
  `memory_estimation/tests.rs:217` (`a_build_preserving_join_is_charged_for_the_keys_it_accumulates`)
  becomes `a_build_preserving_join_is_charged_for_its_matched_column`.
- [ ] **Step 5a: The schema writer refuses what it cannot name.** `CudfJoin`'s `build_schema` and
  `probe_schema` are what the session reads pad types and an absent side's types from, so a field
  the wire cannot type must not be written as `Null`. repartition-keys (Task 5c, #249) already
  made `serialize_schema` refuse an unmapped type in every schema, so `cudf_join` calls it with `?`
  and adds only the side to the message:

```rust
/// A join side's schema, every field typed — an unmapped type is a plan the device cannot run,
/// said at plan time ("not runnable" in the golden) instead of a run-time refusal on whichever
/// lane happens to have no build or no probe batch.
pub(super) fn serialize_join_schema<'a>(b: &mut FlatBufferBuilder<'a>, schema: &SchemaRef, side: &str)
    -> Result<WIPOffset<fb::Schema<'a>>, PlanError> {
    // repartition-keys' Task 5c made serialize_schema refuse an unmapped type (#249); this only
    // names the side
    serialize_schema(b, schema)
        .map_err(|e| PlanError::Unsupported(format!("a join's {side} side: {e}")))
}
```

  Timestamps map already: repartition-keys added `Timestamp(unit, _)` → `fb::DataType_Timestamp*`
  to `convert_data_type` (and `fb_text` renders them). Cases in `gpu_backend/gpu_tests/join.rs`:
  `a_left_join_with_a_timestamp_probe_column_finished_without_a_probe_batch_pads_it` (the `ts_us`
  pad's type is the probe schema's `Timestamp(Microsecond)`) and
  `a_right_join_whose_lane_got_no_build_batch_pads_a_timestamp_build_column`; in `wire/tests.rs`,
  `bug_a_join_side_holding_a_list_column_is_not_runnable` (#249; a `List<Int32>` probe column: the plan
  says "not runnable" naming the column).
- [ ] **Step 5b: The chunk budget.** `CudfJoin.chunk_bytes` (join-session-cpp's field; `0` reads
  as 1 GiB in C++) is written from the plan's budget, so a budgeted mode's joins stay inside it and
  the accountant prices what they hold. The pipeline passes the budget to the writer
  (`Writer::with_join_scratch(bytes)`), and `cudf_join` writes it:

```rust
/// A join call's pairs and cross-product scratch: a sixteenth of the budget, at most 1 GiB; 0 —
/// the session's 1 GiB default — when the mode sets no budget.
pub(crate) fn join_scratch_bytes(knobs: &PlanKnobs) -> u64 {
    match knobs.sizing {
        BatchSizing::Budgeted => (knobs.budget / 16).min(1 << 30),
        _ => 0,
    }
}
```

  Step 5's pricing bounds a join's per-call scratch by the same number, so the accountant and the
  session agree. Test (`wire/tests.rs`): `a_budgeted_join_carries_its_chunk_budget` (tp4-sized:
  `chunk_bytes == budget / 16`; tp1-single: `0`).
- [ ] **Step 6: Flip the pins this task changes, in this commit** (task 10 lists them all):
  the #152 pins (30: the device now answers every probe batch; the cpu always did) →
  `*_agrees` over the same script; the #173 pins (4: the device's finish over no probe batch
  answers) → `*_agrees`; the #59 pins (10: the session honours `null_equals_null` for anti and
  mark) → `run_both(&hash_join(t, false, filtered, None), script).same(Order::Any)`; the #215 pin
  at `nested_cases.rs:429` (the device answers a Left nested loop over a decimal predicate) →
  `*_agrees`; `nested_schema_cases.rs:21` (#207: the device holds the projected columns) → the
  positive schema case. Pins whose cpu half still refuses until task 8 keep that half only, in
  this commit: the three #212 pins (`*_with_no_build_batch_is_refused_on_both`) become
  `bug_*_with_no_build_batch_is_refused_on_the_cpu` (#212, until task 8; the device's rows asserted hand-counted, the cpu's
  `NO_BUILD` refusal asserted); `nested_cases.rs:438` keeps the cpu's `THREE_OF_SIXTEEN`;
  `nested_cases.rs:255` (#207, "dropped on both") keeps the cpu's `TWO_OF_SIXTEEN`.
- [ ] **Step 7: Run green on the device** — `scripts/build-test-shadgpu.sh` with
  `PCK_TEST_FILTER='gpu_backend::gpu_tests::join|join_cases|nested_cases|nested_schema_cases|join_dimension_cases'`,
  every case green. In this commit too, task 12 step 1's regeneration: the plan goldens' join
  recipe lines become the session's and `recipe-payloads.txt` gains `CudfJoin` payloads — the
  writer changes them here, so the goldens move here.
- [ ] **Step 8: Commit.** `git commit -m "the device's joins are sessions: CudfJoin, build once, probe per batch, finish once"`.

### Task 8: The cpu on one DataFusion stream per lane

**Files:**
- Create: `peacockdb-core/src/executor/cpu_backend/channel.rs`
- Modify: `peacockdb-core/src/executor/cpu_backend/join.rs` (all), `cpu_backend/mod.rs:83-92`, `cpu_backend/backend.rs:239-275`
- Test: `executor/cpu_backend/tests/join.rs` (rewritten)

**Interfaces:**
- Produces: `CpuJoin { plan: Arc<dyn Fn(Arc<dyn ExecutionPlan>, Arc<dyn ExecutionPlan>) -> Result<Arc<dyn ExecutionPlan>>>, join_type, output, ctx }`;
  `CpuProbingJoin { tx: Option<UnboundedSender<RecordBatch>>, stream: SendableRecordBatchStream, join_type, output, owes_nothing }`.

- [ ] **Step 1: The failing tests** (rust-only):
  `every_type_answers_at_most_one_batch_per_probe_call` (nine types, hash and nested loop, a
  probe batch producing more rows than `batch_size`: one batch out), 
  `the_build_side_semi_family_answers_nothing_per_probe` (Review Focus 2: `None`, not a zero-row
  batch, for LeftSemi/LeftAnti/LeftMark — DataFusion yields a zero-row chunk per probe batch for
  these, `joins/hash_join.rs:1543-1567`), `a_nested_loop_join_applies_its_projection` (#190),
  `a_cross_join_applies_its_projection` (#207), `a_cross_join_over_a_zero_row_build_answers_one_zero_row_batch`
  (#208), `set_build_none_runs_over_an_empty_build` (#212: Right pads every probe row).
- [ ] **Step 2: Run red.**
- [ ] **Step 3: The channel input** (`channel.rs`), the measured shape of `scratchpad/stream-probe`:

```rust
#[derive(Debug)]
pub(crate) struct ChannelExec {
    schema: SchemaRef,
    rx: Mutex<Option<mpsc::UnboundedReceiver<RecordBatch>>>,
    props: PlanProperties,
}
impl ChannelExec {
    pub(crate) fn new(schema: SchemaRef) -> (Self, mpsc::UnboundedSender<RecordBatch>) {
        let (tx, rx) = mpsc::unbounded();
        let props = PlanProperties::new(EquivalenceProperties::new(schema.clone()),
            Partitioning::UnknownPartitioning(1), EmissionType::Incremental, Boundedness::Bounded);
        (Self { schema, rx: Mutex::new(Some(rx)), props }, tx)
    }
}
impl ExecutionPlan for ChannelExec {
    fn name(&self) -> &str { "ChannelExec" }
    fn as_any(&self) -> &dyn Any { self }
    fn properties(&self) -> &PlanProperties { &self.props }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> { vec![] }
    fn with_new_children(self: Arc<Self>, _: Vec<Arc<dyn ExecutionPlan>>) -> Result<Arc<dyn ExecutionPlan>> { Ok(self) }
    fn execute(&self, _: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        let rx = self.rx.lock().unwrap().take().expect("a join's probe stream is executed once");
        Ok(Box::pin(RecordBatchStreamAdapter::new(self.schema.clone(), rx.map(Ok))))
    }
}
/// Every chunk the stream has ready. "Until Pending" drains one probe batch only because the
/// build is in memory and nothing is spawned beneath the stream — both by construction here.
pub(crate) fn drain(stream: &mut SendableRecordBatchStream) -> Result<(Vec<RecordBatch>, bool)> {
    let mut chunks = Vec::new();
    loop {
        match stream.next().now_or_never() {
            Some(Some(batch)) => chunks.push(batch?),
            Some(None) => return Ok((chunks, true)),
            None => return Ok((chunks, false)),
        }
    }
}
```

- [ ] **Step 4: The executor.** `set_build(build)`: `left = MemoryExec::try_new(&[vec![build.unwrap_or_else(|| RecordBatch::new_empty(build_schema))]], …)`;
  `(right, tx) = ChannelExec::new(probe_schema)`; the plan is `HashJoinExec::try_new(left, right, on, filter, &join_type, projection, PartitionMode::CollectLeft, null_equals_null)`,
  `NestedLoopJoinExec::try_new(left, right, Some(filter), &join_type, projection)` (the projection
  passed — #190), or `ProjectionExec::try_new(kept, Arc::new(CrossJoinExec::new(left, right)))` (#207);
  `stream = plan.execute(0, ctx)`. `probe_and_fetch(batch)`: `tx.unbounded_send(batch)`, `drain`,
  then `None` for the build-side semi family, else `Some(concat_batches(&output, &chunks)?)`
  (`RecordBatch::new_empty(output)` for no chunk — #208, #220). `finish_and_fetch`: drop `tx`,
  drain to the end, `None` for types with no finish and for an empty concat of a non-finishing
  type, else one concatenated batch, cast with `declared_as`. `owes_nothing` reads the type and
  the build's row count, as the device's does. `Calls`, `key_project`, `finish_join`,
  `finish_project`, `pad_project` go.
- [ ] **Step 5: Flip the pins this task changes, in this commit:** the #190 pins (4: the cpu
  applies a nested loop's projection), the #208 pins (2: one zero-row batch over a zero-row build),
  `nested_cases.rs:255` and `:438` (their cpu halves, left by task 7) and the three #212 pins
  (task 7's `bug_*_refused_on_the_cpu` → `*_with_no_build_batch_pads_every_probe_row`, both backends)
  → the positive cases over the same scripts. The `TWO_OF_SIXTEEN`, `THREE_OF_SIXTEEN`, `NO_BUILD`
  consts go with their last user.
- [ ] **Step 6: Run green** (rust-only); then the harness on the device
  (`PCK_TEST_FILTER='join_cases|nested_cases|join_dimension_cases'`), every case green; the cpu
  corpus tier's join cells green, their per-node goldens regenerated in this commit (one batch per
  call, none per probe for the build-side semi family — task 12's step 2 moves here).
- [ ] **Step 7: Commit.** `git commit -m "the cpu's joins are one DataFusion stream per lane, one batch per call (#220, #190, #207, #208)"`.

### Task 9: The refusals lifted

**Files:**
- Modify: `peacockdb-core/src/plan/join.rs:400-445` (capability), `:505-575` (validation), `:60-90` (nested loop validation);
  `planner/translator/nodes.rs:246-301,465-514`; `planner/pipeline.rs:75`, `planner/mod.rs:108-114`, `planner/nullability.rs` (the refusal)
- Test: `planner/tests/join_refusals.rs:16,30,52,64,76`, `join_capability.rs:128-182,309,376,448`, `translator/tests.rs:542`, `plan/tests/joins.rs:159,190,253`

**Interfaces:**
- Produces: `pub(crate) fn needs_finish(join_type: JoinType) -> bool` (Left, Full, LeftSemi,
  LeftAnti, LeftMark); `JoinCapability`, `capability`, `answers_in_one_call`, `per_call_join_type`,
  `finish_join_type` gone.

- [ ] **Step 1: Flip the tests.** `join_refusals.rs`' five: #153, #159 and #160's become
  plans-and-answers tests (`an_outer_join_with_a_residual_filter_plans_and_answers_as_datafusion`,
  … `_159`, `a_nested_loop_join_of_every_type_plans`), each comparing the cpu's answer with
  DataFusion's own collect over the same SQL. The two `_59_80` ones (`:52`, `:64`: a `NOT EXISTS`
  and a mark join over keys NULL on both sides) become
  `an_anti_join_over_null_keys_plans_with_not_exists_semantics` and its mark twin — the physical
  refusal goes because every type now honours `null_equals_null`, which is `NOT EXISTS`'s meaning;
  what `NOT IN` needs beyond that is task 2's rewrite, and what it cannot answer is task 2's
  narrowed refusal, which **stays** (the planner tests of task 2 hold it). Task 5's
  `bug_a_predicate_free_outer_nested_loop_is_refused` (#160) flips to
  `a_predicate_free_outer_nested_loop_pads_its_preserved_side` (8 padded rows over `empty`, as
  DuckDB answers). pbench's hand-off: `PBENCH_JOINS` (pbench's `plan_goldens` test) gains each
  query this task lifts from a refusal, with the join line its plan golden now shows.
  `join_capability.rs`'s matrix (`expected()` at :128) becomes nine rows
  of `(type, needs_finish)` × filtered/not, and :182 asserts no probe-side `GpuCoalesceAllBatches`
  anywhere; :309 and :376 assert planning; :448 asserts Right, Full, LeftSemi nested loops plan.
  `translator/tests.rs:542` and `plan/tests/joins.rs:159,190,253` follow (the probe-coalesce rule
  and the streaming-Left-nested-loop rule are gone).
- [ ] **Step 2: Run red.**
- [ ] **Step 3:** narrow the NULL-key refusal, do not drop it: the physical
  `refuse_null_unsafe_joins` (`nullability.rs`, `planner/mod.rs:108-114`, `pipeline.rs:75`) and
  `hardcodes_null_equality` go — they existed because the C++ hardcoded NULL = NULL for anti and
  mark, which the session no longer does — and `nullability.rs` keeps `can_be_null`,
  `logical_can_be_null`, `in_subquery_may_meet_null` and `refuse_nullable_in_off_the_spine` (task
  2's logical refusal of an IN or NOT IN read off a WHERE's AND/OR spine where it can meet a NULL); replace `capability` with `needs_finish`; in
  `hash_join` the probe is never coalesced; `nested_loop_join` accepts every type and never
  coalesces its probe (`GpuNestedLoopJoin`'s Left single-batch validation goes); the nested-loop
  `check_projection` reads the node's `JoinType`. The two unreachable refusals stay
  (`common.rs:94` mark column; `column_ordinal_of`'s bare-column key).
- [ ] **Step 4: Run green** (rust-only, then the harness on the device); plan goldens regenerate
  (probe-side coalesces disappear; nested loops beyond Inner/Left appear in pbench).
- [ ] **Step 5: Commit.** `git commit -m "every join shape plans: #153, #159, #160 lifted; the NULL-key refusal narrowed to IN off the spine"`.

### Task 9b: A side DataFusion folds to nothing — `GpuEmpty` (review row 8)

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs` (the node, `NodeRef`, `ExecutorCategory`, the name),
  `plan/validate.rs:220`, `plan_text/node_text.rs:67`, `planner/translator/nodes.rs:47-172` (the arm),
  `planner/nullability.rs` (`can_be_null`), `planner/memory_estimation.rs:156,185`,
  `wire/attach.rs:83` (no device call), `executor/cpu_backend/backend.rs:66`,
  `executor/gpu_backend/backend.rs:79`
- Test: `planner/translator/tests.rs`, `tests/gpu_tests/join_cases.rs`

**Interfaces:**
- Produces: `pub(crate) struct GpuEmpty { kind: NodeKind }` with `GpuEmpty::new(schema: Schema)`;
  `NodeRef::Empty(&GpuEmpty)`; `ExecutorCategory::Source` for it; plan text `GpuEmpty: lanes=1, schema=[…]`.

- [ ] **Step 1: The failing tests.**

```rust
#[tokio::test]
async fn an_empty_relation_plans_as_a_leaf_that_emits_nothing() {
    // DataFusion's PropagateEmptyRelation keeps the Left join over an EmptyExec.
    let plan = translated_at_tp4(
        "SELECT * FROM tiny t LEFT JOIN (SELECT * FROM dim WHERE false) d ON t.t_k = d.d_k",
        0,
    )
    .await;
    let text = plan_text(plan.as_ref());
    assert!(text.contains("GpuEmpty:"), "{text}");
    assert!(!text.contains("refused"), "{text}");
}
```

  and in `join_cases.rs`, both engines over a `GpuEmpty` build or probe:

```rust
operator_case! {
    GpuHashJoin,
    fn a_left_join_over_an_empty_relation_pads_every_build_row() {
        let empty = Box::new(GpuEmpty::new(Schema::new(prefixed(&synthetic(0, 1), "p_").schema())));
        let node = hash_join_over(JoinType::Left, build_given(), empty, false, None);
        let outcome = run_both(&node, script(Some(build_batch()), vec![]));
        outcome.same(Order::Any);
        assert_eq!(rows(outcome.cpu.as_ref().unwrap()), build_batch().num_rows());
    }
}
```
  (`hash_join_over`, `build_given`, `build_batch` and `rows` are `join_cases.rs`'s fixtures at
  `:21-190`; where one is named otherwise, use that one.)
- [ ] **Step 2: Run red** — `Unsupported("plan node EmptyExec")`.
- [ ] **Step 3: The node.** In `plan/mod.rs`:

```rust
/// A relation DataFusion proved empty (`EmptyExec`): one lane, no batch, on either engine.
#[derive(Debug)]
pub(crate) struct GpuEmpty {
    kind: NodeKind,
}

impl GpuEmpty {
    pub(crate) fn new(schema: Schema) -> Self {
        let layout = PartitionLayout {
            n: 1,
            batch_layout: BatchLayout::SingleBatch,
            key_distribution: KeyDistribution::NotSpecified,
            sort_order: SortOrder::NotSpecified,
        };
        Self { kind: NodeKind::Source { layout, schema } }
    }
}

impl GpuNode for GpuEmpty {
    fn kind(&self) -> &NodeKind { &self.kind }
    fn children(&self) -> Vec<&dyn GpuNode> { vec![] }
    fn validate_schemas_and_partitions(&self) -> Result<(), PlanError> { Ok(()) }
    fn as_any(&self) -> &dyn Any { self }
}
```
  `NodeRef::Empty(n)` beside `LoadParquet`; `ExecutorCategory::Source`; name `"GpuEmpty"`;
  `validate.rs:220`'s width arm `(schema width, "the columns it declares")`; `node_text.rs`
  renders `GpuEmpty: lanes=1, schema=[…]`; `can_be_null` answers `vec![false; width]` (no row, no
  NULL); `memory_estimation` prices 0 bytes and 0 rows; `wire/attach.rs` writes no node and no call
  (driver-routed, as `GpuMergePartitions`); both backends build a source whose stream ends at once
  (`CpuSource`/`GpuSource` over zero batches — the empty-script path the harness already drives).
  The translator arm, before the `Unsupported` fallthrough:

```rust
    if let Some(empty) = any.downcast_ref::<datafusion::physical_plan::empty::EmptyExec>() {
        return Ok(Box::new(GpuEmpty::new(Schema::new(empty.schema()))));
    }
```
- [ ] **Step 4: Run green** (rust-only, then the harness on the device); pbench's
  `empty-side-left-join` plan golden regenerates in this commit.
- [ ] **Step 5: Commit.** `git commit -m "an empty relation plans as GpuEmpty, a leaf that emits nothing"`.

### Task 9c: `IS [NOT] DISTINCT FROM` as a join condition (review row 9)

**Files:**
- Modify: `cpp/src/expr.cpp:103-127` (`fb_to_ast_op`'s caller in `build_expr`), `:481-500` (the
  column path's binary operators); `planner/translator/nodes.rs:465-514` (`nested_loop_join`)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`, `tests/gpu_tests/nested_cases.rs`,
  `tests/gpu_tests/exec_cases.rs`, `planner/translator/tests.rs`

- [ ] **Step 1: The failing tests.** Device: a `GpuFilter` over `i32 IS NOT DISTINCT FROM i64`
  and `IS DISTINCT FROM` on `synthetic` (NULLs on both sides at their strides) agrees with the cpu,
  on the AST path and with a non-AST operand forcing the column path:

```rust
operator_case! {
    GpuFilter,
    fn is_not_distinct_from_agrees_on_every_null_arrangement() {
        let pred = Expr::binary(Expr::cast(Expr::column(2, "i32"), DataType::Int64),
                                BinaryOp::IsNotDistinctFrom, Expr::column(3, "i64"), DataType::Boolean);
        run_both(&filter(input(), pred), Script::Exec(vec![synthetic(64, 3)])).same(Order::AsEmitted);
    }
}
operator_case! {
    GpuFilter,
    fn is_distinct_from_agrees_on_every_null_arrangement() {
        let pred = Expr::binary(Expr::cast(Expr::column(2, "i32"), DataType::Int64),
                                BinaryOp::IsDistinctFrom, Expr::column(3, "i64"), DataType::Boolean);
        run_both(&filter(input(), pred), Script::Exec(vec![synthetic(64, 3)])).same(Order::AsEmitted);
    }
}
```
  Planner: `SELECT * FROM dim d FULL JOIN tiny t ON d.d_k IS NOT DISTINCT FROM t.t_k` plans as
  `GpuHashJoin{Full, null_equals_null=true}`; with `ON d.d_k = t.t_k AND d.d_w IS NOT DISTINCT FROM
  t.t_v` the `=` stays the key, `null_equals_null=false`, and the IS NOT DISTINCT FROM conjunct is
  the residual.
- [ ] **Step 2: Run red** — the device throws `unsupported BinaryOp: <n>`; the planner plans a
  nested loop.
- [ ] **Step 3: The device.** In `build_expr`'s binary arm, before `fb_to_ast_op`:

```cpp
    if (op == fb::BinaryOp_IsNotDistinctFrom || op == fb::BinaryOp_IsDistinctFrom) {
      auto& eq = ctx.keep(std::make_unique<cudf::ast::operation>(
          cudf::ast::ast_operator::NULL_EQUAL, lhs, rhs));   // 25.02 and 26.02 both have it
      if (op == fb::BinaryOp_IsNotDistinctFrom) return eq;
      return ctx.keep(std::make_unique<cudf::ast::operation>(cudf::ast::ast_operator::NOT, eq));
    }
```
  and in the column path's operator map:

```cpp
    case fb::BinaryOp_IsNotDistinctFrom: return cudf::binary_operator::NULL_EQUALS;
    case fb::BinaryOp_IsDistinctFrom:    return cudf::binary_operator::NULL_NOT_EQUALS;
```
  (`ctx.keep` is `ExprContext::keep`, `expr.h:24-31`, which owns AST nodes for the expression's life.)
- [ ] **Step 4: The promotion.** In `nested_loop_join`, before the nested-loop arm:

```rust
    // DataFusion 45 extracts only `=` as a key; a keyless join on IS NOT DISTINCT FROM is a hash
    // join with null_equals_null, when every key pair would be such a conjunct (one flag for all).
    if let Some(filter) = join.filter() {
        let (pairs, rest) = split_indf_pairs(filter)?; // (Vec<(build_ord, probe_ord)>, Option<residual>)
        if !pairs.is_empty() {
            return hash_join_from_parts(t, join.left(), join.right(), *join.join_type(),
                                        pairs, rest, /* null_equals_null */ true,
                                        projected(join.projection()), Schema::new(join.schema()));
        }
    }
```
  `split_indf_pairs` walks the filter's top-level `AND`s, takes each `Column IS NOT DISTINCT FROM
  Column` whose two sides are one build and one probe column (by `filter.column_indices()`), and
  returns the rest re-joined with `AND` against the filter's schema; `hash_join_from_parts` is
  `hash_join`'s body (`nodes.rs:246-301`) taking keys, filter and the flag instead of reading a
  `HashJoinExec`, so the two arms share the co-partitioning, coalescing and #137 logic.
- [ ] **Step 5: Run green** (rust-only; the harness on the device); pbench's `indf-full-join` plan
  golden shows `GpuHashJoin{Full}` with `null_equals_null=true`, regenerated in this commit.
- [ ] **Step 6: Commit.** `git commit -m "IS NOT DISTINCT FROM: NULL_EQUAL on the device, a null-equal hash key in the plan"`.

### Task 10: The pins flip, and the cases the shapes need

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`, `join_dimension_cases.rs`, `nested_cases.rs`, `nested_schema_cases.rs`, `join_schema_cases.rs`;
  `tests/end_to_end/dimensions.rs:125-221`; `wire/gpu_tests/mod.rs:445,701,709,916,953,974,1020`

- [ ] **Step 1: Check the 56 pins are flipped** (#152 30, #59 10, #173 4, #190 4, #212 3, #207 2,
  #208 2, #215 2; `nested_cases.rs:438` counts for #190 and #215) — each in the commit that changed
  its behaviour (task 7 the device's, task 8 the cpu's), to the positive case with its script, e.g.
  `bug_a_left_join_refuses_its_first_probe_batch_on_the_device` →
  `a_left_join_over_one_probe_batch_agrees` (`run_both(&join(JoinType::Left), one_probe()).same(Order::Any)`).
  `git grep -nE 'fn bug_' -- peacockdb-core/src/tests/gpu_tests/{join,join_dimension,nested,nested_schema}_cases.rs`
  lists exactly the pins of tickets that stay open — #243's
  `bug_a_float_join_key_misses_negative_zero_and_nan_pairs_on_the_cpu` (`join_cases.rs`, from
  repartition-keys) and #246's `bug_a_like_against_a_column_pattern_is_refused_on_the_device`
  (`nested_cases.rs`, this task) — and no pin cites a ticket this task closes:
  `git grep -nB2 'fn bug_' -- peacockdb-core/src | grep -E '#(59|63|80|136|137|152|153|154|155|159|160|173|190|207|208|212|215|220)\b'`
  is empty; `BUILD_COPY`, `PROBE_COPY`, `NO_KEYS`, `NO_BUILD`, `INNER_ONLY`, `TWO_OF_SIXTEEN`,
  `THREE_OF_SIXTEEN` and `device_answers_as_if_null_equals_null` are gone. One #59 case gains a
  hand-counted assertion beside `same`. `crossing_projection`'s `Left | Full => unreachable!` arm
  gets real arms; `nested_with` gives Left a multi-batch probe.
- [ ] **Step 2: The new cases** (design §5.7, in-memory batches; `synthetic::key_types` from
  repartition-keys):
  - NULL keys on both sides for all nine types under `false` and `true` — Left and Full new:
    `a_left_join_pads_a_null_key_build_row_under_the_sql_default` (hand count: the build's NULL-key
    rows padded), Full the same plus the probe's;
  - `a_left_semi_join_over_many_to_many_keys_emits_each_build_row_once`;
  - `a_left_anti_join_keeps_a_row_whose_preserved_condition_is_null` and its mark twin (`false`);
  - each `key_types` column as an Inner and a LeftSemi key (Float with -0.0/NaN excepted: #243's
    pins own those);
  - per type, `*_with_no_build_batch_*` and `*_finishing_with_no_probe_batch_*`;
  - `a_predicate_free_left_nested_loop_over_an_empty_probe_pads_every_build_row`, and Right, Full;
  - `a_keyed_left_semi_join_with_a_decimal_residual_agrees` (the non-AST cross residual);
  - for every type, three probe batches with a zero-row one between (Review Focus 2).
- [ ] **Step 3: `dimensions.rs`** is rewritten: its helper no longer reads `feeds_owing_build`;
  `an_empty_build_lane_answers_like_the_oracle` asserts the lane reached `SetBuild` with no batch
  and answered as the oracle; its two drivers (tpcds q93 Right, tpch anti-join over an empty build)
  stay.
- [ ] **Step 4: `wire/gpu_tests/mod.rs`:** the join walker (`:445`) walks the session calls;
  `driven()` (`:974`) lists `FbKind::Join` as driven; `:709` (one `HashJoin{LeftSemi}` call) goes;
  `:916` and `:953` follow `FbKind::Join`; `SEMI_JOIN` (`:674`) stays as the device's regression
  for the non-AST semi residual.
- [ ] **Step 5: Run green** on the device: the harness, `--lib -- gpu_tests::`, `test_node_timing`.
- [ ] **Step 6: Commit.** `git commit -m "join cases for every pbench shape at node level"`.

### Task 11: The four plan-executor gtests onto the session

**Files:**
- Modify: `cpp/tests/gpu/test_plan_executor.cpp:375,427,609,1054`

- [ ] **Step 1:** `HashJoinNationRegion`, `HashJoinWithProjection`, `AggregateGroupBy`,
  `JoinProjectSort` build a `CudfJoin` leaf instead of `CudfHashJoin` and drive it through
  `peacock_join_build`/`_probe`/`_finish` (the helper `run_join(session, build, probe)` in the
  file), asserting the same 25/25/5/25 rows.
- [ ] **Step 2:** device cycle, the plan-executor target.
- [ ] **Step 3: Commit.** `git commit -m "plan-executor: the join gtests drive the session"`.

### Task 12: The goldens, checked current

Each regeneration below lands in the commit of the code that moves it (task 7 the recipe line,
task 8 the cpu goldens, tasks 3–5 their own); this task re-runs them and confirms no diff is left.

- [ ] **Step 1: The recipe line** (run in task 7's commit; here, re-run for no diff). `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 cargo test -p peacockdb-core --lib plan_goldens`
  (with the `/tmp` testdata symlink `build-test.md:646` names): every join's recipe line becomes
  the session's (75 per tpch mode, 656 per tpcds mode); `recipe-payloads.txt` loses its 134 join
  recipe lines and gains `CudfJoin` payloads. Review: the diff touches join recipe lines and
  payload bytes only. Commit `"goldens: a join's recipe is its session"`.
- [ ] **Step 2: Check** that the cpu per-node and cost goldens are current: task 8 regenerated
  them with the cpu's stream, tasks 3, 4 and 5 with their own changes. `UPDATE_CANONICAL=1` over
  the corpus cpu tier and `test_cost_model` produce no diff; if one does, it belongs to step 1's
  commit and lands there, with `mini.result.txt` byte-identical.

### Task 13: exec_model

**Files:**
- Delete: `scripts/exec_model/operators/recipe_join.py`
- Modify: `scripts/exec_model/operators/{nodes.py:551,568,586,join_types.py:82-121,recipe.py:392,cudf_calls.py}`, `node.py:109`, `tests/corpus.py:461`, `tests/test_join_capability.py`

- [ ] **Step 1:** `BACKENDS` (`test_join_capability.py:157`) keeps the pandas oracle only; the four
  recipe-only tests (:421, :438, :481, :516) go; :329, :349, :365, :623 flip to "answers as the
  oracle"; :527 (`…_is_wrong_today_which_is_153`) flips to the right answer; the guards :654 and
  :666 read the session's `cpp/src/operators/join_session.cpp` (its `fb::JoinType_` arms and its cuDF calls, which
  `cudf_calls.py` models: `hash_join`, `distinct_hash_join`, `mixed_left_semi_join`,
  `conditional_*`, `contains`, `gather`, `apply_boolean_mask`, `cross_join`).
- [ ] **Step 2:** `join_types.py`'s capability follows design §4.1 (two facts per type);
  `recipe.py`'s `copy_handle` goes.
- [ ] **Step 3:** `python -m pytest scripts/exec_model/tests -q` green.
- [ ] **Step 4: Commit.** `git commit -m "exec_model: the recipe join retires"`.

### Task 14: The cells, in batches

- [ ] **Step 1:** for each batch of about five queries (dataset by dataset, mode by mode, in the
  estimate's per-query table order, `reports/join-rewrite-cell-estimate.md` §7), set the
  `corpus_cases.inc` gpu modes (and cpu modes for #190's four and #212's q77 rows), run on
  shad-gpu (`PCK_TEST_FILTER` on the batch), compare with DuckDB (`duckdb_*` cases), enable the
  green, ticket the rest: tpch rollup_over_join gains `65`, tpcds q78 `60`, tpcds q32 at tp4
  `199`, as the estimate expects. The stale `183` rows' join cells are among the batches. A line
  whose first cells this batch turns on (tpch q11 and q22, tpcds q24 and q54, whose cpu cells #190
  kept off) moves its `duckdb_oracle` off `duckdb_none` in the same commit, to the variant its first
  DuckDB case passes under (tpch q11 → `duckdb_fingerprint`, being over the cap): `duckdb_none`
  over two sections that both exist fails.
- [ ] **Step 2:** pbench's join rows: enable their cells (the float rows stay commented out on
  #243), and each row this task first answers moves its `duckdb_oracle` off `duckdb_none` the same
  way; `test_cpu_corpus`'s registry test and `every_device_cell_has_a_cpu_cell_at_the_same_mode`
  green after each batch.
- [ ] **Step 2b: The collapse readings pbench left to this task.** `collapse-not-in` (pbench.md's
  collapse table, the uncorrelated `not-in-uncorrelated` after the rewrite): read its tp4 plan
  goldens and record in the detail file where `GpuMergePartitions` puts the outer side in one lane
  (the rewrite's joins against one-row counts), as pbench's detail file did for the other three.
- [ ] **Step 3:** strike each closed ticket from a row only when every off cell on it carries
  another (`registry.rs:229-240`).
- [ ] **Step 4: Commit** per batch: `"cells: <dataset> <queries> on (#152 …)"`.

### Task 15: Nothing of the old join path survives

- [ ] **Step 1: Remove** (spec step 9): `cpp/src/operators/join.cpp` and its `cpp/CMakeLists.txt` entry (the session lives in `join_session.cpp`, join-session-cpp); `wire/join.rs`'s old writers and `attach.rs`'s
  `cross_join`; `Input::{BuildSide, BuildSideCopy, BatchCopy, AccumulatedKeys}`,
  `ProjectRole::{ProbeKeys, NullPad, Narrow}`, `FbKind::{HashJoin, CrossJoin, NestedLoopJoin}` and
  their renderings (`recipes.rs:150-160`, `mod.rs:41-50`); `CudfHashJoin`, `CudfCrossJoin`,
  `CudfNestedLoopJoin` from `gpu_plan.fbs` and the generated code regenerated; `execute_hash_join`,
  `execute_cross_join`, `execute_nested_loop_join` and `dispatch.cpp:58-62`; the map arm's
  "multi-partition joins are not implemented yet" text (`node_session.cpp:584-596`);
  `NestedLoopJoinType`; `empty_build_answers_nothing` (renamed in task 7).
- [ ] **Step 2: The checks**, quoted in the PR, each empty:

```bash
git grep -nE 'BuildSideCopy|BatchCopy|AccumulatedKeys|Input::BuildSide\b|ProbeKeys|NullPad|ProjectRole::Narrow' -- peacockdb-core scripts
git grep -nE 'FbKind::(HashJoin|CrossJoin|NestedLoopJoin)|CudfHashJoin|CudfCrossJoin|CudfNestedLoopJoin' -- peacockdb-core cpp flatbuffers scripts
git grep -nE 'execute_(hash|cross|nested_loop)_join|without_build|feeds_owing_build|empty_build_owes_its_probe' -- peacockdb-core cpp scripts
git grep -nE 'copy_of|build_copy|finish_without_keys|answers_in_one_call|per_call_join_type|finish_join_type|refuse_null_unsafe_joins|copy_handle|recipe_join' -- peacockdb-core scripts
git grep -n 'multi-partition joins are not implemented' -- cpp
```

  and `cargo build -p peacockdb-core --features gpu 2>&1 | grep -c 'never used\|never constructed'`
  is 0, as is the C++ build's `-Wunused-function` count.
- [ ] **Step 3: The wiki:** `architecture.md`'s Joins section rewritten from the design (the
  session, the matchers, the capability as two facts, the `NOT IN` rewrite, #137, `__rowmarker__`);
  `hacks-audit.md`'s P1, P4, P8, P9, P10, P13 and findings 5, 11, 12 marked resolved with this PR;
  `build-test.md`'s rows and counts.
- [ ] **Step 4: Commit.** `git commit -m "the old join path removed: recipes, tables, executors, scaffolding"`.

### Task 16: The estimate, compared; the record

- [ ] **Step 1:** in the detail file, the estimate's per-query table (§7) against what turned on:
  each cell the estimate called FLIP and stayed off, and each it did not call FLIP and turned on,
  gets one line with its cause; each NEXT ISSUE confirmed or not; the eight checkable statements
  (§8) answered true or false.
- [ ] **Step 2:** the closed tickets archived (`archive/archived-tickets.md`) — #155, #152, #173,
  #212, #159, #59, #80, #137, #220, #190, #207, #208, #136, #153, #160, #215, #63, #154; any ticket
  a cell met filed with its query and mode.
- [ ] **Step 3: Commit.** `git commit -m "join-backend: the estimate compared, the record"`.
