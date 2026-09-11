# #55 — proposal: the defect cannot exist on this wire; close it with one device probe

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. DataFusion 45.0.0 / arrow-arith
54.2.1 from the cargo registry (`Cargo.lock`); cuDF read from `/home/dmitry/cudf` (25.x fork) for
the fixed-point divide contract. Nothing built or run.

## 1. Issue

The ticket (`llm-wiki/tickets.md:327-331`) records, against an older executor: DataFusion evaluates
`sum(decimal / int)`'s division — with the divisor cast to `Decimal128` — only in the Partial phase;
the GPU's Final-phase aggregate re-evaluated the same argument expression against the Final's input
(the partial state), where the cast column does not exist, and cuDF failed the cast. tpcds q66 is the
case: twelve `sum(<month>_sales / w_warehouse_sq_ft)` over a two-branch union.

What it disables (00-tickets.md row, confirmed): nothing on its own. `testdata/cost-registry.csv:67`
carries `55 152 183` on `tpcds/q66`, whose five gpu cells are off and five cpu cells on
(`peacockdb-core/tests/common/corpus_cases.inc:193`, gpu modes `none`, `data_fusion_exact` on the
cpu). The gpu cells are held by #152 at the four multi-batch modes (web_sales and catalog_sales
stream through four Inner joins) and by #183 at tp1-single (the plan's string keys are `Utf8View`,
`tp1-single.plans.txt:6876`). q66 is also absent from the exec-model corpus, which is a hand-lowering
choice and not a defect. The 00-tickets.md "code path" column names "GpuAggregate Final-phase
re-evaluation" — corrected below: no Final phase exists on this wire.

**Correction to the row: #55 is not a live defect.** The current translator, wire and C++ make the
recorded mechanism unreachable by construction, and q66's cpu cells — which evaluate the identical
translated expression — are green at all five modes (`mini.result.txt`, section `q66`,
`mode=tp4-sized`). What is missing is one device observation; nothing in the tree has ever run this
shape on a device.

## 2. Root cause

The old mechanism needed an executor that evaluated an aggregate's *argument expression* in the
phase that reads state. Three layers now prevent it, each on its own.

