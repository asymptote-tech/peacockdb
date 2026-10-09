# A DISTINCT aggregate plans beside any companion

Kind: production

**This task closes [#62](../tickets/corpus-coverage.md#t62)** (a DISTINCT beside an avg or a count
is refused at planning). Second of chain K, on guard-checks' branch; runs beside chain J while the
GPU host is down, so everything it proves, it proves on the cpu. Files nothing new: the one shape
it still refuses with no ticket of its own is [#261](../tickets/complete-coverage.md#t261), filed
with this spec.

## Why it happens

DataFusion removes a DISTINCT only through `SingleDistinctToGroupBy`, whose precondition is that
every DISTINCT aggregate shares one argument, every other aggregate is `sum`, `min` or `max`, and
there are no grouping sets: its outer level re-applies the same function, sound only where
`f(f(x))` is `f(x)`. Anything else reaches us with the flag set, and `decompose`
(`planner/translator/aggregate.rs`) refuses it with `DISTINCT inside … (#62)`. tpcds q28 — six
global `avg, count, count(DISTINCT ss_list_price)` blocks, cross-joined — is refused at every mode.

The restriction is DataFusion's, not the shape's. This engine already separates init from merge
(`decomposition()`, `plan/aggregates.rs`): a `count` merges by `sum`, an `avg` by `sum` over each
of its two state columns. So the outer level can merge each companion's state with ordinary
aggregators instead of re-applying the function.

## The lowering

Applies when every DISTINCT aggregate in the node, of any function the engine decomposes
(`count`, `sum`, `avg`, `min`, `max`, `stddev`, `var`), has the same single argument `x` once
DataFusion's coercion casts are stripped. DataFusion casts `sum`'s and `avg`'s argument and not
`count`'s, so `count(DISTINCT c), sum(DISTINCT c)` over an `Int32` column arrives as `c` and
`CAST(c AS Int64)`. A widening cast that keeps distinct values distinct is stripped: integer to a
wider integer, integer to `Float64` (exact below 2^53), decimal to a wider decimal, a decimal of
precision ≤ 15 to `Float64`. The inner stage groups on the stripped `x`; each DISTINCT aggregate
re-applies its own casts over `__distinct_arg`. The decision is made before the node's input is
translated, so the input is translated once: a second translation reaches its sources twice, and
tp4-sized's two-pass planner refuses a plan whose passes reach different source counts. Two
`aggregate_sequence` stages replace the one:

- **Inner stage.** Groups by `(x, keys)` — `x` first — and runs the companions' inits. `x` is
  always its own key column, `__distinct_arg`, even where it is also a group key: under a grouping
  set a key can be masked to NULL, and the distinct argument never is. Shuffled as DataFusion
  shuffles the aggregate — on its keys (a subset of the inner's), or collapsed for a keyless one —
  so every row of an outer group is already in one lane. Emits state; no finalize.
- **Outer stage.** Groups by `keys`; no shuffle (a subset of the inner's keys, `plan/aggregate.rs`'s
  subset rule). Its first node is an init: each DISTINCT aggregate runs as its non-distinct twin
  over the deduplicated `x` (from values); each companion's state column runs its merge rule from
  `decomposition()` (`sum` over a count or a sum, `min`, `max`) as an init aggregator. It then
  merges and finalizes to DataFusion's final schema, so nothing above the node changes.

A NULL `x` makes one inner group, and the outer `f(x)` skips it, as SQL requires.

The four traps from `reports/bugfix-proposals/62-review.md`, built in:
1. The outer stage pairs each companion's state columns with `rule.state` by position, not by
   tag. `declared_nullable`'s `[tag]` lookup does not match this engine's own names (`avg(…)$sum`,
   `$count`) and would refuse q28's `avg` as drifted.
2. Each outer argument's type is the inner stage's declared type at its position, through
   `state_type(merge_func, …)`. `arg_type` reads the raw input's first expression, which in the
   outer stage is the wrong column or out of range.
3. Every state column the outer stage declares is nullable, and every `count` in it — companion
   or DISTINCT — finalizes as `CASE WHEN o IS NULL THEN 0 ELSE o END`: a count merged by `sum`
   answers NULL over an empty keyless input, where SQL says 0. `plan/aggregates.rs`'s count
   finalize stays the bare column; the translator wraps it for the outer stage only.
4. The shapes under "Still refused" stay refused.

And one the review of this spec found: the outer init's `sum` over a decimal state widens it again
(`state_type`), so an outer `sum` companion's state is wider than DataFusion's declared output. Its
finalize casts explicitly to the declared type (`architecture.md`: no executor changes a type the
plan did not ask for).

## Grouping sets

A ROLLUP, CUBE or GROUPING SETS under a DISTINCT plans too. The inner stage expands the sets over
`(x, keys)` with `x` unmasked in every set. DataFusion folds the id first key highest, `(acc << 1)
| masked`, so `x`'s bit is the top one and always 0: the inner `__grouping_id` is the value
DataFusion computes over `keys` alone. Its width is not always: at exactly 8, 16 or 32 keys the
extra key moves it to the next type. The outer stage groups by `keys ++ [__grouping_id]` at the
inner's width; where DataFusion's width differs, a project above the outer stage casts the id to
DataFusion's type. Not a cast in `group_by`: a merge's co-location check and the hash it carries
up read plain column keys only, so a cast key would lose the shuffle's distribution.

On the device the id is [#65](../tickets/corpus-coverage.md#t65)'s: bits from the other end and
`Int32`. There `x` is bit 0, so the inner id is twice the device's usual value — still one per set,
so the groups hold, and only the id's value is wrong, as #65 already makes it; at 8, 16 or 32 keys
the project's cast also overflows it. #65's fix removes both. No device cell reaches it before
then: every grouping-set corpus query is off on #65, #152 or #189. #65 gains a line saying so.

## Still refused

The lowering's classifier refuses these two by name, before `decompose`. Each is a `bug_` test
(`coding-style.md`), with its ticket in a comment above it:
- a `stddev` or `var` companion beside a DISTINCT —
  [#261](../tickets/complete-coverage.md#t261). Its state merges as `Merge::Combined(MergeM2)`, a
  merge-phase aggregator only, and the outer node is an init. A `stddev(DISTINCT x)` is not this:
  it reads values and plans.
- DISTINCT over two arguments, or over two different arguments —
  [#144](../tickets/complete-coverage.md#t144), now with a refusal of its own (it fell to #62's
  until now). #195's bullet saying it has none is updated.

Neither test name carries its ticket number (`coding-style.md`'s Names rule); the number is in
the comment.

Any other aggregate that still carries the flag reaches `decompose`, whose `is_distinct()` check
stays and refuses it: `DISTINCT inside <agg> in a shape the lowering does not handle`. No SQL we
know reaches it. It is the net for a shape DataFusion sends that the classifier does not
recognise, which would otherwise be computed as non-distinct on both engines, where the
cpu-vs-device comparison cannot see it. It names no ticket: it is not a known wrong answer.

## The wire

`AggregateFuncNode.distinct` goes: marked `(deprecated)` in `gpu_plan.fbs`, so `alias` and every
later field keep their slots and both generated sides lose the accessor. With it go the writer's
`distinct: false` (`wire/aggregate_writer.rs`), the C++ guard that reads it (`aggregate.cpp`,
"DISTINCT aggregate … see #62"), and the argument at the three `CreateAggregateFuncNode` calls in
`cpp/tests/gpu/test_plan_executor.cpp`. Nothing sets the flag before or after this task: the
rewrite removes DISTINCT before the wire. FlatBuffers omits a `false` at its default, so
`recipe-payloads.txt` does not move; the payload test confirms it.

## Corpus

q28's five plan goldens go from the `(#62)` refusal to a plan. q28 gains a `corpus_cases.inc`
line, its cpu modes run and each one enabled if it passes (its cpu, cost and result sections
written by the cpu tier), or ticketed if it does not. Its gpu cells stay off: q28 is a cross join,
behind #152 (join-backend). Registry row 29: the plan cells enabled, the cpu cells as run, `62`
struck.

A new tpch query, `testdata/tpch-queries/rollup-distinct.sql`, puts a DISTINCT under a grouping
set, which no corpus query does:

```sql
-- A DISTINCT aggregate under a grouping set. DataFusion's SingleDistinctToGroupBy declines
-- grouping sets, so the flag reaches the translator; the avg companion is q28's.
SELECT l_returnflag, l_linestatus,
       count(DISTINCT l_suppkey), avg(l_quantity), count(*)
FROM lineitem
GROUP BY ROLLUP(l_returnflag, l_linestatus);
```

It reads lineitem whole so the tp4 modes split it across lanes and the inner stage shuffles; about
60k inner groups. Its five plan goldens and its cpu sections are written. Cpu cells: the tp1 modes
enabled; the tp4 modes off on #189, whose shuffle cannot hash the inner merge's `__grouping_id`
(as `rollup_over_join`'s are) until repartition-keys lands. Gpu cells off at every mode on #65,
whose `Int32` id the schema validator refuses, and #189 at tp4. A new registry row: features
`rollup count_distinct`, tickets `65 189`.

A second new tpch query, `testdata/tpch-queries/distinct-functions.sql`, reaches the widened
functions, which no corpus query does:

```sql
-- DISTINCT aggregates of the functions the lowering widens to, beside companions DataFusion's
-- SingleDistinctToGroupBy declines (count(*), avg), so the flag reaches the translator.
SELECT l_returnflag,
       count(DISTINCT l_quantity) AS distinct_qty,
       sum(DISTINCT l_quantity) AS sum_distinct_qty,
       avg(DISTINCT l_quantity) AS avg_distinct_qty,
       stddev(DISTINCT l_quantity) AS stddev_distinct_qty,
       count(*) AS n,
       avg(l_extendedprice) AS avg_price
FROM lineitem
GROUP BY l_returnflag;
```

DataFusion cannot be its oracle. With these companions it runs the DISTINCT aggregates itself,
and DataFusion 45 refuses `stddev`/`var(DISTINCT)` ("not implemented"), refuses a keyless
`avg(DISTINCT)`, and answers a grouped decimal or float `avg(DISTINCT)` as the plain average,
silently: its grouped `avg` accumulator ignores the flag. So its line declares a new cpu oracle,
`data_fusion_disabled`, which skips the DataFusion compare; a comment above the line says DuckDB's
answer is to be its oracle once chain J's duckdb-oracle has merged. Until then its answer is held
by the end-to-end case below, which runs this file against a hand-written oracle. All five cpu
cells enabled; gpu cells off — never run, chain K has no GPU — under [#262](../tickets/corpus-coverage.md#t262), filed with this spec.
A new registry row: features `count_distinct`.

All three lines carry whatever fields `corpus_query!` takes when this task builds. If chain J's
duckdb-oracle has merged, each new tpch query needs its `duckdb-result.txt` section, from the
pinned `testdata/duckdb_result.py`, or `duckdb_none`; and `distinct-functions`' cpu oracle stays
`data_fusion_disabled`, its DuckDB oracle on. If repartition-keys merges after this task, its
`drop_grouping_id` lands in `sequence`, keyed on each stage's own id ordinal (n + 1 in the inner
stage), and it turns on `rollup-distinct`'s tp4 cpu cells and strikes `189`; whichever lands
second does that.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/translator/aggregate.rs`, `aggregate/distinct.rs` (new), `aggregate/tests.rs` (new) | the stage split, the two-stage lowering, the refusals |
| `peacockdb-core/src/planner/translator/tests.rs` | plan tests of the two stages |
| `peacockdb-core/src/planner/tests/join_refusals.rs` | the #62 pin replaced by two `bug_` tests |
| `peacockdb-core/src/tests/end_to_end.rs` | the answers at the five modes; `sql_answers_match_oracle`, a variant taking a separate oracle SQL |
| `peacockdb-core/src/test_support/corpus.rs` | the `data_fusion_disabled` cpu oracle |
| `flatbuffers/gpu_plan.fbs`, `wire/aggregate_writer.rs`, `cpp/src/operators/aggregate.cpp`, `cpp/tests/gpu/test_plan_executor.cpp` | `distinct` deprecated; its writer, guard and test arguments |
| `peacockdb-core/src/plan/mod.rs` | `PlanError::Unsupported`'s doc drops "a mixed distinct (#62)" |
| `testdata/goldens/tpcds.sf1/*.plans.txt`, the cpu tier's q28 sections, `testdata/cost-registry.csv`, `corpus_cases.inc` | q28 |
| `testdata/tpch-queries/rollup-distinct.sql`, `distinct-functions.sql`, `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's sections for them, `testdata/cost-registry.csv`, `corpus_cases.inc` | rollup-distinct, distinct-functions |
| `llm-wiki/architecture.md` ("DISTINCT lowers to grouping", "Grouping sets", the wire's `aggr_funcs`), `build-test.md`, `tickets/` | the lowering described; counts; #62 archived, #65, #195 and #261 lines; #262 filed with this spec |

Component-level API: the wire format — `AggregateFuncNode.distinct` deprecated, no slot moves. No
facade, trait or ABI symbol changes.

## Restriction

The lowering, its refusals and the `distinct` field. No backend change: both run the two-stage
plan as they run any aggregate sequence. No device fix for #65.

## Tests

- Plan tests of the two stages: grouped at four lanes (one shuffle, hashed on the keys one column
  right; the outer stage co-located with no shuffle of its own), keyless at four lanes, `x`
  already a key, an `avg` companion (its outer `$sum` state the widened decimal), `count` and
  `sum(DISTINCT)` of one `Int32` column sharing one inner key, a ROLLUP (`x` unmasked in every
  set), and a ROLLUP over eight keys whose project casts a `UInt16` id to `UInt8` — and a
  two-key ROLLUP with no such cast.
- End-to-end on the cpu (`tests/end_to_end.rs`, the five modes):
  - against DataFusion on the same SQL, where DataFusion answers right: `count` and `sum`
    DISTINCT beside `count` and `avg` companions; an argument holding NULLs
    (`store_sales.ss_customer_sk`, 129,392 NULLs at sf1); the empty keyless case, red before the
    fix: `select count(ss_customer_sk), count(distinct ss_customer_sk) from store_sales where
    ss_quantity + ss_item_sk < 0;` (tpcds) answers `0, 0` — a predicate row-group statistics
    cannot prune, since `ss_quantity < 0` prunes every row group and the plan is refused before
    it runs;
  - against a hand-written oracle (`sql_answers_match_oracle`): `distinct-functions.sql` itself,
    against SQL DataFusion answers right — the DISTINCT aggregates over `SELECT DISTINCT
    l_returnflag, l_quantity`, joined on `l_returnflag` to the companions over `lineitem`, under
    the same column names.

  No rollup case here: the harness runs all five modes, and a rollup's tp4
  modes fail on the cpu on #189 until repartition-keys lands; `rollup-distinct`'s tp1 cpu cells
  compare its answer with DataFusion instead.
- `bug_` tests for #261 and #144.
- `decompose` called directly with a distinct `AggregateFunctionExpr` is refused with the
  generic message. This is an ordinary test, not a `bug_` one.
- Every existing plan golden but q28's unchanged (rollup-distinct's are new); `recipe-payloads.txt`
  unchanged.

## Verification bar

- rust-only: `--lib` (plan goldens, the planner's tests), `test_cpu_corpus`, `test_corpus_goldens`,
  `test_cost_model`.
- ffi: `--lib -- ffi_tests::` and the payload test, through `scripts/cargo-cudf.sh`
  (`build-test.md`'s FFI workflow).
- C++: `scripts/build.sh --cudf_ROOT <rapids-cuda-12.2> --gcc-version 12` builds `cpp/build`,
  `peacock_plan_tests` included; `ctest -L cpu`.
- device: none. q28's gpu cells wait on join-backend; the device's DISTINCT walk waits on the host.

## No GPU, superseded

**Superseded 2026-10-09:** this task's GPU tests now run on nebius-gpu, under chain K's board
note in `tasks.md`, which reopens it to `building` for its GPU half. The paragraph below is the
original rule, kept for the record.

Chain K runs without a GPU, so it does not contend with chain J for nebius-gpu. No device run,
no GPU cycle. This task reaches `done` when every CI job but the GPU tests is green; the GPU jobs
are not waited on. The C++ changes are built, not run on a device.

## Completeness signoff

Solved under its constraints. The lowering is as specified — two sequences beside any companion
the engine decomposes, #261 and #144 refused by name, the wire's `distinct` deprecated with no
slot moved — and `distinct-functions` now runs it on a device at five modes, the first such plan
any device has run, the C++ change with it.

Two shortcuts, both on #225 and both earned by a run: a schema-validation mask off at the three
tp4 modes, where #225 refuses the outer init's Welford state, and `golden_approx_std` for one ULP
of `stddev`. The cast-back stays pinned by a plan test, for a better reason than first given — a
device holds `{type_id, scale}`, not a precision. Nothing else.

Owed at merge: q28's and `rollup-distinct`'s device cells, off on #152, #65 and #189 with #262
narrowed to that; #144's `Float32 → Float64` subcase; and two wiki edits nobody asked for, a
`build-test.md` antipattern bullet and a widened `coding-style.md` re-export rule.
