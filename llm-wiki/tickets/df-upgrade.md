
# Tickets related to DataFusion upgrade

<a id="t241"></a>
### #241 — DataFusion 49 plans `RightMark`, which the planner and the wire cannot express

An upgrade to DataFusion 49 or later refuses every mark join that `JoinSelection` swaps, on both
backends. On 45 those same queries plan and run.

DataFusion 45 has one mark type, `LeftMark`, and `JoinType::supports_swap` is false for it
(`datafusion-common-45/src/join_type.rs`). So a mark join keeps the outer query as its left
input, which our build side is: the outer query is hashed and held, the subquery side streams as
the probe, and the marks come out only at finish. DataFusion 49 adds `RightMark` and lets
`LeftMark` swap to it. When the subquery side is the smaller one, the outer query becomes the
streamed probe and each probe call can answer its own rows with their marks.

Nothing here knows `RightMark`. The translator maps DataFusion's `JoinType` one to one, the fbs
`JoinType` enum stops at `LeftMark`, the join session has no arm for it, and `planner/nulls.rs`
and the capability matrix list the mark type by name.

The fix is a probe-side mark beside `RightSemi`: the same `distinct_hash_join` matcher over the
build's distinct keys, emitting every probe row plus `mark` per call, with no finish. The
null-aware form reads its two facts from the build side, known before the first probe.

**Corpus queries:** the mark joins, `tpcds` q10, q35 and q45 — whichever of them `JoinSelection`
swaps on the upgraded version.

<a id="t23"></a>
### #23 — q27 and q72 do not plan on DataFusion 45, and an upgrade fixes neither

Two TPC-DS queries fail to physical-plan (`plan_status=fail`, every cell `na`): q27 is refused by
`SanityCheckPlan`, q72 by type coercion (`Cannot coerce arithmetic expression Date32 + Int64`).