**Translator.** `aggregate()` (`peacockdb-core/src/planner/translator/aggregate.rs:211-235`) always
decomposes from the *partial* `AggregateExec` — `decompose(partial.aggr_expr(), &input_schema, …)`
at `:278` — and uses the finisher only for output names and the output schema (`:294-313`).
Inside `decompose`, the argument expressions are translated once against the partial's input
schema (`:139-142`) and attached only to the init calls (`:159-165`). The merge calls take
`vec![column.clone()]` where `column` is a `ColumnRef` into the state (`:167-189`); a `sum`'s merge
is `sum(state)` (`plan/aggregates.rs:46-49`) and its finalize a rename (`:79`). The golden shows it:
`tp1-single.plans.txt:6882` init `sum(jan_sales@9 / __common_expr_1@0)`; `:6880` merge
`sum(sum(x.jan_sales / x.w_warehouse_sq_ft)@20)`, `final=[…@20]`. The divisor cast is not even in
the aggregate: DataFusion's CSE hoisted `CAST(w_warehouse_sq_ft@1 AS Decimal128(20,0)) as
__common_expr_1` into a `ProjectionExec`, which is the `GpuProject` at `:6883`, evaluated over
input rows and consumed by the init alone.

**Wire.** `wire/attach.rs:172` writes a `GpuAggregate` as `Phase::Init` and `:255` a
`GpuAggregateBatches` as `Phase::Merge`; `wire/aggregate_writer.rs:78-81` maps those to
`AggregateMode::Partial` and `AggregateMode::Merge` exhaustively. `Final`/`FinalPartitioned`
are never written. The merge node's `aggr_funcs[i].args` are the state column refs the translator
built (`:159-183` writes `call.args` verbatim).

**C++.** In `execute_aggregate` the grouped path reads a state-phase argument positionally —
`reads_state` (`cpp/src/operators/aggregate.cpp:509`) → `req.values = tv.column(in_off)`
(`:705-706`) — and never looks at `func->args()` there; the width guard at `:509-532` checks the
state layout first. Only the init (`Partial`) evaluates an argument, through `arg_col`
(`:447-456`) → `build_column` for anything that is not a bare `ColumnRef`. The keyless reduce path
(`:184-226`) does read `args` when `!is_final`, but a merge's arg is always a `ColumnRef` whose
ordinal equals the position (`state_at = n_keys + offset`, `translator/aggregate.rs:148,170`), so it
resolves to the same column the positional read would.

So the only place the divide runs on a device is the init, over the table the divide was written
against. Tracing that evaluation: `arg_col` → `build_column` (`cpp/src/expr.cpp:820`) →
`is_ast_able` false because an operand is `DECIMAL128` (`:423-428`) → `build_column_binary`
(`:579`) → the decimal-divide arm (`:589-600`): both operands are `DECIMAL128` (numerator: cuDF's
groupby `SUM` keeps the fixed-point scale; denominator: the project's `cudf::cast(INT64 →
DECIMAL128, scale 0)` at `:912-935`), so the numerator is rescaled to `e_out + e_den = −6 + 0` and
`binary_operation(DIV)` answers at scale −6. `out_decimal_scale = 6` comes from
`translator/expr.rs:37-46` (`bin.data_type(input_schema)`) through `wire/expr_writer.rs:57-65`.

Digits agree with the oracle by construction: arrow's decimal `Div` is `(l × 10^(s_out − s_l +
s_r)) div_checked r` with `s_out = s_l + 4`, truncating (`arrow-arith-54.2.1/src/numeric.rs:791-819`);
cuDF's fixed-point `operator/` is `lhs._value / rhs._value` at scale `s_l − s_r`, integer
truncation (`cudf/cpp/include/cudf/fixed_point/fixed_point.hpp:738-749`), written back at the same
scale (`binary_ops.cuh:78-90`). Same integer quotient, same scale, same truncation. This is the
identical arm `an_average_finalizes_to_the_digits_the_oracle_computes`
(`peacockdb-core/tests/test_gpu_recipe_walk.rs:701-713`) already proves against DataFusion's digits
— the avg finalize divide is a `Decimal128 / Decimal128` with `out_decimal_precision != 0` through
`:589-600`, and its denominator is the same `INT64 → DECIMAL128(…,0)` cast (`plan/aggregates.rs:83-98`).

What the corpus proves piecewise on a device today: a computed decimal argument inside a
`Partial` init (tpch q6, `sum(l_extendedprice * l_discount)`, every mode — the `arg_col` →
`build_column` path); the decimal divide arm with a rescaled numerator and a cast-int denominator
(the walk's `AVG_BY_FLAG` at two lanes, including an `Aggregate{Merge}` above it); a grouped decimal
`SUM` (tpcds q19 at tp1-single). The composition — a divide *inside* an init argument, merged as
state above — is not proven anywhere. No state is shared between those pieces, so by reading it
composes; that is the one thing a run adds.

**Relation to #163 (proposal and review).** Same path, different expression class. #163's second
wall is planner-*invented* IR (`finalize()` for `avg`) whose `out_type` the CPU drops
(`cpu_backend/expr_physical.rs:55-59`), so arrow re-types it. q66's divide is DataFusion's own
`BinaryExpr`, translated with the type DataFusion computed, so on the CPU the produced type is the
declared type — which is why q66 is green there and why nothing in #163's 3a/3b touches this path:
`state_type(Sum, Decimal128(38,6))` returns today's `Decimal128(38,6)`, and 3b edits only the
`AggFunc::Avg` arm. The review's verified claim that `expr.cpp:584-600` pre-scales from
`out_decimal_scale` regardless of the numerator's cast is the same fact this reading rests on. One
caution carried over: #163-review F7 notes `aggregate.cpp:582-597` (`is_avg && Merge`) is dead
because the wire never names `avg`; the same holds for every `is_avg`/`is_final` arm on the q66
path — the live arms are `:447-456`, `:702-738`.

## 3. Localized fix

No engine change. The ticket is closed by one observation and one permanent pin, then the registry
stops naming it. Three edits.

### 3a. A recipe-walk test that pins the shape on a device — `peacockdb-core/tests/test_gpu_recipe_walk.rs`

Add beside `MAX_OF_SUMS` (`:638`):

```rust
/// q66's aggregate shape without its joins, strings or sort: a decimal sum divided by an
/// integer key, the divisor cast hoisted by DataFusion into a project because it is shared
/// by two sums, the divide evaluated by the init and the quotient merged as state.
const SUM_OF_QUOTIENTS: &str = "SELECT l_linenumber, \
     sum(price / l_linenumber) AS price_per_line, \
     sum(quantity / l_linenumber) AS qty_per_line \
     FROM (SELECT l_linenumber, l_suppkey, sum(l_extendedprice) AS price, \
     sum(l_quantity) AS quantity FROM lineitem GROUP BY l_linenumber, l_suppkey) x \
     GROUP BY l_linenumber";
