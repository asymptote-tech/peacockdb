# #56 — review of the proposal

Read at master 188c23ce, against `56-proposal.md` as it stood on disk at the time of reading (the
`ProjectSqrtThroughTheColumnPath` cite at `:718-771`). Every file:line below was opened. cuDF
contracts read in `/home/dmitry/cudf` (v25.06.00a-397). Nothing built or run.

## 1. Verdict

**Needs changes** — the diagnosis is right and the fix is the right size, but one of the four
"every primitive has run on a device" claims is false (the grouped `arg_col` branch has no device
run), the root cause can be grounded in git rather than inferred, and one edit the commit owes is not
listed.

## 2. Findings

### F1 — the grouped `arg_col` branch has never run on a device (important)

The proposal: "A computed init argument through `arg_col` → `build_column`: q19's
`sum(l_extendedprice@0 * (1 - l_discount@1))` and q6's, both on the device."

Evidence. Both are keyless — `tpch.sf1/tp1-single.plans.txt` `== q19` line 5: `GpuAggregate:
group_by=[]`, and q6 the same. `execute_aggregate` branches on `key_cols.empty()` at
`cpp/src/operators/aggregate.cpp:231`: the keyless path resolves the argument through
`get_values_col` (`:184-215`, `build_column(arg, tv)` at `:203`) and reduces; the grouped path is the
one below (`groupby` at `:441`, `arg_col` at `:447-455`, consumed at `:731`). q2's init is grouped
(`group_by=[d_week_seq@1]`), so the branch q2 takes is `arg_col`'s non-`ColumnRef` arm, and nothing
in the tree has driven that arm on a device: the six device cells are q6 (keyless) and q19 (keyless);
the walk's grouped queries `SUM_BY_FLAG`, `AVG_BY_FLAG`, `MAX_OF_SUMS`, `ROLLUP`
(`test_gpu_recipe_walk.rs:634-641`) all pass a bare column; the gtest `nation_aggregate`
(`cpp/tests/gpu/test_plan_executor.cpp:805-835`) builds its one argument with `make_col_ref`;
`test_gpu_executors/exec.rs:131` uses `summing("sum(v)")`, a column. Searched `sum(` over
`tests/test_gpu_executors/`, `executor_cases.inc` and the gtest suites.

Why it matters: the verdict "not a live defect" rests on "every primitive the live path composes has
run on a device", and the primitive that is missing is the one that decides #56 — the grouped init
materializing a computed argument. The arm is three lines identical to `get_values_col`'s
(`:203` vs `:452`), so the risk is small, and 3a is exactly its first run. But the proposal must say
so rather than claim it proven; a reader who trusts the list will skip 3a as belt-and-braces.

Correction: move "a computed argument through the grouped `arg_col`" from the proven list to the
unproven one beside the no-else arm, and let 3a's doc comment say it is the first device run of both.

### F2 — the root cause is readable in git, not only inferable (minor, strengthens)

