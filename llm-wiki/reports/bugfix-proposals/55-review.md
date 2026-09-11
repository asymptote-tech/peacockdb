# #55 — review of the proposal

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. Nothing built or run.

## 1. Verdict

**Needs changes.** The diagnosis is right and verified — the merge takes state column refs, the
wire never writes `Final`, the C++ reads a merge's argument positionally, so the ticket's
mechanism cannot occur — and "no engine change, one device pin, archive the ticket" is the right
fix. What needs correcting is the proposal's model of the plan the probe produces (it assumes
`OneBatchPerLane` yields a `SingleBatch` loader, which it does not), two wrong evidence
attributions, and where the test lands, since two approved tasks already own that file.

## 2. Findings

### F1 — the probe's plan shape is misread: the per-lane merge is emitted (important)

Proposal 3a: "at `TWO_LANES` (`OneBatchPerLane`) lineitem is one batch per lane, so the inner
aggregate inits at two lanes (no per-lane merge — `translator/aggregate.rs:360-368` skips it for a
single-batch lane)", and section 5: "at `ONE_LANE` … both aggregates take the single-batch
shortcut".

Both rest on the loader declaring `SingleBatch` under `OneBatchPerLane`. It declares
`MultipleBatches` whatever the batching — `plan/source.rs:65-71` ("Batching off still declares
MultipleBatches"), architecture.md "Modes and knobs" paragraph, and #170 is the ticket for saying
otherwise. `GpuAggregate` inherits its input's layout (`plan/aggregate.rs:452`), so at
`aggregate.rs:361` `batches(tree) != SingleBatch` is true for an init over a loader and the
per-lane `GpuAggregateBatches` is built. The same file's own `SUM_BY_FLAG` test pins exactly this
(`test_gpu_recipe_walk.rs:685-696`: "two lanes below the shuffle and two above it merge state",
`== 4`), and `tpch.sf1/tp4-single.plans.txt` `== aggregate-groupby` shows the shape (a
`GpuAggregateBatches lanes=4, batches=single` under the `GpuMergePartitions`).

Consequences for the probe:

- At `TWO_LANES` the trail is 4 `CudfAggregate{Partial}`, **6** `CudfAggregate{Merge}` (2 inner
  per-lane, 2 inner cross-lane, 2 outer cross-lane — the outer's input is the inner accumulator's
  `SingleBatch` output through a 1:1 project, so only the outer skips its per-lane half), and 4
  finalize projects. The proposal's `== 4` inits and `>= 2` merges both pass, so the test as
  written is green — but its doc and the "why these numbers" paragraph describe a plan the
  translator does not produce, and `>= 2` is looser than the file's convention for the same
  claim. The hedge "if DataFusion inserts the second repartition" buys nothing: without it the
  outer groups would be split across lanes and the digits compare fails first.
- At `ONE_LANE` only the outer aggregate takes the self-finalizing shortcut; the inner takes the
  merge route with the finalize on the merge — the shape `MAX_OF_SUMS`'s doc (`:716-723`)
  describes. The section-5 sentence is wrong, harmlessly, since no `ONE_LANE` test is proposed.

Correction: assert `== 6` merges and `== 4` finalizes, with the doc saying the inner aggregate
merges per lane and across the shuffle and the outer across it alone. See section 4.

### F2 — the file the test lands in is owned by two approved tasks, and one of them already lists #55 (important)

`test-layout.md` (board task 4, approved to build) moves `test_gpu_recipe_walk` to
`peacockdb-core/src/wire/gpu_tests/` (`test-layout.md:177`, `test-layout-impl.md:656`) and carries
its case count (10). `walk-drives-every-plan.md` (task 13, approved to build) rewrites the walk's
session so a refusal is recorded rather than fatal, and names #55 as one of "the four known
refusals" it will observe (`walk-drives-every-plan.md:126`, `-impl.md:106`, whose step 1 drives
"one of the four known refusals" — #55 among them). `declared-schemas-derived.md:59,96` carries
the same inherited claim.

So 3a as a standalone task either edits a path that has moved or hands task 4 a conflict, and
task 13's plan is partly built on the premise 3a exists to test. The proposal's 3c says the specs
are "conditional on the survey" and leaves the placement open.

Correction: state the landing. The cheapest is to make 3a the first item of task 13 — that task's
step 1 is "argue what is still worth teaching, and stop for the human", and "#55 does not refuse;
here is the pin" is exactly that argument — with 3b/3c following once the run is green. If it must
be its own task, it goes before task 4 on that chain or its path is `src/wire/gpu_tests/…` after
it, and the impl's refusal list drops #55 either way.

### F3 — two evidence attributions are wrong (minor)

Section 2, "What the corpus proves piecewise on a device today":

- "a grouped decimal `SUM` (tpcds q19 at tp1-single)" — tpcds/q19 has no device cell
  (`corpus_cases.inc:181`, gpu modes `none`; registry line 20). The enabled q19 is **tpch**/q19
  (`corpus_cases.inc:25`), which is `sum(l_extendedprice * (1 - l_discount))` with no `GROUP BY`
  — keyless.
- "the `arg_col` → `build_column` path" proven by tpch q6 — q6 is keyless too, so it runs the
  reduce path's `get_values_col` (`aggregate.cpp:184-205`), not the grouped `arg_col`
  (`:447-456`). Both call `build_column` for a non-`ColumnRef` argument, so the expression
  evaluation is proven; the grouped-init-over-a-computed-argument arm specifically is not, by
  the corpus or by the walk (every grouped walk query aggregates a bare column).

The conclusion — the composition is unrun, so run it — stands. The risk list should say the probe
is also the first grouped init over a computed argument on a device, so a red at
`CudfAggregate{Partial}` needs reading before it is called #55.

### F4 — the "single run that answers the ticket as filed" has no arm for the failures it will actually meet (minor)

Section 5's q66-on-device procedure decides between "#183 at the unload" and "#50/#52 failed, #55
alive". Between the loader and `#50` sit twenty-four branch inits of `sum(CASE WHEN <bool col>
THEN ws_ext_sales_price * CAST(ws_quantity …) ELSE 0.00 END)` through `build_column_case`
(`tp1-single.plans.txt:6886`), the union's per-branch casts, and four Inner joins; above `#52`
sit a `GpuSort`/`GpuAccumulateBatchesAndSort` on `Utf8View` keys. A refusal at any of those is
neither outcome, and the proposal says nothing about what to conclude. Add: a failure anywhere
but `#50`/`#52` says nothing about #55 and gets its own ticket if it is not already one of
#56/#183/#152.