```

and one test, in the style of `each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them`
(`:685-696`):

```rust
/// The divide inside an aggregate's argument runs in the init and nowhere else: the merge
/// above it sums state. Two lanes, so the merge is real and the digits cross it. The
/// compare is on rendered digits — a divide that rounded where arrow truncates, or a merge
/// that re-evaluated the argument against state, both show here and nowhere on a CPU.
#[tokio::test]
async fn a_quotient_summed_across_a_merge_keeps_the_digits_the_oracle_computes() {
    let calls = assert_walk_matches_datafusion(SUM_OF_QUOTIENTS, TWO_LANES).await;
    assert_eq!(times(&calls, FbKind::Aggregate { merge: false }), 4,
        "two aggregates at two lanes init once per lane: {}", trail(&calls));
    assert!(times(&calls, FbKind::Aggregate { merge: true }) >= 2,
        "the quotient never crossed a state merge: {}", trail(&calls));
}
```

Why these numbers: at `TWO_LANES` (`OneBatchPerLane`) lineitem is one batch per lane, so the inner
aggregate inits at two lanes (no per-lane merge — `translator/aggregate.rs:360-368` skips it for a
single-batch lane), shuffles on `[l_linenumber, l_suppkey]`, and merges + finalizes per lane; the
outer inits at two lanes over the CSE project, shuffles on `[l_linenumber]`, merges + finalizes. Four
`Partial` calls, at least two `Merge` calls (two per aggregate level if DataFusion inserts the second
repartition, which `Partitioning::satisfy` requires since `Hash([a,b])` does not satisfy
`Hash([a])`; the `>=` keeps the test honest if it does not). Every kind the walk makes here is
already in `PROVEN` (`:798-811`), so `the_kinds_a_device_has_run_are_the_kinds_this_file_claims`
needs no change; add the query to its list (`:816-828`) anyway so the cover reads it.

Types the query lands on, so no other ticket intrudes: inner sums `Decimal128(25,2)`; divisor
`CAST(l_linenumber AS Decimal128(20,0))`, used twice so CSE hoists it as `__common_expr_1` (the
q66 shape); quotient `Decimal128(29,6)` (arrow `numeric.rs:791-819`: scale 2+4, precision 25+4);
outer sum `Decimal128(38,6)` (`Sum::return_type`, `min(29+10, 38)`), which the device exports as
`(38,6)` — so #187 cannot fire; group key `Int64`, no strings — #183 cannot; no join — #152 cannot;
no sort — the walk's own refusal (`:788`) does not; `l_linenumber ∈ 1..7`, never zero. Divisors 3, 6
and 7 make truncation visible in the sixth digit.

### 3b. The registry and the ticket

- `testdata/cost-registry.csv:67`: `55 152 183` → `152 183`. The tickets column names what still
  holds a cell off; the cost-report resolves an archived number too (`cost-report/src/main.rs:440-460`),
  so nothing else breaks either way.
- `llm-wiki/tickets.md:327-331`: remove #55; the Contents row "Blockers for disabled coverage"
  (`:19`) drops to 13 and loses `#55`.