The proposal reconstructs the legacy mechanism from f944c2cd's prose and says "the raw error text is
not preserved in the tree". It is not, but the code that produced it is. The note `q2: GpuAggregate
binaryop "Unsupported operator for these types"` first appears at fe046022
(`peacockdb-core/tests/test_gpu_executor_tpcds.rs:167`), and at that commit:

- `get_values_col` (`cpp/src/plan_executor.cpp:1132-1145` at fe046022) reads `func->args()` and
  calls `build_column(arg, tv)` in **every** phase — there is no `is_final` branch; the positional
  Final read was added later.
- The GPU test ran at `target_partitions = 1` (`test_gpu_executor_tpcds.rs:73`), but q2 still had a
  Partial/Final pair there, because `UnionExec` sums its inputs' partitions (web_sales 1 +
  catalog_sales 1 = 2), so the Final was not adjacent to the Partial and DataFusion did not fold them
  into `Single`. The tp8 plan golden of the day shows the pair (`testdata/plans-tpcds.sf1/q2.txt:18,21`).
- `is_ast_able`'s string-literal gate already existed (`:445`), so the CASE took `build_column_case`
  in both phases; in the Final, `d_day_name@2` indexed `[d_week_seq, sum-state…]` and hit
  `binary_operation(DECIMAL128 col, string_scalar, EQUAL, BOOL8)` — cuDF's
  `is_supported_operation` refuses `Equal<decimal128, string_view>` with exactly that text
  (`binaryop.cpp:207-211`).

So the ticket's "Partial GpuAggregate" is a mislabel — it was the Final — and the proposal's reading
is confirmed by code, not by inference. Cite fe046022 in the Stale marker and in section 2; drop
"the inference rests on" for this half. The T19 inference (below) stays an inference.

### F3 — the "unproven" list contradicts the paragraph after it (minor)

Section 2 lists "the no-else arm (`:790-795`) on a device, and the composition inside a `Partial`"
as unproven, then argues the composition ran once at T19 batch 12. q2's CASE has no ELSE
(`tp1-single.plans.txt` `== q2`: `THEN sales_price@0 END`; `plan_text/expr_text.rs:67-69` prints an
ELSE it has), so whatever that argument proves for the composition it proves for the no-else arm too.
State it once: both are inferred from the scheduling rule and the recorded #152, and 3a is the first
direct observation of either.

### F4 — today's `build_copy` is offered as the T19 mechanism (minor)

`build_copy` (`gpu_backend/join.rs:304-313`) is today's refusal. The archive records that at T19 the
deciding mechanism was **not known** — `tpcds/q11` took 336 probe batches through copying joins
without refusing (`archived-tasks.md:1825-1840`) — while still recording #152 as "a join refusing
its second probe batch" (`:1795-1797`). The inference the proposal needs (the first probe call ran
and its output climbed to `#12` before anything refused) holds under either era's mechanism, and is
in fact stronger than stated: after `#10` emits batch 1, `#11` and `#12` are the smallest-height
runnable nodes, and catalog_sales' batch has not been scanned yet when the Partial runs. Cite the
archive's own sentence rather than today's code for what T19 did.

### F5 — one edit the commit owes is not listed (minor)

`build-test.md:19` counts the walk at `N = 10` and the header at "Rust 1135 … 1569"; 3a adds a test
target case, so the row, the Rust figure and the total move (the two gtest arms sit inside the
existing `AstRouting.IsAstAble` case, so C++ stays 11 at `:57`). The walk row's prose names what
the aggregates there prove and should gain the clause. "Four edits" is five. Also
`cpp/tests/cpu/test_executor.cpp:78-79`, the header comment of `AstRouting.IsAstAble`, names the
decisions the test pins and should name the two new ones.

### F6 — the probe's aliases add a node the "plan shape" paragraph does not mention (minor)

`AS a_price, AS r_price` puts a `ProjectionExec` above the aggregate, which lowers to a `GpuProject`
above the finalize — q2's `… @1 as sun_sales` project is the same shape. It is `FbKind::PlainProject`,
in `PROVEN` (`test_gpu_recipe_walk.rs:798-811`), and `per_batch` (`:326-341`) handles it at two lanes,
so nothing breaks; but the paragraph "Plan shape, from `SUM_BY_FLAG`'s at the same knobs" should say
the shape is `SUM_BY_FLAG`'s plus one project, or the aliases should go (DataFusion's own names match
across both engines either way, since the finalize alias is DataFusion's output name).

### Not findings, recorded so the consolidator does not re-derive them

- `PlanExecutor.ProjectSqrtThroughTheColumnPath` is cited for the CASE fold on a device. Its literals
  go through the helpers `typed-nulls.md:76-93` says are one position off; both literals there are
  `0`/`0.0`, so the bug is invisible and the proof stands — by luck of the values, worth a word if
  the cite is kept.
- `archived-tickets.md:24-27` says the widget resolves a number to "whichever of the two files"; the
  code reads three (`cost-report/src/main.rs:452-456`). Pre-existing drift, not this proposal's, and
  a one-word fix if the archive is being edited anyway.
- `walk-drives-every-plan.md` (board task 13, approved to build) restructures the walk's refusal and
  `PROVEN` machinery; 3a's addition is compatible with it, but the proposal names only
  `test-layout.md` as a mover of that file.

## 3. Claims verified

- **Translator.** `decompose` translates the argument once against the input schema
  (`translator/aggregate.rs:139-142`), attaches it to every init call (`:159-165`), and builds the
  merge calls from `Expr::column(state_at + offset)` (`:167-189`). Golden `== q2` shows init
  `sum(CASE … END)`, merge `sum(\`sum(CASE …)\`@1)`, `final=[…@1…]`.
- **Wire.** `attach.rs:172` `Phase::Init`, `:255` `Phase::Merge`; `aggregate_writer.rs:79-80` maps
  to `Partial`/`Merge` only; `named_func` (`:155-185`) writes `call.args` verbatim and
  `distinct: false`.