### F5 — page counts the change moves are not in the change list (minor)

`build-test.md:19` carries the walk at 10 cases and the header at 1569; 3a makes them 11 and
1570. `test-layout.md:177` carries the same 10. Neither is named in 3b/3c.

### F6 — the probe is not minimal, and the section does not say why the larger shape was kept (minor)

`SELECT l_linenumber, sum(l_extendedprice / l_linenumber), sum(l_quantity / l_linenumber) FROM
lineitem GROUP BY l_linenumber` reaches the same composition — the cast hoisted by CSE, the divide
evaluated by `arg_col` → `build_column_binary`'s decimal arm, the quotient merged as state across
a shuffle — with one aggregate level and half the calls. The inner level in the proposal's query
buys two things it does not name: the numerator is a cuDF `SUM` output rather than the loader's
widened decimal (q66's provenance), and the outer sum lands on `Decimal128(38,6)` so a future
corpus cell would not trip #187 (the walk itself compares names and rendered rows only,
`result_text.rs:84-96`, so #187 cannot fire there either way). Keep the query if those are the
reasons; say so.

## 3. Claims verified

- Translator: `decompose` translates arguments once against the partial's input schema
  (`aggregate.rs:139-142`), attaches them to init calls only (`:159-165`), builds the merge from
  `Expr::column(state_at + offset)` (`:167-190`, `state_at` at `:148`); `aggregate()` always
  decomposes from the partial and uses the finisher for names and output schema (`:211-235`,
  `:278`, `:294-313`); `sum` merges by `sum` and finalizes as a rename (`plan/aggregates.rs:46-49`,
  `:79`).
- Golden: `tpcds.sf1/tp1-single.plans.txt:6882` init `sum(jan_sales@9 / __common_expr_1@0)`,
  `:6880` merge `sum(`sum(x.jan_sales / x.w_warehouse_sq_ft)`@20)` with `final=[…@20]`, `:6883`
  the CSE `GpuProject` with `CAST(w_warehouse_sq_ft@1 AS Decimal128(20,0)) as __common_expr_1`,
  `w_warehouse_sq_ft:Int64`, string keys `Utf8View`.
- Wire: `attach.rs:171` `Phase::Init`, `:254` `Phase::Merge`; `aggregate_writer.rs:78-81` maps
  them to `Partial`/`Merge` and nothing else; `named_func` writes `call.args` verbatim
  (`:159-183`); the wire never names `avg` (`plan/aggregate.rs:99-121`, `sql_name` reached only
  through the Welford fold).
- C++: `agg_phase` (`aggregate.cpp:48-74`); `reads_state = Final || Merge` (`:509`); the width
  guard (`:509-533`); the merge arm `req.values = tv.column(in_off)` (`:706`) never reads
  `args`; only `arg_col` (`:447-456`) evaluates an argument, and the keyless `get_values_col`
  (`:184-205`) reads `args[0]` when `!is_final`, which for a merge is a `ColumnRef` at its own
  position.
- Expression path: `is_ast_able` false on a `DECIMAL128` operand (`expr.cpp:423-428`) and on a
  cast to anything but INT64/FLOAT64 (`:439-447`); `build_column_binary`'s decimal-divide arm
  pre-scales the numerator to `e_o + e_r` and divides at `e_o` (`:579-600`); the cast arm
  (`:912-935`); `out_decimal_scale` from `translator/expr.rs:37-46` via `expr_writer.rs:57-65`.
- Types: `l_linenumber`, `l_suppkey` are `Int64`, `l_extendedprice`/`l_quantity`
  `Decimal128(15,2)` (tpch plan goldens); arrow-arith 54.2.1 `numeric.rs:791-819` gives the
  quotient scale `s1+4` and precision `p1+4` capped at 38, so `(25,2)/(20,0)` is `(29,6)` and
  q66's `(38,2)/(20,0)` is `(38,6)`; the IPC export is precision-38 for any DECIMAL128
  (`gpu_executor.cpp:59-70`, `cudf::to_arrow_schema`), so the outer sum's `(38,6)` matches.
- Registry and cases: `cost-registry.csv:67` `55 152 183`, gpu cells disabled, cpu enabled;
  `corpus_cases.inc:193` gpu `none`; `mini.result.txt:4816-4817` `== q66` / `mode=tp4-sized`;
  the registry test only requires a non-empty tickets column where cells are off
  (`registry.rs:263-281`); the cost-report resolves an archived number (`main.rs:440-460`,
  `:327-341`); the plan-goldens meta test reads `archive/archived-tickets.md` too
  (`test_plan_goldens.rs:794`), and no golden refusal names #55.
- Walk: `ONE_LANE`/`TWO_LANES` both `OneBatchPerLane` (`:43-57`); `assert_walk_matches_datafusion`
  compares sorted row digests under column names against DataFusion at one partition
  (`:582-608`, `result_text.rs:66-130`); `times`, `trail`, `FbKind::Aggregate { merge }`,
  `ProjectRole::Finalize` exist as used; every kind the probe makes is in `PROVEN` (`:798-811`)
  and `driven()` handles it; the walk refuses sorts (`:787`) and multi-batch probes (`:437-446`).
- References to #55 elsewhere: `plan/mod.rs:61`, `architecture.md:62`, `frame.py:29`,
  `declared-schemas-derived.md:59,96`, `walk-drives-every-plan.md:126`,
  `walk-drives-every-plan-impl.md:106`, two archive mentions — all as the proposal lists;
  no `bug_` test, branch or fixture anywhere; hacks-audit.md does not name #55.
- The alternative rejected: `tp1-single-mini.cpu.txt:2885-2912` shows the CPU's join output
  chunked into many batches for q66, so a device cell would diverge on batch lists before any
  comparison reached the aggregate.

## 4. Corrected proposal

Only the sections that change.

### 3a (numbers and doc)

```rust
/// The divide inside an aggregate's argument runs in the init and nowhere else: every merge
/// above it sums state. Two lanes, so the merges are real and the digits cross them — the
/// inner aggregate merges per lane and again across the shuffle, the outer across it alone,
/// since its input is already one batch per lane. A divide that rounded where arrow
/// truncates, or a merge that re-evaluated the argument against state, both show here and
/// nowhere on a CPU.
#[tokio::test]
async fn a_quotient_summed_across_a_merge_keeps_the_digits_the_oracle_computes() {
    let calls = assert_walk_matches_datafusion(SUM_OF_QUOTIENTS, TWO_LANES).await;
    assert_eq!(times(&calls, FbKind::Aggregate { merge: false }), 4,
        "two aggregates at two lanes init once per lane: {}", trail(&calls));
    assert_eq!(times(&calls, FbKind::Aggregate { merge: true }), 6,
        "inner per lane and across the shuffle, outer across it: {}", trail(&calls));
    assert_eq!(times(&calls, FbKind::Project(ProjectRole::Finalize)), 4,
        "each level finalizes once per lane, at done: {}", trail(&calls));
}
```

Drop the "if DataFusion inserts the second repartition" hedge. Add the query to the
`the_kinds_a_device_has_run…` list as proposed.

### 3c (placement and pages)

- Land 3a as the first item of `walk-drives-every-plan`, or sequence it before `test-layout` on
  that chain; either way `walk-drives-every-plan.md:126` and `-impl.md:106` drop #55 from the
  refusal list and the impl's step 1 picks one of the remaining three.
- `build-test.md:19` walk row 10 → 11, header 1569 → 1570; `test-layout.md:177` 10 → 11.

### 5 (evidence and decision procedure)

Replace "tpcds q19 at tp1-single" with "a grouped decimal `SUM` over a bare column, by the walk's
`SUM_BY_FLAG` and `AVG_BY_FLAG`; the corpus's two device queries are both keyless". Say the probe
is the first grouped init over a computed argument on a device. For the q66-on-device run, add
the third outcome: a non-zero rc anywhere but `#50`/`#52` says nothing about #55.

### 7 (risks)

Add: a red at `#… CudfAggregate{Partial}` may be the grouped `arg_col` path itself rather than
the divide; the trail names the seq and the payload golden the expression, so read it before
attributing.

## 5. Complexity

**S**, as proposed. The corrections above are the same test with three exact counts, a placement
decision, and four page lines. The cost is unchanged: one shad-gpu run of the walk, and the
bookkeeping the archive already has a shape for.