- `llm-wiki/archive/archived-tickets.md`: append #55 under Stale, in this shape — "Stale
  <date>. Unreachable since the aggregate sequence: the merge takes state column refs
  (`planner/translator/aggregate.rs:172-189`), the wire never writes `Final`
  (`wire/aggregate_writer.rs:78-81`), and the C++ reads a merge's argument positionally
  (`aggregate.cpp:705-706`). Pinned on a device by `a_quotient_summed_across_a_merge_…` in
  `test_gpu_recipe_walk.rs`. q66's device cells stay on #152 and #183."

### 3c. Comments and pages that name #55 (no logic)

- `peacockdb-core/src/plan/mod.rs:61` and `llm-wiki/architecture.md:60-63` name "the #55/#56/#63
  bug class" as the reason init and merge see different tables. Both stay true as rationale;
  the numbers resolve to the archive. Leave them.
- `scripts/exec_model/operators/frame.py:29` likewise. Leave.
- `llm-wiki/tasks/declared-schemas-derived.md:59` and `:96`, `walk-drives-every-plan.md:126`,
  `walk-drives-every-plan-impl.md:106` list #55 among refusals the walk hits "today". No detail
  file or run records that; the claim is inherited from the ticket. Both specs are `approved to
  build` and are the helper's: drop #55 from the refusal lists, keep the
  `sum(<decimal> / <int>) with a GROUP BY` measurement row in `declared-schemas-derived.md:96` with
  its expectation flipped to "declared `(38,6)` equals produced". 3a makes that row's walk query
  exist.
- `00-tickets.md:55` (scratch) — the code-path column is wrong as written.

### What it deliberately does not touch

`planner/translator/aggregate.rs`, `plan/aggregates.rs`, `wire/`, `cpp/` — nothing. No `bug_`
test: the behaviour is right, so the test asserts what is right. No corpus cell: a new corpus
device cell for this shape would go red on #185 (the device's `in_rows` at `GpuAggregateBatches`,
`active-tickets.md:97-118`) at every mode and on #184 at tp4 if the source were one lane — neither
is #55's, and the walk compares digits without a per-node golden. The `operator-cases.md` row for
`GpuAggregate` (`:34`) does not list an argument with a decimal divide; when that task builds, one
case there is the second pin, and 3a is not a prerequisite for it.

### CPU and GPU agreement

