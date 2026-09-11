# #163 — review of the proposal

Read at master 188c23ce (no code moved since the proposal's c18e063a; the diff is wiki only).
Paths relative to `/media/data/peacockdb`; DataFusion 45.0.0 / arrow-arith 54.2.1 from the
cargo registry, as in `Cargo.lock`.

## 1. Verdict

**Needs changes.** The root cause, both walls, and the two code changes are right and every
load-bearing cite checks out; what is wrong is the enablement plan (two of the 23 queries carry a
`stddev` the corpus already knows needs the tolerance oracle) and several descriptions a developer
would find false in the first hour (an `architecture.md` sentence missed, the minimum query's tp4
shape, a vacuous risk, golden line counts).

## 2. Findings

### F1 — q17 and q39 will not pass `data_fusion_exact` at four of five modes, and the proposal does not say so — important

Both queries carry `stddev_samp` beside their `avg`s (`testdata/tpcds-queries/q17.sql`,
`q39.sql`; registry features `stddev_var avg` on lines 18 and 40). A Welford triple merged across
batches or lanes (`merge_m2`) reassociates floats, and the corpus already records the consequence
for exactly this shape: `shuffle_stddev` is declared `data_fusion_approximate, golden_approx_std`
because "tp1-single matches DataFusion to the digit … the other four differ by ~3e-15"
(`corpus_cases.inc:70-84`). q17 and q39 aggregate over joins at tp4 (shuffle, cross-lane merge)
and over many batches at tp1-rowgroup (per-batch init, merge across batches), so the same
divergence is certain there; only tp1-single is exact.

Section 6 lists both as coming back "at all five" and the corpus lines stay `data_fusion_exact`.
Section 7's rule — "each such query is disabled with a ticket" — is the wrong response here: a
developer following it would file a ticket and disable eight cells that need only the oracle
changed. q39 is worse than digits: its inner `WHERE CASE mean WHEN 0 THEN 0 ELSE stdev/mean END > 1`
filters on the float ratio, so a group sitting on the boundary can change the *row set*, which no
tolerance oracle absorbs — improbable at sf1, but it is a row-count failure if it happens, and the
proposal should name it as the shape to look for.

Correction: declare q17 and q39 `data_fusion_approximate` (gpu oracle `golden_approx_std`) from the
start, cite `shuffle_stddev` as the precedent in the corpus comment, and list a boundary row in q39
as the one legitimate reason the approximate compare could still fail.

### F2 — an `architecture.md` sentence 3a falsifies is not on the list — minor

`architecture.md:246-247`: "`AggFunc`, what SQL asked for, and `PlanAgg`, what a node runs — with
state names and types from DataFusion's `state_fields()` so our split cannot drift from the split
it planned." After 3a the types of a per-column decomposition come from the per-column aggregate's
`return_type`, not from `state_fields()`. The proposal lists `:48-50` and `:356-360` and stays
silent on this one, which is the registry paragraph a reader of "The aggregate sequence" lands on.

Correction: add `:246-247` to the pages that move, reworded the same way as `:48-50`.

### F3 — the minimum corpus query's tp4 shape is misdescribed — minor

`SELECT avg(s_acctbal) FROM supplier`: supplier.parquet is 794 KB with one row group. The
small-table rule (`planner/translator/nodes.rs:373-389`) bites only with batching on, so:

- tp4-rowgroup and tp4-sized plan **one lane** (794 KB < `SMALL_TABLE_BYTES` = 5 MiB,
  `planner/mod.rs:37`) — identical to the tp1 plans, no cross-lane merge;
- tp4-single plans four lanes over one row group, `partition_groups=[[[0]],[],[],[]]` (the shape
  every supplier scan shows in `tpch.sf1/tp4-single.plans.txt`, e.g. line 524), so three of the
  four per-lane keyless merges run `compact()` over nothing and answer an identity row of NULLs
  (`cpu_backend/accumulate.rs:371-388`).

So "at tp4 a per-lane merge, a merge of lanes, then the finalizer" is true at tp4-single only, and
there the "merge of lanes" folds one real state with three empty-lane rows. That is a useful
accident — it exercises the nullability argument in 3a (a non-nullable `$count` would refuse right
there, #180's shape) — but it is not the 4-way merge the section implies, and a reader picking
tp4-rowgroup "for the merge path" gets the tp1 plan.

Correction: say so; and for a genuine multi-lane merge at every tp4 mode, name
`SELECT avg(l_quantity) FROM lineitem` (the recipe walk's `AVG_BY_FLAG` without its `GROUP BY`),
which is the smallest shape above the threshold at every mode.

### F4 — the finalize-narrowing risk is vacuous, and the real bound is elsewhere — minor

Section 7: "The finalize's CPU result `(29,6)` narrows to `(19,6)` through `declared_as`'s
`safe: false` cast: an average whose magnitude exceeds 10^13 errors rather than rounds." An average
is bounded by its input's maximum, which fits `Decimal128(15,2)` by construction, so it fits
`(19,6)` always; the narrowing at the finalize cannot trip. The arithmetic bound that exists is
arrow's `l.mul_checked(10^4)` on the `(p+10, s)` sum (`arrow-arith numeric.rs:791-819`) near the
i128 range — unreachable at any scale factor here, and identical to `DecimalAverager::avg`'s own
`mul_checked` in the oracle (`utils.rs:194-210`), so it is not a divergence risk either.

Correction: drop the paragraph, or replace it with the `mul_checked` bound and its symmetry.

### F5 — 3b leans on `declared_as` at the finalize and the trade-off is not stated — minor

After 3b the CPU's finalize project produces `Decimal128(29,6)` and the node's declared `(19,6)` is
reached only because `declared_as`'s `widened_decimal` arm (`cpu_backend/mod.rs:239-275`,
`:499-506`) narrows it. That arm's doc argues a merge ("narrower is not what a merge does"); the
proposal broadens the comment but does not weigh the alternative it implicitly rejected: an explicit
outer `Cast(sum / Cast(count → (p,0)) → (p+4, s+4))` in `finalize()`. The rejection given for
Cast-wrapping in §4 (arrow's scale-reducing cast rounds) does not apply to a same-scale cast — the
outer cast would be precision-only on the CPU and a same-scale `cudf::cast` on the device
(`expr.cpp:912-931` supports it). It would keep "Every cast is explicit" literally true and leave
`declared_as` with the one job its doc claims.

The counter-argument, which decides it for me: if arrow's `+4` ever moved, the outer cast would
*round silently* to the declared scale, while `declared_as` refuses on a scale mismatch — the
proposal's version fails loud. Keep 3b as proposed, but write that reasoning at the site in place
of the broadened comment, and note that `widened_decimal` now has two callers with two reasons.

### F6 — the golden line counts describe the wrong lines — minor

"the lines holding `UInt64` today: tpch 5/5/23/21/21, tpcds 32/32/120/116/116" — the counts are
right (I reproduced them) but the description is not: the grep includes Welford `$count`
declarations, which do not move (`tpch.sf1/tp1-single.plans.txt:1708` is a `stddev` line), and
excludes the finalizing `GpuAggregateBatches` lines, whose `final=[CAST(...$sum AS Decimal128(19,6)) / …]`
moves while their schema holds no `UInt64`. Harmless under `UPDATE_CANONICAL=1`, but a developer
checking the regen against this sentence will find both extra and missing lines.

Correction: "every node line declaring an avg state (`$sum`, `$count`) and every finalizing line
carrying an avg divide; Welford `$count` lines stay."

### F7 — pages and comments that move but are not listed — minor

- `build-test.md:5` (grand total 1569 / Rust 1135) and `:42` ("37 queries", `N = 447`) change with
  up to 106 new cpu cases; the proposal lists only the "thirteen queries … on #163" clause.
- `wire/expr_writer/tests.rs:375-376` comment "a divide of two casts" describes the avg finalize
  shape 3b removes; the fixture stays green, the sentence goes stale.
- `aggregate.cpp:582-597` is cited for "the merge's `sum` over it, INT64 in cuDF" — those lines are
  the dead `is_avg && Merge` arm (never reached: the wire names `sum` and `count`,
  `aggregate_writer.rs:149-158`). The claim holds through the generic arm's `make_agg("sum", true)`
  (`aggregate.cpp:76-86`, `:702-736`); cite that.

### F8 — the new CPU test should build its finalize through `plan::finalize()` — minor

§3 describes the new decimal-avg CPU test as "init and finalize on one node … with state declared
as the planner now declares it". Every existing hand-built avg fixture writes the divide by hand
(`cpu_backend/tests/exec.rs:265-303`). If the new one does too, it pins the arrow/avg `+4`
coincidence but not 3b — a later edit to `finalize()` restoring the numerator cast would leave it
green. Build the expression with `plan::finalize(AggSpec{Avg,0}, &state, at, &Decimal128(19,6))`
as `plan/tests/aggregate.rs:194` does for Welford. The same test should cover the merge leg over
the new `(25,2)` state (produced `(35,2)`, cast back), which no fixture exercises today — all
merge fixtures are Int64 (`cpu_backend/tests/accumulate.rs:188-230`).

## 3. Claims verified

Opened and found true:

- Root cause: `Avg::state_fields` = `[count: UInt64, sum: input type]`, both nullable
  (`average.rs:154-167`); `decompose` copies those types by tag (`planner/translator/aggregate.rs:127-157`);
  the CPU never runs `Avg` — `init_aggregates` resolves `sum`/`count` by name
  (`cpu_backend/mod.rs:388-406`, `plan/aggregate.rs:115-137`); `check_state_layout` refuses at
  construction (`cpu_backend/mod.rs:467-491`), reached from `CpuExec::aggregate` `:130-140` /
  `aggregate_exec` `:323-384` and from the merge (`accumulate.rs:73`) via `executors_for`
  (`backend.rs:81-83`). Device count is INT64 (`aggregate.cpp:823-827`).
- Derived types equal produced types: `Sum::return_type` = `Sum::state_fields` type
  (`sum.rs:153-165`, `:202-207`); `Count` Int64 both (`count.rs:146-167`); `Min`/`Max` return the
  argument and the default `state_fields` uses `return_type` (`min_max.rs:228-230`, `:1055-1057`;
  `udaf.rs:428-439`). `AggregateExprBuilder::build` computes `return_type(arg data types)` with no
  coercion (`physical-expr aggregate.rs:114-149`) — the same call `state_type` makes. Aggregate
  arguments arrive already coerced (`type_coercion.rs:507-530`, `:785-806`; goldens show
  `sum(CAST(… AS Float64))` under avg), so `state_type(Sum, Float64)` for an integer avg is right.
- Effect on standalone `sum`/`min`/`max`/`count` and Welford: none (types equal today's); so no
  enabled cell, no non-avg golden line moves. Widths equal (`src/common.rs:30`, `:37`), so
  `--- memory ---` does not move. Nullability kept from the SQL aggregate's `state_fields` is
  right and necessary: a keyless per-lane merge over an empty lane produces a NULL row
  (`accumulate.rs:371-388` → `compact` → `declared_as`), #180's shape
  (`active-tickets.md:263-276`).
- The second wall: `expr_physical.rs:55-59` drops `out_type`; DataFusion types `Divide` by
  running arrow's `div` on empty arrays (`binary.rs:137-160`); arrow's decimal `Div` is
  `scale = s1+4`, `precision = p1 + 4 + s2` (`numeric.rs:791-819`) → `(19,6)/(19,0)` gives
  `(23,10)`; `widened_decimal` refuses a scale change and `RecordBatch::try_new` errors. After
  3a+3b `(25,2)/(19,0)` gives `(29,6)`, which `widened_decimal` accepts. The device pre-scales the
  numerator from `out_decimal_scale` regardless (`expr.cpp:584-600`; decimals leave the AST path,
  `:403-436`; `expr_writer.rs:46-68` writes the scale), so the removed cast was a redundant kernel.
  DataFusion's `DecimalAverager::avg` truncates `sum × 10^4 / count` (`utils.rs:194-210`), arrow
  truncates the same integer quotient, so the digits agree.
- Untouched surfaces: `aggr_input_schema` has no reader in `cpp/` (grep); `out_type` is read only
  by `expr_writer.rs`; `finalize()` is called directly only by Welford tests
  (`plan/tests/aggregate.rs:194`, `:248`); the payload cover (`test_plan_goldens.rs:450`) keys on
  node call shapes, not expression kinds; the payload sections with an avg are exactly tpch q22,
  tpcds q14, q39, and their rendered text carries the finalize but no state types.
- Registry rows and modes: 23 rows carry `163`, all `plan_status=ok` (`cost-registry.csv`); the
  `corpus_cases.inc` lines and comments are where the proposal says. `all_default_aggregate_functions`
  exists with `sum`, `max`, `min`, `count` as primary names (`functions-aggregate lib.rs:135-173`);
  the re-export is not feature-gated (`datafusion lib.rs:793`). `Merge` and `Decomposition` are
  `Copy` (`plan/mod.rs:381-395`), so the snippet compiles as written.
- The existing tests named for change are where cited (`schema_tests.rs:146-221`;
  `test_gpu_recipe_walk.rs:701-712`); the hand-built fixtures declare `avg(v)$count: Int64`
  (`cpu_backend/tests/exec.rs:265-303`, `accumulate.rs:188-230`, `test_gpu_executors/exec.rs:149-180`).
- Hacks-audit: nothing named for #163; `widened_decimal` is the #187 neighbour ("What I found
  nothing for") and finding 12's second reconciliation — the fix adds no third. No `bug_` test
  exists for #163 (grep).
- #189 for q14/q18/q22 at tp4: a rollup's `__grouping_id` is hashed by the scatter
  (`active-tickets.md:223-237`), same wall as `rollup_over_join`, q5, q80.

## 4. Corrected proposal (sections that change)

### 3. Localized fix — additions to "Tests, goldens, registry, comments that move"

Tests:
- The new decimal-avg CPU test builds its finalize with `plan::finalize(...)`, not by hand, and
  runs the merge leg too: init over `Decimal128(15,2)` rows → state `(25,2)`/`Int64`; merge of two
  such states (DataFusion answers `(35,2)`, `declared_as` casts back); finalize → `(19,6)` digits
  equal to DataFusion's `avg`.

Corpus lines:
- tpcds q17 and q39: `data_fusion_approximate` for the cpu oracle and `golden_approx_std` for the
  gpu one, with the comment pointing at `shuffle_stddev` (`corpus_cases.inc:70-84`) as the
  precedent: the Welford merge across batches and lanes reassociates, tp1-single alone is exact.
  q39's inner filter on `stdev/mean > 1` is the one place an approximate compare can still go red
  legitimately (a boundary group changes the row set); if it does, that is a finding about the
  oracle, not a wall.

Pages and comments (in addition to those listed):
- `architecture.md:246-247` — "with state names and types from DataFusion's `state_fields()`"
  → names ours; types from the aggregate the executor runs each column as (`sum`/`count`'s own
  return types for a per-column split, the Welford accumulator's `state_fields` for the triple).
- `build-test.md:5` and `:42` — the Rust total, the grand total, "37 queries" and `N = 447` move
  with the cells enabled; the "thirteen queries" clause goes.
- `wire/expr_writer/tests.rs:375-376` — "a divide of two casts" → "a divide with a cast
  denominator".
- `cpu_backend/mod.rs:243-248` — say why the finalize is reconciled here and not by an outer
  cast in the IR: a scale that stopped matching would be refused rather than rounded.
- Cite `aggregate.cpp:76-86` and `:702-736` (the generic arm) for the device's merge of the
  count, not `:582-597`.

### 5. Minimum corpus query — corrected description

`SELECT avg(s_acctbal) FROM supplier` reaches both walls at every mode (the init refusal is at
executor construction, the finalize at the top merge). Its plan is one lane at tp1-single,
tp1-rowgroup, tp4-rowgroup and tp4-sized — supplier is 794 KB and one row group, under
`SMALL_TABLE_BYTES` wherever batching is on — and four lanes at tp4-single only, three of them
empty, so that mode also runs the keyless empty-lane merge (`compact()` over no input, a NULL
identity row that the nullable `$count` admits). Run tp1-single first, then tp4-single. For a
cross-lane merge with four real states at every tp4 mode, `SELECT avg(l_quantity) FROM lineitem`
is the smallest shape above the threshold.

### 6. Cells re-enabled — corrected

As proposed, with: tpcds q17 and q39 come back under `data_fusion_approximate` (not
`data_fusion_exact`) at the four multi-batch modes and either oracle at tp1-single.

### 7. Risks and unknowns — corrected

Drop "an average whose magnitude exceeds 10^13 errors rather than rounds" (an average never
exceeds its input's maximum, so the finalize narrowing cannot trip). The arithmetic bound that
exists — arrow's `mul_checked(10^4)` on a `(p+10, s)` sum near the i128 range — is unreachable and
shared with DataFusion's own avg, so it is not a divergence. Add: q39's float-ratio filter is a
row-set risk under tolerance, not a digits risk.

## 5. Complexity

**M for the fix and its goldens, L for the branch.** The code is as small as stated (~45 lines
across four files, two tests changed, one or two added, ten plan goldens and three payload sections
regenerated). What the proposal folds into "the size is in the corpus enablement" is 106 cells
authored, compared and triaged on the CPU backend at sf1 — including q22 and q39 over inventory
(11.7M rows), q14's three-channel union with a rollup, and q9's fifteen scalar subqueries — plus a
shad-gpu run for the recipe walk and the payload golden, plus an oracle change for two queries and
however many third walls the run finds, each a ticket. T19 needed twenty batches to roll out 37
queries; this is 23 more in one branch. That is an L by this repo's own history, and calling it M
sets the wrong expectation for whoever dispatches it.