- **C++.** `reads_state` (`aggregate.cpp:509`) is true for `Merge`; state arms read
  `tv.column(in_off)` (`:564`, `:577`, `:706`); keyless `get_values_col` reads `args` when
  `!is_final` and a merge's arg is its own position. Grouped init: `arg_col` (`:447-455`) →
  `build_column` (`:452`) → `:731`.
- **Routing.** `is_ast_able`: `CaseExprNode` false (`:405-407`); string literal either side false
  (`:419-420`, `is_string_like_literal` `:326-340` covers Utf8/LargeUtf8/Utf8View and the binaries).
  `build_column` (`:820`) checks literal, then column ref, then `is_ast_able` (`:861`), then the
  switch with `build_column_case` (`:906-907`). `build_column_case` (`:773-803`): value-form guard
  `:779`, no-else fill `:790-795`, fold `:797-802`. `build_column_binary` column-scalar fast path
  `:609-616`; `build_scalar` string arm `:476-480` with `valid = !is_null` (`:456`);
  `binop_output_type` predicate → BOOL8 (`:553-556`).
- **cuDF.** `copy_if_else` rule `output[i] = (mask.valid(i) and mask[i]) ? lhs[i] : rhs[i]`
  (`copying.hpp:611`) — a NULL condition selects the else side; `have_same_types` on lhs/rhs
  (`copy.cu:367-368`), scale included for fixed-point. `make_default_constructed_scalar` on
  fixed-point keeps the scale and is invalid (`scalar_factories.cpp:131-139`; commit baeffb8073,
  2021-10-15, v21.12). `Equal` on `string_view` with BOOL8 out is supported (`compiled/util.cpp`
  `bool_op`, `operation.cuh:371`); the refusal text is `binaryop.cpp:210`.
- **Loader widening** to DECIMAL128 with scale preserved: `scan.cpp:98-108`.
- **Plan facts.** `ELSE NULL` is in the SQL (`tpcds-queries/q2.sql:13-16`) and absent from the
  physical CASE (golden + `expr_text.rs:67-69`, and the translator keeps any else it is given,
  `translator/expr.rs:98-101`). `l_linenumber:Int64`, `l_returnflag:Utf8View`,
  `l_extendedprice:Decimal128(15,2)` in the tpch goldens. Int64 keys are hashed on a device
  (`test_inc2_conformance.rs:167-172`; `spark_hash_partition.cu:172`).
- **Corpus and registry.** `corpus_cases.inc:207` gpu `none`, comment `:201-203`; registry `:3`
  `56 152`; `tpch/q2` is `:102` with `152 187`. `tp1-single-mini.cpu.txt` `== q2`: join
  `in_rows=[[73049],[2160932]]`, merge `batch_rows=[[719384,1441548]]`, join `batch_rows` in 8192s.
  The registry loader only requires a disabled row to name some ticket (`registry.rs:270-282`), so
  `56 152 → 152` is safe; nothing else in code, tests or goldens names #56 (the `#56` hits in
  `recipe-payloads.txt` are seq numbers).
- **Device evidence that holds.** q19 tp1-single is a green cell (`corpus_cases.inc:25`) whose
  filter predicates and join residual carry string-literal equalities (`tp1-single.plans.txt`
  `== q19` lines 6, 8, 10) through `filter.cpp:27` and `join.cpp:366`. `SUM_BY_FLAG` at two lanes
  asserts 4 merges and 2 finalizes (`test_gpu_recipe_walk.rs:684-698`). `TpchSf40.Q8`
  (`test_tpch.cpp:703-717`) runs the bare chain. `ProjectSqrtThroughTheColumnPath`
  (`test_plan_executor.cpp:718-771`) runs a search-form CASE with an ELSE through the fold.
- **The walk.** `assert_walk_matches_datafusion` compares digests of rendered rows, names not types
  (`common/mod.rs:145-160`, `result_text.rs:84-98`), so a (38,2) export against a (25,2) oracle and a
  `Utf8` export against `Utf8View` both pass — the proposal's #183/#187 argument for 3a holds.
  `ONE_LANE`/`TWO_LANES` are `OneBatchPerLane` (`:42-56`); `driven` refuses Sort at `:788` and
  cross/nested-loop at `:790`; `PROVEN` `:798-811`; the cover list `:816-828`.