The CPU already runs this expression as DataFusion's own `BinaryExpr` (`cpu_backend/expr_physical.rs:55-59`)
and matches the oracle by identity (q66's five green cpu cells). The device's arithmetic is the
section-2 trace; 3a is the proof. Both engines consume one IR in which the divide is on the init
and the merge is `sum(state)`.

### Hacks-audit scaffolding

None named for #55 in either audit pass (grep: no `bug_` test, no branch, no fixture). Adjacent
and untouched: finding 10 (`aggregate.cpp` name spellings and the `distinct` guard) and finding 5
(`stddev_ddof` re-deriving the ddof) sit in the same file and are not on this path.

## 4. Alternatives rejected

- Enable `tpcds/q66`'s gpu tp1-single cell to observe it — refused by #183 before any comparison,
  and the per-batch lists diverge at the joins (the cpu golden chunks join output into 8192-row
  batches, `tp1-single-mini.cpu.txt:2910-2912`); a red cell would say nothing about #55.
- A `bug_` test — there is no wrong behaviour to assert.
- A validator comparing a merge's argument against the state schema — the merge's arguments are
  column refs by construction (`:167-189`); a check would guard a shape the type of `AggCall.args`
  does not prevent but the one constructor never builds. Not worth a rule.
- Leave the ticket open until the sink-divergence survey runs q66 — the survey stops at the sink
  (#183) and cannot see the aggregate calls; 3a costs one walk query.
- A CPU-tier test — the CPU evaluates DataFusion's expression with DataFusion; it cannot disagree
  with the oracle and so cannot pin anything about the device.

## 5. Minimum corpus query

The probe in 3a, `SUM_OF_QUOTIENTS`, against tpch sf1 `lineitem` (49 row groups, 6,001,215 rows):

    SELECT l_linenumber,
           sum(price / l_linenumber)    AS price_per_line,
           sum(quantity / l_linenumber) AS qty_per_line
    FROM (SELECT l_linenumber, l_suppkey,
                 sum(l_extendedprice) AS price, sum(l_quantity) AS quantity
          FROM lineitem GROUP BY l_linenumber, l_suppkey) x
    GROUP BY l_linenumber

Plans today at every mode and both walk knob sets (nothing in it is refused by the planner: two
grouped sums, a divide, a cast, no join, no sort). Backend: the device, through the recipe walk at
`TWO_LANES` (the merge is the point) and once at `ONE_LANE` (where both aggregates take the
single-batch shortcut, `translator/aggregate.rs:317-333`, so the divide runs in an init that
finalizes itself). On the CPU it is exact by identity. What it shows today, predicted: seven rows
equal to DataFusion's to the sixth decimal. What would reopen #55: a non-zero `rc` on a
`CudfAggregate{Partial}` call (the divide's cast or binop refused), or seven rows whose sixth
digit differs (a rounded rather than truncated quotient) — the walk's message names the call
trail either way.

The single run that answers the ticket as filed, if the human wants q66 itself: flip
`corpus_cases.inc:193`'s gpu modes to `tp1_single` and the registry's `gpu_tp1_single` cell
together, `PCK_TEST_FILTER=q66` through `scripts/build-test-shadgpu.sh`, and read the driver's
error, which carries node and lane. It must be `GpuUnload lane 0 … expected Utf8View` (#183's
text, `active-tickets.md:73-75`) — i.e. `#50 CudfAggregate{Partial}` in both lanes and `#52
CudfAggregate{Merge}` (`tp1-single.plans.txt:6934-6936`) returned 0. A failure at #50 or #52 is
#55 alive and the ticket stays. Revert both edits after.

## 6. Cells re-enabled

None now. `tpcds/q66` gpu × 5 stay off: tp1-single on #183, the four multi-batch modes on #152.
Dropping `55` from the row is what makes q66 come back with no further triage when those two
clear — today a reader of the row would look for a third fix that does not exist. The walk gains
one query; the walk is not a registry cell.

## 7. Risks and unknowns

- The composition is unrun: a divide inside a `Partial` argument on a device, merged above. Every
  piece is proven separately (section 2) and the C++ has no phase-carried state between them, but
  the first execution of 3a is the observation. If it goes red at `#…CudfAggregate{Partial}`, the
  ticket is live and its mechanism is not the recorded one — the message will name it.
- DataFusion's CSE hoisting the shared cast into a `ProjectionExec` is what the q66 golden shows
  for twelve uses; two uses is the rule's threshold and I did not confirm it against the DF 45
  `CommonSubexprEliminate` source. If it stays inline, the probe still exercises the more general
  shape (the cast evaluated inside `arg_col` → `build_column`'s cast arm, `expr.cpp:912-935`) and
  the test's assertions do not depend on it.
- The `>= 2` merge count assumes DataFusion inserts a second hash repartition above the outer
  partial; if `EnforceDistribution` reuses the inner partitioning, the outer merge is still emitted
  (`Shuffle::ByHash` at one lane collapses to the tree, but two lanes keep it, `:370-377`), so
  the count holds. Not verified against DF 45's `satisfy`.
- cuDF version: the divide arm and both casts run on shad-gpu's 25.02 today through `AVG_BY_FLAG`;
  the fork read here is a 25.x tree, not 25.02 byte for byte. `binary_operation_fixed_point_scale`
  and the truncating `operator/` are unchanged since 21.x.
- Overflow: arrow's `mul_checked(10^4)` on a `(25,2)` sum and cuDF's unchecked `__int128` division
  are both far from range at sf1 and at sf40.
- The specs listed in 3c are the helper's to edit and are conditional on the survey; the claim
  that #55 "aborts the walk today" is unverified, not disproved, until 3a runs.

## 8. Complexity

**S.** One test file gains a constant and a test (~25 lines), one CSV cell loses a number, one
ticket moves to the archive, four spec/wiki lines lose a reference. No engine code, no C ABI, no
`.fbs`, no wire format, no declared-schema contract, no golden regenerated (the walk pins nothing
in `testdata/goldens/`). The cost is one shad-gpu run of the walk.
