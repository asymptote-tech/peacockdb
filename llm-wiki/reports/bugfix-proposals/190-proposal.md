# #190 — the CPU backend drops a nested-loop join's projection

Read at master c18e063a. Paths relative to `/media/data/peacockdb`. Nothing was built or run.

## 1. Issue

`CpuJoin::nested_loop` (`peacockdb-core/src/executor/cpu_backend/join.rs:123-151`) builds the
DataFusion operator with

    NestedLoopJoinExec::try_new(placeholder(build), placeholder(probe), Some(filter), &join_type, None)
                                                                                              ^^^^  :145

The last argument is DataFusion's output projection. The node carries one
(`GpuNestedLoopJoin.projection`, `plan/mod.rs:721-724`), the planner fills it from
`NestedLoopJoinExec::projection()` (`planner/translator/nodes.rs:511`), `check_projection`
validates it (`plan/join.rs:68-77`), the declared schema is DataFusion's *projected* schema
(`nodes.rs:512`, `Schema::new(join.schema())`), the recipe writer puts it on the wire
(`wire/join.rs:357-372`), and the C++ applies it (`cpp/src/operators/join.cpp:519-529`). Only
the CPU executor ignores it, so DataFusion answers with every column of `[build…, probe…]`
and `declared_as` (`cpu_backend/mod.rs:239-277`, reached via `declared()` at `join.rs:250`)
refuses on the column count: `the node declares Schema { … 2 fields } and DataFusion answered
with Schema { … 3 fields }`. A refusal, never a wrong answer — arrow's `RecordBatch::try_new`
checks the count before any value is read.

Which nodes are affected: the plan goldens hold a projecting `GpuNestedLoopJoin` in exactly
four queries, identical at all five modes — `tpch/q11` (1 node), `tpch/q22` (1), `tpcds/q24`
(1), `tpcds/q54` (2). The other three nested-loop shapes in the corpus
(`nested-loop-join`, `nested-loop-left-join`, tpcds q14 ×3) carry `projection: None`, for
which `None` is correct.

Cells disabled, correcting `00-tickets.md`:

- **Directly, today**: `tpch/q11` × 5 and `tpcds/q54` × 5 on the cpu, and so the gpu column
  too — `corpus_cases.inc:103-106,114` and `:212-213,219`; registry rows 111 and 55 carry
  `190`. That is what the enumeration says.
- **Behind #163, not in the enumeration**: `tpch/q22` × 5 (registry row 122, tickets
  `59 80 97 163`) and `tpcds/q24` × 5 (row 25, `45 163`) both hold a projecting nested-loop
  join. When #163 lands they hit this refusal next, and neither row names #190. So #190
  gates 20 cpu cells, not 10; fixing it first is what keeps #163's rollout from rediscovering
  it.

## 2. Root cause

One argument, traced:

1. DataFusion's physical optimizer embeds a projection into `NestedLoopJoinExec` when it
   cannot push it below the join — `try_swapping_with_projection`
   (`datafusion-physical-plan-45.0.0/src/joins/nested_loop_join.rs:566-598`) tries
   `try_pushdown_through_join` first, which fails whenever the join filter reads a column
   the projection drops (`projection.rs:815-855`, `update_join_filter`), then falls back to
   `try_embed_projection` (`projection.rs:381`). A `HAVING … > (scalar subquery)` is exactly
   that shape: the filter reads the build side's scalar and the output drops it. The embedded
   list is `collect_column_indices` — sorted, deduplicated ordinals into `left ++ right`.
2. The translator does not swap sides for a nested-loop join (`nodes.rs:493-494`:
   `build = join.left()`, `probe = join.right()`), so the plan's `projection` ordinals index
   the same `[build…, probe…]` table DataFusion's do, and `Schema::new(join.schema())` is the
   projected schema (`nested_loop_join.rs:288-295`, `project_schema(&schema, Some(projection))`).
3. `plan/validate.rs:200-207` (`declared_width`) checks the declared width against
   `projection.len()` — the plan is consistent.