- **The gtest helper trap.** `CreateScalarValue`'s second positional is `is_null`
  (`cpp/build/generated/gpu_plan_generated.h:954-967`); `typed-nulls.md:76-93` records it.
- **Comments and pages naming #56.** `plan/mod.rs:61`, `architecture.md:60-63`, `frame.py:29`,
  and one baseline (`module-layout-baselines/doc-sentences-final.txt:3694`); no task spec names it.
- **Hacks-audit.** Nothing for #56 in either pass; findings 5 and 10 are in `aggregate.cpp` off
  this path.
- **CPU side.** `expr_physical.rs:86-108` rebuilds `CaseExpr::try_new(comparand, arms, otherwise)`.

## 4. Corrected proposal — the sections that change

### 2. Root cause (replace the two paragraphs "What has run on a device" and "The composition has in fact run once")

The mechanism as filed is in git. At fe046022, where `test_gpu_executor_tpcds.rs:167-168` first records
`q2: GpuAggregate binaryop "Unsupported operator for these types"`, `get_values_col`
(`plan_executor.cpp:1132-1145`) called `build_column(arg, tv)` for a non-`ColumnRef` argument in
every phase, and q2's plan carried a Partial/Final pair even at the test's `target_partitions = 1`
because the union under the join has two partitions. The Final evaluated `d_day_name@2 = 'Sunday'`
against `[d_week_seq, state…]`, where ordinal 2 is a DECIMAL128 sum, and cuDF's `binary_operation`
refuses `Equal<decimal128, string_view>` with that text (`binaryop.cpp:207-211`). The ticket's
"Partial" is a mislabel; the AST never saw the CASE, at fe046022 or now (`is_ast_able` had the
string-literal gate at `:445` then, `:419` now, and refuses a CASE outright).

Closed today by the three layers above. What has run on a device, piece by piece: a string-literal
equality through `build_column_binary`'s column-scalar arm (q19 tp1-single, filter and residual); a
computed argument through the **keyless** `get_values_col` → `build_column` (q19, q6); a search-form
CASE with an ELSE through the fold (`ProjectSqrtThroughTheColumnPath`); a grouped decimal `SUM`
merged across a shuffle (`SUM_BY_FLAG` at two lanes); the bare cuDF chain (`TpchSf40.Q8`). Not run
on a device: the **grouped** `arg_col` non-`ColumnRef` arm (`aggregate.cpp:452`) — the branch q2's
init takes, three lines identical to the keyless one — and the no-else fill (`expr.cpp:790-795`).
Both are inferred to have run once at T19 batch 12: the archive records #152 for tpcds/q2 at
tp1-single as "a join refusing its second probe batch" (`archived-tasks.md:1795-1797`), and under the
smallest-height rule the first probe batch's output climbs `#11` → `#12` → the accumulator before
catalog_sales' batch is even scanned. A live #56 would have failed at `#12` first. 3a is the first
direct observation of both arms.

### 3. Localized fix — five edits, and two lines in the ones already listed

- 3a unchanged in substance. The doc comment says it is the first device run of the grouped
  computed-argument arm and of the no-else fill. The shape is `SUM_BY_FLAG`'s plus one
  `PlainProject` for the aliases (or drop the aliases; either is in `PROVEN`).
- 3b unchanged; extend the `AstRouting.IsAstAble` header comment (`test_executor.cpp:78-79`) to name
  the string-literal and CASE decisions.
- 3c: the Stale marker cites fe046022 for the mechanism rather than inference, and the archive
  header's "two files" becomes three while the file is open.
- 3e (new): `build-test.md:19` walk row `N` 10 → 11, its prose gains "and a sum over a
  string-conditioned CASE, whose merge sums state"; header Rust 1135 → 1136, total 1569 → 1570.

### 7. Risks — one line replaces the first two bullets

The grouped `arg_col` arm and the no-else fill have no device run behind them; both are inferred
from T19 batch 12's recorded #152 and the scheduling rule, and 3a is the first direct observation.
If 3a goes red at a `CudfAggregate{Partial}`, #56 is live with a mechanism other than the recorded
one, and the message names the step.

## 5. Complexity

**S**, agreeing with the proposal. One walk test (~25 lines), two gtest arms with two helpers
(~35 lines), one registry cell, one ticket moved, one `build-test.md` row and its two counts. The
device cost is one more query in the walk's per-test run and in its cover read; nothing regenerates.