q27 is a `UNION ALL` of three `GROUP BY`s over one CTE, not a ROLLUP. `SanityCheckPlan` refuses a
`SortPreservingMergeExec` over a `UnionExec` whose ordering dropped to `[]`: `try_add_ordering`'s
guard (`datafusion-physical-expr-45/src/equivalence/properties.rs`) skips the direct check once a
branch contributed a constant, and a branch whose sort keys are all constant adds no ordering. q72:
`TypeCoercion` defers `Date32 + Int64` to arrow's `add_wrapping`, whose date arm has no integer
case; no DataFusion or arrow version read adds one. q70 and q86 were listed here too: they are
`rank() OVER` window queries the translator refuses on any DataFusion (`nodes.rs`, #143 archived),
so they are out of scope; their rows keep `32 143`.

**Corpus queries:** `tpcds/q27` and `tpcds/q72`, plan cells off at all five modes.

**Fix proposed:** [fix 9 of `reports/corpus-fixes.md`](../reports/corpus-fixes.md#fix9), two session rules in
`lib.rs::build_session_state`, with no upgrade.
- `planner::MergeInputSort`, a `PhysicalOptimizerRule` inserted before `SanityCheckPlan`: under a
  `SortPreservingMergeExec` whose input does not satisfy its ordering, add a `SortExec` with the
  merge's fetch and `preserve_partitioning`. Inert wherever the sanity check passes. Insert it by
  replacing the list, `with_physical_optimizer_rules` over
  `base.state().physical_optimizers().to_vec()`; `with_physical_optimizer_rule` appends after the
  gate and does not work. The translator then emits `GpuMergeSortedPartitions → GpuSort →
  GpuUnion` unchanged.
- `planner::DateDayArithmetic`, a `FunctionRewrite` before `TypeCoercion`: `Date32 ± integer`
  becomes `CAST(CAST(d AS Int32) ± CAST(n AS Int32) AS Date32)`, days, DuckDB's meaning. Every
  piece crosses the wire. It avoids intervals because of #168.

Tests: a `bug_` test per rule on a plain session (a two-branch union with a selected constant;
`select date '2000-01-01' + 5`), red the day DataFusion no longer needs the rule; a planner test
and an end-to-end case each (`sum(n_nationkey)` per branch; `sum(l_quantity)` over
`l_receiptdate > l_shipdate + 5`). The q27 and q72 sections of the five tpcds `.plans.txt` turn
from refusals into trees; `recipe-payloads.txt` is unchanged. Next: q27's cpu cells need #199 at
the tp4 modes, its third branch being a keyless `avg` over a join. q72's cpu cells need a measured
run: its FROM-order first join (#20) is paid by the oracle too and may not fit a 15 GiB runner
at the tp1 modes. Device cells for both meet #152. An upgrade would retire at most one rule;
#166 is the one concrete reason for an upgrade this does not supply.

On the upgrade, whenever it comes:

View types on the bump. In 45 the parquet scan is the only unconditional source of `Utf8View`/
`BinaryView` (`schema_force_view_types`, default true); every coercion and string function
returns a view only for a view input, so turning the option off in `build_session_state` clears
[#183](../archive/archived-tickets.md#t183) end to end. Later releases add producers that do not go through
the scan — `map_varchar_to_utf8view` (SQL `VARCHAR`/`CAST` → `Utf8View`) at least — and Arrow's
`ListView`/`LargeListView` may gain a first producer. cuDF holds no view layout, so any that reaches
the wire is #183 again. After the bump: `grep -c 'Utf8View\|BinaryView\|ListView'` over
`testdata/goldens/*/*.plans.txt` must stay zero, and every new `datafusion.*view*` option is
read for its default.

<a id="t166"></a>
### #166 — physical planning drops a LIMIT interval, and the answer changes

DataFusion 45 loses a limit in two shapes, both measured against DuckDB 1.5.4 on the same sf1
parquet: the interval is absent from the physical plan, so both engines compute the same wrong answer.

A limit inside a `UNION ALL` branch survives only as an `AggregateExec … lim=[n]` early-stop hint,
which applies neither the offset nor the truncation: two branch limits holding 18 rows under an outer
`LIMIT 40 OFFSET 5` answer 40 where DuckDB answers 13, at tp1 and tp4 alike. The hint is why a golden
carrying it looks like coverage — it reads as a limit in plan text and is not one. Separately, at tp4
only, an outer limit above an aggregate drops the mid-plan limit below it and the aggregate then counts
its whole input. No corpus query has either shape, so nothing is wrong today; `nested-limits.sql` was
reshaped rather than canonized against it. Upstream
[#14406](https://github.com/apache/datafusion/issues/14406) is the same class — a global limit removed
above children that keep only a local one — and its fix landed after 45.0.0 and is in 46.0.0, so the
upgrade is the experiment; a residual after it would need the logical limit set compared to the physical.

A second limit shape rides on the upgrade too. `limit_interval` (`planner/translator/common.rs`)
turns a root `CoalesceBatchesExec` carrying a fetch into the unload's interval, and says it is
not reachable: at the root DataFusion 45 leaves a `GlobalLimitExec` and parks a coalesce's fetch
only below it. The arm stays, deliberately defensive, so one query cannot get two plan shapes
depending on where the fetch was parked; `nodes.rs` handles the same node mid-plan, which is
live. What it lacks is a test. From `reports/hacks-audit.md` §4.

**Fix proposed** for the coalesce arm: a translator test in `planner/translator/tests.rs`, beside
the hand-built `GlobalLimitExec` case that wraps a planned tree. Wrap a planned single-lane tree in
`CoalesceBatchesExec::new(plan, 8192).with_fetch(Some(5))` at the root, translate it, and assert
the unload carries `skip=0, fetch=5` and the coalesce leaves no node behind. Keep the arm. After
the upgrade, re-read whether any plan golden gains a root coalesce with a fetch, which would make
the arm live.

<a id="t228"></a>
### #228 — DataFusion 55's grouping-set id packs a duplicate ordinal the device does not make

From 55, DataFusion packs a duplicate ordinal above the key bits of `__grouping_id`, so a grouping
set listed twice keeps two groups. The device gives both copies the same id, and its merge folds
them into one.

`group_id_array` (`physical-plan/aggregates/mod.rs`) takes an `ordinal` and `max_ordinal`, and
`Aggregate::grouping_id_type(group_exprs, max_ordinal)` widens the type by the ordinal's bits.
`GROUPING()` masks the ordinal off, so its answer is unchanged. On 45 there is no ordinal, and
DataFusion folds the duplicate sets itself, so the engines agree today. Belongs to the upgrade.
Assumes #65's C++ id.

**Corpus queries:** none; no corpus query repeats a grouping set. Example, tpch, on 55:
`select l_returnflag, sum(l_quantity) from lineitem group by grouping sets ((l_returnflag),
(l_returnflag), ());` must answer each flag's row twice.

**Fix proposed:** with the upgrade. Each set's ordinal is known at plan time, so the wire carries
it: append an `ordinal` to `GroupingSetMask` (`gpu_plan.fbs`), written from DataFusion's
grouping sets. #65's `grouping_id_column` packs it above the key bits and takes the width from
the plan's declared type rather than from `nkeys`, since the width is no longer a function of
the key count alone. The payload golden regenerates for every rollup.

<a id="t247"></a>
### #247 — DataFusion 45 refuses or mis-answers seven subquery shapes
Each is DataFusion 45's own limit, so both engines refuse it (or, for the last, both answer the
same wrong thing), and the fix is an upgrade or a rewrite of ours, not executor work. Checked
against DataFusion 45's source by the chain-J review (2026-10-07); pbench tables.

| shape | example | DataFusion 45 |
|---|---|---|
| uncorrelated `EXISTS` / `NOT EXISTS` | `SELECT * FROM tiny WHERE EXISTS (SELECT 1 FROM sub)` | refused: `build_join` returns `None` without a correlation, and the physical planner refuses the `Exists` left in the filter |
| `IN` / `EXISTS` as a projected value | `SELECT f_k IN (SELECT s_y FROM sub) FROM fact` | refused: subqueries decorrelate only inside a filter |
| a tuple `IN` / `NOT IN` | `... WHERE (f_k, f_qty) NOT IN (SELECT s_y, s_z FROM sub)` | refused: "InSubquery should only return one column" |
| `ANY` / `ALL` over a subquery | `... WHERE f_k <> ALL (SELECT s_y FROM sub)` | not planned (inferred) |
| `LATERAL` | `SELECT * FROM tiny t, LATERAL (SELECT * FROM dim WHERE d_k = t.t_k) x` | refused (inferred: no lateral decorrelation) |
| a correlation under a `LIMIT`, union, sort or window | `... WHERE f_k IN (SELECT s_y FROM sub WHERE s_z = f_qty LIMIT 1)` | refused: `can_pull_up = false` (`decorrelate.rs:126-151`) |
| a scalar subquery returning several rows | `SELECT (SELECT s_y FROM sub) FROM tiny` | wrong: no single-row check, so both engines agree on an answer SQL forbids; DuckDB errors |

**Corpus queries:** none. Each line above is the test once an upgrade lands.


<a id="t257"></a>
### #257 — DataFusion 45 answers `(x IN (subquery)) IS NULL` as nothing at all
`SELECT f_id FROM fact WHERE (f_k IN (SELECT s_y FROM sub)) IS NULL` plans, in DataFusion 45, to a
bare `EmptyExec` — the whole query, both scans gone — and answers 0 rows. DuckDB 1.5.4 answers
14,998 over the same parquet, which is the right answer: `sub.s_y` holds NULLs, so the `IN` is
UNKNOWN for every `f_k` that matches nothing, and UNKNOWN `IS NULL` is true.

The loss is in logical optimization, in four steps traced through DataFusion 45's source. An
`InSubquery` inside a larger expression becomes a LeftMark join plus a reference to its `mark`
column (`decorrelate_predicate_subquery.rs:147`); the mark is declared **non-nullable**
(`logical_plan/builder.rs:1354`, and again at `joins/utils.rs:628`); `IsNull` over a non-nullable
expression folds to `false` (`simplify_expressions/expr_simplifier.rs:1545`); and `where false`
becomes an `EmptyRelation` that `propagate_empty_relation` then collapses, both scans with it, which
physical planning renders as `EmptyExec`. `Expr::InSubquery::nullable` delegating to the inner
expression (`expr_schema.rs:310`) is why the fold needs the mark join to happen first.

So the optimized logical plan is already an `EmptyRelation` — an earlier note here said the logical
plan still carried the filter and sent the reader to physical planning, which was read off the
*initial* plan. Measured on the committed pbench sf1 data at four partitions.

Why it is ours and not only theirs: DataFusion at `target_partitions = 1` is the corpus' cpu
oracle (`corpus.rs::assert_answer`), so a query this reaches would be checked against the wrong
answer and agree with it. Today the engine refuses the plan — `unsupported: plan node EmptyExec`,
[#155](joins.md#t155) — so no wrong answer is served; the moment that arm lands, this one starts
answering 0 rows to a user. The DuckDB oracle ([#235](corpus-coverage.md#t235)) is what caught it,
and is the only thing that could have.

**Corpus queries:** pbench's `in-is-null`. Its plan cells are disabled on `155`, the refusal it
actually meets, and the row carries this ticket and `250` beside it — `250` is the ticket it was
WRITTEN for (the `IN`'s NULL is read) and cannot demonstrate while the answer is empty.