4. `CpuJoin::nested_loop` hands DataFusion `None`, so `NestedLoopJoinExec` runs with
   `column_indices_after_projection = self.column_indices.clone()`
   (`nested_loop_join.rs:508-513`) and emits the full crossed row.
5. `probe_and_fetch` (`join.rs:236-251`) relabels the answer to the declared schema through
   `declared_as`; `RecordBatch::try_new(declared, columns)` refuses 3 columns against 2 fields.

The hash-join twin does it right forty lines up: `join.rs:303-313` maps `node.projection`
to `Vec<usize>` and passes it to `HashJoinExec::try_new`. One family applies the plan's
projection, the other drops it.

The C++ side is already correct and needs nothing: `execute_nested_loop_join` gathers
`[left…, right…]` (or masks the cross product) and then, `if (join->projection() &&
join->projection()->size() > 0)`, keeps the listed ordinals with their names
(`join.cpp:519-529`). The exec-model twin does the same (`scripts/exec_model/operators/
recipe.py:298,313`, `_project(…, node.projection)`).

## 3. Localized fix

**Yes, a localized fix exists: one argument in one function, plus a regression test and the
corpus bookkeeping.** No ABI symbol, no fbs field, no wire byte, no plan golden moves.

### 3a. `peacockdb-core/src/executor/cpu_backend/join.rs`

In `CpuJoin::nested_loop` (`:123-151`), replace the `None` at `:145` with the node's
projection, mapped exactly as the hash-join path does at `:303-306`:

```rust
    pub fn nested_loop(
        node: &GpuNestedLoopJoin,
        build: &ArrowSchema,
        probe: &ArrowSchema,
        ctx: Arc<TaskContext>,
    ) -> Result<Self, PlanError> {
        let filter = join_filter(&node.filter, &node.filter_columns, build, probe, ctx.as_ref())?;
        let join_type = match node.join_type {
            NestedLoopJoinType::Inner => JoinType::Inner,
            NestedLoopJoinType::Left => JoinType::Left,
        };
        // The plan's ordinals index `[build…, probe…]`, which is DataFusion's `left ++ right`
        // here too — the translator never swaps a nested-loop join's sides.
        let projection = node
            .projection
            .as_ref()
            .map(|columns| columns.iter().map(|column| *column as usize).collect());
        let join = NestedLoopJoinExec::try_new(
            placeholder(build),
            placeholder(probe),
            Some(filter),
            &join_type,
            projection,
        )
        .map_err(|error| PlanError::Invalid(format!("GpuNestedLoopJoin: {error}")))?;
        Ok(Self { calls: Self::one_call(Arc::new(join), node.kind(), ctx) })
    }
```

Optional and small: lift the three-line `as_ref().map(… as usize …)` into a private
`fn projection_of(projection: Option<&Vec<u32>>) -> Option<Vec<usize>>` and call it from both
`hash_join` (`:303-306`) and `nested_loop`. Two call sites, same mapping; either form is fine.
Nothing else in the file changes: `one_call` already takes the node's declared (projected)
schema as `output` (`:169`), so `declared()` at `:250` now sees matching widths and relabels
by position as it does for every other join.

Why this is the right layer: DataFusion applies the same ordinal list to the same
`left ++ right` table (`nested_loop_join.rs:508-513`), so the CPU's answer is the plan's
declaration by construction — the same argument the hash-join path already makes.
`with_new_children` (`:459-470`) carries the projection through the `StreamSourceExec`
substitution `execute_single_node` makes, so the placeholder swap does not lose it.

### 3b. Regression test — `peacockdb-core/src/executor/cpu_backend/tests/join.rs`

Red before the fix, green after. Sits beside
`a_nested_loop_inner_join_emits_the_pairs_its_predicate_keeps` (`:365`) and mirrors
`a_projecting_outer_join_pads_the_columns_its_projection_keeps` (`:266`):

