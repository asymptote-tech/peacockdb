
# Tickets related to DataFusion upgrade

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

**Fix proposed:** fix 9 of `reports/corpus-fixes.md`, two session rules in
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
