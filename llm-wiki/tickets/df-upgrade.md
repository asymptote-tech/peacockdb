
# Tickets related to DataFusion upgrade

<a id="t23"></a>
### #23 — Umbrella: Upgrade DataFusion 45→46+ to unblock q27/q70/q72/q86
Four TPC-DS queries fail to physical-plan on DataFusion 45 (`plan_status=fail`): q27
SanityCheckPlan vs ROLLUP SortPreservingMerge ordering; q70/q86 `GROUPING()` aggregate
not planned; q72 `Date32 + Int64` coercion. Whole rows dead until the upgrade. See #114.

View types on the bump. In 45 the parquet scan is the only unconditional source of `Utf8View`/
`BinaryView` (`schema_force_view_types`, default true); every coercion and string function
returns a view only for a view input, so turning the option off in `build_session_state` clears
[#183](active-tickets.md#t183) end to end. Later releases add producers that do not go through
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
above children that keep only a local one — and its fix landed after 45.0.0 and is in 46.0.0, so #23's
upgrade is the experiment; a residual after it would need the logical limit set compared to the physical.