```rust
/// The node's projection is the answer's shape, and the build key the predicate reads is
/// not in it: `[label, v]` keeps one column of each side and drops both `k` and `fk`. A
/// nested-loop join that emitted the crossed row whole would answer four columns against
/// two declared, which is what `declared_as` refused for tpch q11.
#[test]
fn a_nested_loop_join_emits_the_columns_its_projection_keeps() {
    let (filter, columns) = greater(0, 0);
    let output = [("label", DataType::Utf8), ("v", DataType::Int64)];
    let node = GpuNestedLoopJoin::new(
        side(&dim_columns(), BatchLayout::SingleBatch),
        side(&fact_columns(), BatchLayout::MultipleBatches),
        NestedLoopJoinType::Inner,
        filter,
        columns,
        Some(vec![1, 3]),
        schema_of(&output),
    );
    let join = CpuJoin::nested_loop(
        &node,
        &schema_of(&dim_columns()).fields,
        &schema_of(&fact_columns()).fields,
        ctx(),
    )
    .expect("the nested loop join builds");
    assert_eq!(drive(join), rows(&["c|20", "c|21"]));
}
```

Today this fails inside `drive` with `the node declares Schema { … 2 fields } and DataFusion
answered with Schema { … 4 fields }`. A Left variant (`Some(vec![1, 3])`, single-batch probe,
expecting `["a|-", "b|-", "c|20", "c|21"]`) is a cheap second case if wanted; one is enough
to pin the argument.

### 3c. Corpus and registry

- `peacockdb-core/tests/common/corpus_cases.inc:114`:
  `corpus_query!(tpch, 1, q11, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_exact, golden_exact);`
