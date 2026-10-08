
# Complete coverage

Tickets for MVP SQL functionality milestone

<a id="t239"></a>
### #239 — `date_part` could declare cuDF's `Int16` in DataFusion instead of casting on the device
DataFusion 45 types `date_part`/`extract` as `Int32`; cuDF's `extract_datetime_component`
answers `INT16` for every field. Since [#191](../archive/archived-tickets.md#t191) the device arm in
`build_column_scalar_fn` (`expr.cpp`) casts the component to the wire's `return_type`, one
extra column per extract.

The alternative: register our own `date_part` `ScalarUDF` in DataFusion's function registry
(`SessionContext::register_udf`), shadowing the built-in, whose `return_type` is the type cuDF
produces. The cpu and the device would then agree with no cast, and the device arm would refuse
any other `return_type` rather than cast to it. To decide: what the user then sees (`Int16`
where DataFusion and Postgres give a wider integer), every field's cuDF type, and whether the
corpus goldens move. No wrong answer today; this is a choice about where the type is fixed.

<a id="t195"></a>
### #195 — the corpus is numeric-aggregate heavy, and six shapes have no query at all
Measured off `tp1-single.plans.txt` over the 61 enabled queries and the four largest held
back: node counts run 33 to 267 while distinct node kinds run 6 to 12, median 9. More corpus
tests scale, not surface, so each shape below wants a hand-written query over the existing
tables, no new dataset, plus the engine work it needs.

- `ORDER BY … LIMIT n OFFSET m`: every corpus limit has `skip=0`, so both lowerings' offset
  half is synthetic-only; shape it away from [#166](#t166)'s two droppers.
- `min`/`max` over a string or date column: zero uses, and the merge is a string reduce.
- a join on a nullable key: [#59](#t59), [#80](#t80) and [#137](#t137) rest on there being none.
- a shuffle keyed on a decimal: not a query but [#95](#t95)'s kernel work, and the murmur3
  conformance gate extended to cover it.
- two `DISTINCT` args over different expressions: [#144](#t144) has no refusal of its own, and
  `count_distinct` marks queries this mode handles, so a grep for one finds the wrong two.
- a wide `SELECT DISTINCT`: dedup whose state is the whole row, the compaction worst case.

<a id="t161"></a>
### #161 — aggregate shapes the planner refuses: FILTER, and functions with no decomposition
Two refusals in the aggregate arm. A `FILTER (WHERE …)` clause has no lowering, and an
aggregate function outside the decomposition registry is refused by name.

Only the second is reachable: `SELECT median(v) FROM tiny` is refused by name, while
`sum(v) FILTER (WHERE v > 0)` does not parse in DataFusion 45 at all, so that refusal is
constructor-only until the parser gains the clause. The FILTER form would lower to a CASE
inside the aggregate argument and needs no new node; the registry gap is per function and each
wants its merge aggregator stated. Neither shape appears in either benchmark, which is why
they are refusals rather than work.


<a id="t144"></a>
### #144 — multiple DISTINCT arguments need a gid-multiplying expand
**Priority: low** — no query in either benchmark has this shape.

`count(DISTINCT a), count(DISTINCT b)` over different expressions is the one distinct shape the
lowering cannot express.

Single-distinct lowers to grouping on the distinct argument, and non-distinct companions ride
along because Σ over the inner groups recovers each total. Two distinct arguments break it: one
grouping can dedup `a` or `b`, not both, and grouping by `(a, b)` dedups neither. The standard
fix is Spark's `RewriteDistinctAggregates` — an expand multiplies each row into one per distinct
argument tagged with a group id, the inner aggregate groups by `(keys, gid, args)`, and the
outer computes each count from the rows carrying its own gid. That needs a new row-multiplying
node, which is why it is a ticket rather than a planner tweak. Not [#65](corpus-coverage.md#t65), whose gid is the
ROLLUP/CUBE `__grouping_id` — the two would coexist as separate columns. Until it lands the
planner refuses the shape at plan time.

<a id="t261"></a>
### #261 — a `stddev` or `var` beside a DISTINCT is refused at planning
**Priority: low** — no query in either benchmark has this shape.

`stddev(y), count(DISTINCT x)` does not plan, on either backend. Every other companion of a
DISTINCT does.

distinct-companions (chain K, closes [#62](corpus-coverage.md#t62)) lowers a DISTINCT in two
stages. The inner stage groups by `(x, keys)` and runs the companions' inits. The outer stage's
first node reads two kinds of input: the DISTINCT aggregate reads values, and the companions
read the inner stage's state. A node has one phase (`AggregateBody`'s `Phase`, the wire's
`AggregateMode`), so that node is an init, and each companion is merged by an init aggregator:
`sum` over a count or a sum, `min`, `max`. That works for every `Merge::PerColumn` rule. Welford
merges as `Merge::Combined(MergeM2)`: the `[count, mean, m2]` triple merges together, and
`MERGE_M2` exists only as a merge (cuDF's groupby `MERGE_M2`, `cpu_backend/merge_m2.rs`). So the
translator refuses a Welford companion beside a DISTINCT. A `stddev(DISTINCT x)` is not this
ticket: it reads values and plans. Pinned, once distinct-companions lands, by a `bug_` test in
`planner/tests/`.

**Corpus query:** none. Simplest: `select l_returnflag, stddev(l_quantity), count(distinct
l_partkey) from lineitem group by l_returnflag;` (tpch).

**Fix proposed:** let one node mix phases per call.
- Plan: `AggCall` takes a `phase`, the node's phase its default. The outer stage's first node
  marks the DISTINCT aggregate's calls `Init` and the companions' calls `Merge`, `MergeM2`
  included.
- Wire: append `mode: AggregateMode` to `AggregateFuncNode`. Unset means the node's mode, so no
  existing payload moves.
- Cpu: `aggregate_exec` (`cpu_backend/mod.rs`) already runs `AggregateExec` as Partial in both
  phases and builds one UDAF per call. Each call picks `init_aggregates` or `merge_aggregates`
  for itself. The state-layout check and the `u64` → `Int64` count cast go per call.
- Device: `aggregate.cpp` takes the phase per function, not once per node from `agg_phase`.

After [#216](corpus-coverage.md#t216): a keyless outer stage merging a Welford state meets the
device's missing keyless Welford arm. The `bug_` test flips to a plan test and a cpu-vs-device
case.

<a id="t249"></a>
### #249 — the wire has no Time, Duration, Interval, Struct or List type, and writes such a field as `Null`
The fbs `DataType` enum (`flatbuffers/gpu_plan.fbs:14-35`) stops at `Decimal128`, and
`serialize_schema` maps any Arrow type it cannot name to `Null` without saying so
(`wire/serialize.rs:136`). Most device nodes take a column's type from the data, so such a
column usually passes through unnoticed; where the device builds a column from the declared
schema — a join's NULL pads and empty or absent sides, a typed NULL literal — it meets a `Null`
field and either refuses at run time or builds the wrong type, depending on the mode and the data.

Related: [#168](corpus-coverage.md#t168) (an interval literal cannot cross; proposes
`DurationDays`), [#224](scalars.md#t224) (an integer-to-date cast needs a duration type), and
[#245](joins.md#t245) (a nested key cannot cross a shuffle). The join-rewrite chain adds the four
`Timestamp` variants (repartition-keys) and makes an unmapped type a `PlanError` in every schema,
so the gap shows as a plan-time refusal naming the type rather than a wrong pad; the types
themselves remain to add here.

**Corpus queries:** none in tpch or tpcds, and none in pbench yet. The two pbench queries
written for this ticket — an interval column carried through a join
(`SELECT d_id, t.iv FROM dim LEFT JOIN (SELECT t_k, INTERVAL '1' DAY AS iv FROM tiny) t ON d_k = t_k`)
and a struct column carried through one (`SELECT d_id, d_kstruct FROM dim JOIN tiny ON d_k = t_k`)
— cannot reach the wire at all: the planner panics on either column before a plan exists
([#255](#t255)). They land with that fix, and are declared not runnable on this one then.

<a id="t283"></a>
### #283 — a ROLLUP or CUBE over no rows answers no grand-total row
**Priority: low** — no query in either benchmark has an empty input under a rollup.

`select a, count(*), sum(a) from (select 1 as a where false) t group by rollup(a)` answers one
row, `NULL, 0, NULL`, in DuckDB 1.5.4: the grand-total set has no keys, so like a keyless aggregate
it owes a row over nothing. DataFusion 45 answers none, and so do both engines: a grouping-set
init over nothing emits no groups for any set, the empty one included. Found 2026-10-08 reviewing
keyless-identity, which makes a keyless aggregate answer its row and leaves this one as it is.

**Corpus query:** none. Simplest: the query above, or over a filtered `nation` (tpch).

**Fix proposed:** after keyless-identity (chain L): an init with grouping sets, over no rows, emits
the identity row for each set whose mask masks every key, its keys NULL and its grouping id the
set's. DataFusion is not the oracle for it; DuckDB is.


<a id="t255"></a>
### #255 — the planner panics on a Struct or Interval column instead of refusing it
`common::type_structural_size` has an arm per flat type and `panic!`s on everything else
(`common.rs:66`, "add a deterministic arm"). The planner's memory estimation calls it for every
field of every node's output schema (`logical_size_from_schema`, `common.rs:84`), so a query
carrying a `Struct` or an `Interval` column through any node aborts the process at plan time.
Three queries show it, each a `SELECT` a user can write:

    SELECT f_id, d_id FROM fact JOIN dim ON f_kstruct = d_kstruct
    SELECT d_id, d_kstruct FROM dim JOIN tiny ON d_k = t_k
    SELECT d_id, t.iv FROM dim LEFT JOIN (SELECT t_k, INTERVAL '1' DAY AS iv FROM tiny) t ON d_k = t_k

Not [#249](#t249), which is the wire writing an unnamed type as `Null`: this fires first, in the
planner, so the plan #249 describes is never built. Not [#245](joins.md#t245) either — that is the
shuffle's hasher, and the second query above has no nested key at all.

**The guard to delete when this closes:** the three queries' absence is asserted, not merely tolerated — `testdata/test_duckdb_result.py`'s `test_the_queries_that_panic_the_planner_are_not_in_the_tree` goes red the moment a `.sql` appears. So adding the files turns every Rust tier green and then fails in a Python test in the cost-report job. Delete that case in the same commit, and add the three registry rows, corpus lines and `duckdb-result.txt` sections the absence guard currently stands in for.

A panic is the wrong refusal in two ways. The engine's contract for a shape it cannot plan is
`PlanError`, which renders as a `refused:` line a reader can act on; and a panic cannot be
recorded, so these three queries cannot join the corpus at all — they take the whole plan golden
down with them rather than landing with a ticket on their row. The fix is an estimator that
refuses a type it has no deterministic width for, through the same `PlanError` path the rest of
the planner uses. A nested type's width cannot be derived from the parent row count (the panic's
own comment says so), so the arm wanted is a refusal, not a number.

**Corpus queries:** pbench's `struct-key-join`, `struct-through-join` and
`interval-through-join`, whose SQL is above. They are NOT in `testdata/pbench-queries/`: a query
file there is read by every plan-golden renderer, so one of these in the directory aborts all five
of pbench's plan goldens and the dataset cannot land at all. They arrive with the fix. pbench's
own task (`pbench.md`) could not land them — its Restriction forbids fixing what a query
shows — and recorded the hold in `pbench-detail.md`.