- `corpus_cases.inc:219`: the same five cpu modes for `tpcds, 1, q54`.
- Rewrite the two comments that name the ticket: `:103-106` (batch 6, "q11 is out at every
  mode … #190") and `:212-213` (batch 13, "q54's nested-loop join projects, so #190 drops
  it"). Whatever replaces them says what the device cells are off on (below).
- `testdata/cost-registry.csv:111` (tpch q11) and `:55` (tpcds q54): the five `cpu_*` cells
  `disabled → enabled`; the `tickets` cell drops `190` and must name what the still-disabled
  gpu cells are off on — `registry.rs:274-285` refuses a row with disabled cells and no
  ticket. By structure the four non-tp1-single gpu modes are #152: q11's Inner hash joins
  take partsupp in 7 probe batches at tp1-rowgroup with `build copy`
  (`tp1-rowgroup.plans.txt`, q11 section), and the tp4 modes likewise. The tp1-single cell
  is the unknown: every join in q11 and q54 sees one probe batch there, so #152 does not
  fire and the run may reach the unload. Decide it with one shad-gpu run
  (`PCK_TEST_FILTER=q11`/`q54` over `test_gpu_corpus` with `tp1_single` declared on the
  gpu side); if it passes, that is a bonus device cell each, else name the ticket it lands
  on. Fallback without a device: write `152`, keep the gpu column `none`, and say in the
  `corpus_cases.inc` comment that tp1-single is unverified.
- Execution goldens: the corpus cpu tier authors the sections under `UPDATE_CANONICAL=1`
  (build-test.md, "Golden files"): `== q11` in `testdata/goldens/tpch.sf1/<mode>-mini.cpu.txt`
  (5 files, today `skipped: not enabled at this mode`), the derived `<mode>-mini.cost.txt`
  (5), and `mini.result.txt` `== q11` (today `skipped: not enabled at any mode`; ~1048 rows
  at sf1, under the 256 KiB cap). The same eleven for `tpcds.sf1` / q54. `q11.duckdb_cost.txt`
  and `q54.duckdb_cost.txt` already exist. `<mode>.plans.txt` and `recipe-payloads.txt` do
  **not** move — the plan is unchanged; a diff there means something else changed.
- `llm-wiki/build-test.md:42` (Corpus, cpu): the query count and the disabled list, N 447 →
  457 (444 cells + 10); "Lib unit (Rust)" N 435 → 436 and the grand total 1569 → 1570 for the
  new unit test; `:44` unchanged unless a device cell comes on.
- `llm-wiki/tasks/active-tickets.md:197-221` → `llm-wiki/archive/archived-tickets.md` under
  Done, anchor kept (`<a id="t190">`), since the widget resolves the number to whichever file
  holds it.

### 3d. What it deliberately does not touch

- `declared_as` (`cpu_backend/mod.rs:239`). It is the guard that caught this; the fix makes
  the count agree rather than relaxing the check. hacks-audit §12 warns against a third
  reconciliation rule — this adds none.
- `CpuJoin::cross` (`join.rs:111-121`). `GpuCrossJoin.projection` can be `Some` only through
  the predicate-free `NestedLoopJoinExec` arm (`nodes.rs:486-489`), no golden shows one, and
  the wire's `CudfCrossJoin` has no projection field (`gpu_plan.fbs:440-443`) — so it is a
  different, latent, both-backend gap, not #190. See Risks.
- The wire, the C++, the exec model, the planner, `plan/join.rs`, the plan goldens.

### 3e. CPU and GPU stay one engine

Both now apply the node's `projection` ordinals to the `[build…, probe…]` table after the
predicate: DataFusion via `column_indices_after_projection` (`nested_loop_join.rs:508-513`),
cuDF via the gather loop at `join.cpp:519-529`, pandas/recipe via `_project`. The recipe
(`wire/join.rs:343-386`) and the payload bytes are untouched. One asymmetry to know and not
fix here: the C++ treats an empty projection vector as no projection (`size() > 0`), while
DataFusion would project to zero columns; DataFusion never embeds an empty list
(`try_embed_projection` returns `None` for it, `projection.rs:388`), so no plan reaches the
difference.

### 3f. hacks-audit scaffolding

None to remove. The audit excluded #190 by name (`hacks-audit.md:8-9`) and its second pass
found nothing grown around it. No production branch, field or fixture exists for this
ticket; the only records are the two `corpus_cases.inc` comments, two registry cells, and
mentions in task specs (`operator-cases.md:39`, `operator-cases-impl.md:1184-1188`,
`declared-schemas.md:216`, `declared-schemas-derived.md:36,100`). The operator-cases plan
expects `a_nested_loop_join_projects_the_same_columns` to land as a `bug_` form; if that
task merges first, the `bug_` test goes red with this fix and is deleted in the same change,
per coding-style's rule. The specs themselves are frozen and need no edit.

## 4. Alternatives rejected

- **A `ProjectionExec` over an unprojected `NestedLoopJoinExec` on the CPU** — two operators
  where DataFusion offers one argument, and it diverges from the hash-join path that already
  passes the projection in.
- **Emit a `GpuProject` above an unprojected nested-loop join in the translator** — moves the
  plan goldens for q11/q22/q24/q54 at all five modes plus the memory sections, the device
  already applies the projection correctly, and it is a whole-tree change for a one-argument bug.
- **Trim to the declared width inside `declared_as`** — hides the class of defect the guard
  exists for (a width mismatch is the one thing that catches a dropped or extra column) and
  adds a third produced-vs-declared rule hacks-audit §12 argues against.
- **Fix on the wire or in C++** — nothing is wrong there; the device applies the projection.

## 5. Minimum corpus query

```sql
SELECT b.n_name FROM region a, nation b WHERE a.r_regionkey < b.n_regionkey;
```

tpch sf1 (`testdata/tpch.sf1/region.parquet`, `nation.parquet`). Mode: `tp1-single` is
enough — both tables are one lane at every mode, so the plan is the same shape at all five.
Backend: cpu (the gpu half is untested by the ticket and the C++ already projects).

What happens today: it **plans** — the same shape
`a_non_equi_predicate_becomes_a_nested_loop_join` pins (`planner/translator/tests.rs:410`),
now with `projection=[n_name@1]` on the `GpuNestedLoopJoin` and `schema=[n_name:Utf8View]`,
because the filter reads `r_regionkey` and `n_regionkey`, neither of which the output keeps,
so DataFusion embeds the projection rather than pushing it down (`projection.rs:738-750`,
`join_allows_pushdown` fails on `far_right_left_col_ind >= 0`; even a two-sided
`SELECT a.r_name, b.n_name …` embeds, via `update_join_filter` returning `None`). It is
**refused at run time**, not at plan time: the first `probe_and_fetch`
(`cpu_backend/join.rs:236-251`) returns `BackendError` from `declared_as`
(`cpu_backend/mod.rs:271-276`): `the node declares Schema { fields: [n_name] … } and
DataFusion answered with Schema { fields: [r_regionkey, n_name, n_regionkey] … }: number of
columns(3) must match number of fields(1) in schema`. The driver fails the query at that node
and lane. After the fix it answers the nations whose region key exceeds some region's key.

Smaller than q11 (2 scans, 1 join, no aggregate) and than the corpus's own
`nested-loop-join.sql`, which is `SELECT *` and so never projects.

## 6. Cells re-enabled

- **Back on**: `tpch/q11` × {tp1-single, tp1-rowgroup, tp4-single, tp4-rowgroup, tp4-sized}
  cpu; `tpcds/q54` × the same five, cpu. Ten cells; registry rows 111 and 55.
- **Possibly on, decided by a device run**: `tpch/q11` and `tpcds/q54` at gpu tp1-single
  (one probe batch everywhere, so #152 is silent; whether the unload agrees on
  `Decimal128(38,2)` / the q54 Int64s is what the run says).
- **Stay off**: the other four gpu modes of both — #152 (`build copy` on a multi-batch probe).
  `tpch/q22` × 5 and `tpcds/q24` × 5 — #163 first; this fix removes the refusal they would
  hit next, and their registry rows should gain nothing (the `190` was never written on
  them) — but the #163 proposer should know these two are clear on this axis afterwards.

## 7. Risks and unknowns

- **Not run.** Whether q11 and q54 then match DataFusion exactly at all five modes is the
  corpus run's to say. Both compare `data_fusion_exact`; q11's `value` is a
  `Decimal128(38,2)` sum computed by DataFusion on both sides, q54 is `LIMIT 100` over ints.
  A second defect could sit behind this refusal — nothing in the plan text suggests one, and
  q54's two nested-loop joins are the only shape not already exercised elsewhere in the
  corpus.
- **The device cells.** The tp1-single gpu cell for each is genuinely unknown and needs a
  shad-gpu run to attribute; the registry cannot hold a disabled cell with no ticket.
- **The cross-join sibling.** `GpuCrossJoin.projection` (`plan/mod.rs:686-689`) is `Some`
  only via the predicate-free nested-loop arm (`nodes.rs:486-489`); the CPU
  (`CrossJoinExec::new` has no projection) and the wire (`CudfCrossJoin` has no field) both
  drop it. No golden reaches it. It is a separate, both-backend gap that would want a wire
  field or a planner-side `GpuProject`; the operator-cases plan's
  `a_cross_join_projects_the_same_columns` is where it will surface. Worth a ticket when it
  does, not this fix.
- **Nullability.** `RecordBatch::try_new` re-validates non-nullable declared fields against
  the produced nulls. A Left nested-loop join's probe columns are declared nullable by
  DataFusion's own `build_join_schema`, and the non-projecting Left form already passes, so
  the projected form should too — verified by reading, not by running.
- **Counts in `build-test.md`** are read off the registry and the N column; recheck them
  against `--list` after the run rather than trusting the arithmetic above.

## 8. Complexity

**S.** One argument in one function (`cpu_backend/join.rs`, ~6 lines net, optionally a
3-line shared helper), one ~25-line unit test, two `corpus_query!` lines and their comments,
two registry rows, and 22 golden files *authored* by the cpu corpus run (sections that today
read `skipped`), plus the `build-test.md` counts and the ticket's move to the archive. No
frozen surface changes — no C ABI symbol, no FlatBuffers field, no wire bytes, no
declared-schema contract; `<mode>.plans.txt` and `recipe-payloads.txt` stay byte-identical.
The only step above S in cost is the optional device run to attribute the tp1-single gpu
cells, and it costs a run rather than code.
