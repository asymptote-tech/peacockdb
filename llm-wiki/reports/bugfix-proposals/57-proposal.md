# #57 — proposal: the value-form CASE lowering was right; the test that condemned it built every literal as zero

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. cuDF contracts read from the
25.02.2 headers at `/media/data/miniforge3/envs/rapids-cuda-12.2/include/cudf/` (the version
shad-gpu runs) and, for the compiled-binop support rules, from `/home/dmitry/cudf/cpp/src/binaryop/`.
DataFusion 45 read from `~/.cargo/registry/src/*/datafusion-{physical-expr,optimizer}-45.0.0/`.
Nothing built or run.

## 1. Issue

`build_column_case` (`cpp/src/expr.cpp:773-780`) throws `"value-form CASE not supported in column
path"` whenever a `CaseExprNode` carries a comparand (`c->expr()`), so any `CASE x WHEN v THEN …`
that reaches a device fails the call. The search form (`CASE WHEN cond …`) is the fold at
`:785-803` and works.

The value form does reach the device. DataFusion does **not** rewrite `CASE x WHEN v` into
`CASE WHEN x = v`: the translator forwards `CaseExpr::expr()` as `comparand`
(`peacockdb-core/src/planner/translator/expr.rs:86-107`), the wire writes it as
`CaseExprNode.expr` (`wire/expr_writer.rs:114-149`), and the plan goldens show it in the only
corpus query that uses it, tpcds q39, at every mode:

    GpuProject: exprs=[…, CASE avg(inventory.inv_quantity_on_hand)@4 WHEN 0 THEN NULL ELSE stddev(…)@3 / avg(…)@4 END as cov]
    GpuFilter: predicate=(CASE avg(inventory.inv_quantity_on_hand)@4 WHEN 0 THEN 0 ELSE stddev(…)@3 / avg(…)@4 END) > 1

(`testdata/goldens/tpcds.sf1/tp1-single.plans.txt:3847` onward, twice each since the CTE is planned
twice; the recipe payloads carry the same shape, `testdata/goldens/recipe-payloads.txt:2519` +58/+61:
`case avg(…)@4 when 0 then null::Float64 else (…) end`.) A grep over all ten `*.plans.txt` finds
the pattern `CASE <expr> WHEN` in q39 alone — four sites per mode, no other query. The row in
`00-tickets.md` is right on that point.

What it disables, corrected: **no cell today, and none on its own.** `corpus_cases.inc:298`
declares q39 `none, none` and its comment (`:293-296`) names #163 at every mode on the cpu; the
registry (`testdata/cost-registry.csv:40`) carries `57 163`. Because a device cell is only ever
enabled where the cpu cell is, #57 is the second wall behind q39's five gpu cells, and it is not
the last one either (section 6). The 00-tickets row also says q39 is absent from the exec-model
corpus; that is true, and the model's `Case` has no comparand "deliberately … it is what #57
withheld on the GPU" (`scripts/exec_model/operators/expressions.py:349-355`) — scaffolding this
fix should take down.

One drift beside it: `PlanError::Unsupported`'s doc (`peacockdb-core/src/plan/mod.rs:52-54`) lists
"a value-form CASE (#57)" among the shapes the planner refuses. Nothing refuses it — `translate_expr`
accepts both forms and `test_planner_join_refusals.rs` has no such pin — so a reader of the enum
believes the shape is caught at plan time when it is a run-time throw on one backend.

## 2. Root cause

Two layers. The guard is the proximate cause; why the guard exists is the real finding.

**The guard.** `expr.cpp:779-780`. Introduced by commit 25bec107 ("withhold value-form CASE
(GPU-incorrect)"), which removed a working-looking implementation — comparand materialised once,
per-branch `comparand == when` through `cudf::binary_operation(EQUAL, BOOL8)`, then the existing
`copy_if_else` fold — because the gtest written for it, `PlanExecutor.ProjectValueFormCase`, failed
on the GPU CI step with "every output row came back 0/null". The commit message and the ticket both
attribute the failure to the lowering.

**Why the test failed: every Int64 literal in that test was zero.** The fbs `ScalarValue` table is
`type, is_null, bool_val, int_val, uint_val, float_val, …` (`flatbuffers/gpu_plan.fbs:45-62`), so the
generated positional constructor is

    CreateScalarValue(fbb, type, is_null = false, bool_val = false, int_val = 0, uint_val = 0, float_val = 0.0, …)

(`cpp/build/generated/gpu_plan_generated.h:954-967`). The test file's helper, unchanged since 12ad4999
(May 25), is

    // cpp/tests/gpu/test_plan_executor.cpp:50-57
    auto sv = fb::CreateScalarValue(fbb, fb::DataType_Int64, /*bool_val=*/false, /*int_val=*/val);

Commit 7ece0548 (June 2, #24) inserted `is_null` as the second field. It updated the join helper's
positional arguments in the same file (`/*filter_columns=*/0`) and not the literal helpers. From
that day `make_int64_literal(fbb, val)` writes `is_null = false, bool_val = (val != 0), int_val = 0`:
a valid Int64 whose value is always 0. `make_float64_literal` (`:59-66`) is the same one slot over —
`val` lands in `uint_val` and `float_val` stays 0.0.

Apply that to the removed test (`git show 25bec107 -- cpp/tests/gpu/test_plan_executor.cpp`):
`CASE CAST(r_regionkey AS Int64) WHEN 0 THEN 100 WHEN 2 THEN 200 ELSE -1 END` was executed as
`CASE … WHEN 0 THEN 0 WHEN 0 THEN 0 ELSE 0 END`. Every row 0 — the reported symptom, exactly, with
no null involved. Whichever value the key column read back, every expected value (100, −1, 200) is
non-zero and every actual value was 0, so every row mismatched. The lowering was never wrong.

Why nothing else caught the helper: it has three other call sites and each is insensitive to the
value. `FilterNation` (`:296`) asks `n_regionkey > 2` and asserts only `0 < rows < 25`, which
`> 0` (20 rows) satisfies as well as `> 2` (10 rows). `ProjectSqrtThroughTheColumnPath` (`:737`,
`:742`) uses literals 0 and 0.0. The value-form test was the first in the file whose answer
depended on a literal's value, nine days after the slot shifted.

**The lowering, checked against both contracts.** SQL's rule for `CASE x WHEN v₁ THEN t₁ … ELSE e END`
is "the first vᵢ with `x = vᵢ` TRUE; unknown never matches; no match is `e`". DataFusion 45
implements exactly that in `CaseExpr::case_when_with_expr` (`datafusion-physical-expr-45.0.0/src/
expressions/case.rs:198-266`): `eq(when, base)`, nulls turned to false by `prep_null_mask_filter`,
the else applied to `base_nulls ∪ unmatched`. The removed C++ was the same three rules:

| rule | DataFusion (cpu) | the fold (device) |
|---|---|---|
| match | `compare_with_eq(when, base)` | `binary_operation(comparand, when, EQUAL, BOOL8)` — null where either side is null (compiled binops AND the input masks) |
| NULL never matches | `prep_null_mask_filter` → false | `copy_if_else`: "Null element represents false" (`copying.hpp:606-631`) |
| first match wins | `remainder = and_not(remainder, when_match)` | fold from the last WHEN back: `if c₀ then t₀ else (if c₁ …)` |

Types agree at the wire because DataFusion coerces comparand and WHEN values to one type, and
THEN/ELSE to one type, before the physical plan (`datafusion-optimizer-45.0.0/src/analyzer/
type_coercion.rs:808-880`, `coerce_case_expression`); q39's `WHEN 0` is `Float64(0)` beside a
Float64 avg, which is why the plan text prints a bare `0`. Where a numeric width still differed,
`binary_operation` takes the common type (`/home/dmitry/cudf/cpp/src/binaryop/compiled/util.cpp:
79-118`, `is_binary_operation_supported`), and string-vs-string EQUAL is supported the same way.

So #57 is a real gap — the arm is missing — but the "recorded wrong answer" it rests on is a
test artefact, and the fix is to put the arm back with a test whose literals are what they say.

## 3. Localized fix

Five files. No Rust engine code, no wire, no goldens.

### A. `cpp/src/expr.cpp` — `build_column_case` (~20 lines net)

Delete the comment and throw at `:775-780`. Materialise the comparand once and derive each
branch's condition from it; the ELSE/fold below stays as written.

    static std::unique_ptr<cudf::column> build_column_case(
        const fb::CaseExprNode* c, cudf::table_view const& table) {
      auto* whens = c->when_thens();
      if (!whens || whens->size() == 0)
        throw std::runtime_error("CASE has no WHEN/THEN pairs");

      // Value form: the comparand is evaluated once and every WHEN becomes
      // `comparand = value`. cuDF's EQUAL is null where either side is, and
      // copy_if_else reads a null condition as false, so a NULL comparand or WHEN
      // falls through to the ELSE — SQL's rule and DataFusion's.
      std::unique_ptr<cudf::column> comparand;
      if (c->expr()) comparand = build_column(c->expr(), table);
      auto condition = [&](const fb::Expr* when) -> std::unique_ptr<cudf::column> {
        if (!comparand) return build_column(when, table);
        auto bool8 = cudf::data_type{cudf::type_id::BOOL8};
        if (when->node_type() == fb::ExprNode_LiteralExpr) {
          auto value = build_scalar(when->node_as_LiteralExpr()->value());
          return cudf::binary_operation(comparand->view(), *value,
                                        cudf::binary_operator::EQUAL, bool8);
        }
        auto value = build_column(when, table);
        return cudf::binary_operation(comparand->view(), value->view(),
                                      cudf::binary_operator::EQUAL, bool8);
      };

      … ELSE column exactly as at :787-795 …

      for (cudf::size_type i = …; i >= 0; --i) {
        auto* wt = whens->Get(static_cast<flatbuffers::uoffset_t>(i));
        auto cond = condition(wt->when());          // was: build_column(wt->when(), table)
        auto then = build_column(wt->then(), table);
        result = cudf::copy_if_else(then->view(), result->view(), cond->view());
      }
      return result;
    }

`build_scalar` and `<cudf/binaryop.hpp>` are already in the file (`:453`, `:7`). The literal
fast path mirrors `build_column_binary`'s column–scalar arm (`:609-626`) and avoids a broadcast
column per branch; the column arm is for a WHEN that is an expression. The comparand is evaluated
once, not per branch — the reason to lower here rather than rewrite into the search form.

Nothing else in the file moves: `is_ast_able` already routes every `CaseExprNode` to the column
path (`:405-408`), `infer_expr_type` already types a CASE by its first THEN (`:389-393`), and both
callers (`operators/project.cpp:45-52`, `operators/filter.cpp:22-28`) already fall through to
`build_column`. The aggregate's argument path (`operators/aggregate.cpp:203`), join residuals
(`join.cpp:366`, `:477`) and sort keys (`sort.cpp:41`) reach the same function and gain the form
for free.

### B. `cpp/tests/gpu/test_plan_executor.cpp` — the helpers, the test, one tightened assertion

1. Rewrite the two literal helpers with the builder so a future field insertion cannot shift them
   again, and add a typed-null one:

       static flatbuffers::Offset<fb::Expr> make_int64_literal(FlatBufferBuilder& fbb, int64_t val) {
         fb::ScalarValueBuilder sv(fbb);
         sv.add_type(fb::DataType_Int64);
         sv.add_int_val(val);
         auto lit = fb::CreateLiteralExpr(fbb, sv.Finish());
         return fb::CreateExpr(fbb, fb::ExprNode_LiteralExpr, lit.Union());
       }
       // make_float64_literal: add_type(Float64), add_float_val(val)
       // make_null_literal(fbb, type): add_type(type), add_is_null(true)

   (The positional form with the `is_null` slot written explicitly is the smaller diff; the
   builder is the one that cannot regress.)

2. Tighten `FilterNation` (`:304-306`) from `0 < rows < 25` to `EXPECT_EQ(rows, 10)` — five nations
   per region, keys 0..4, `> 2` keeps regions 3 and 4. This is the assertion that would have
   caught the helper in June; with the helper fixed it goes from 20 rows to 10 and must be
   updated in the same change.

3. Re-add the test, shaped so a wrong rule shows as a wrong row rather than a wrong total:

       // CASE (CASE WHEN r_regionkey = 3 THEN NULL ELSE CAST(r_regionkey AS Int64) END)
       //   WHEN 0 THEN 100  WHEN 2 THEN NULL  ELSE -1 END        over region (keys 0..4)
       // expected: [100, -1, NULL, -1, -1]

   Row 2 pins a typed-NULL THEN (q39's `WHEN 0 THEN NULL`); row 3 pins a NULL comparand taking
   the ELSE, the one rule the two engines could disagree on and the shape no corpus data
   exercises (tpch has no nulls; q39's avg is never null). Assert `mapped.null_count() == 1`, the
   null at row 2 via `cudf::is_valid(mapped)` copied to host, and the four values by row. The
   inner search-form CASE is the comparand on purpose: it is a non-literal comparand and it
   manufactures the null without a nullable fixture. Keep the CAST-to-Int64 key column beside it,
   as the removed test did, so the expected value is derived from the row rather than assumed.

### C. `peacockdb-core/src/plan/mod.rs:53`

Drop "a value-form CASE (#57)" from `PlanError::Unsupported`'s doc. Nothing refuses the shape.

### D. `scripts/exec_model/operators/expressions.py:349-370`

`Case` gains `comparand: Expr | None = None`; `evaluate` builds the mask as
`comparand_values == when_values` (pandas `==` is False on NaN/None, which is the nulls-as-false
rule) when a comparand is set, and the docstring's last sentence goes. One model test with a null
comparand row. build-test.md's row says the model's scalar expressions are "pinned to what
`expr.cpp` does", and after A that includes the value form. Lowering q39 into
`plans_tpcds.py` is a separate, larger piece and not part of this fix.

### E. Bookkeeping in the same commit

- `testdata/cost-registry.csv:40`: `57 163` → `163` (the row keeps a ticket, which the registry
  parser requires for disabled cells, `tests/common/registry.rs:271-283`).
- `llm-wiki/tickets.md`: #57 to `archive/archived-tickets.md` as Done, one line naming the
  misdiagnosis so the next reader of 25bec107 does not re-derive it; the index at `:19` loses #57
  and its count goes 14 → 13.
- `llm-wiki/build-test.md`: "Plan-executor (C++)" 27 → 28 and the header's C++ 65 → 66.
- `llm-wiki/tasks/operator-cases-impl.md:229-231`: the planned `a_value_case_agrees` row keeps its
  green form; its "if it diverges, rename `bug_`…" clause becomes moot once this lands first, and
  that task's developer should find it green. Not edited here — it is that task's document.

### What this deliberately does not touch

The translator, the `Expr::Case` IR, `expr_writer.rs`, `fb_text.rs`, `gpu_plan.fbs`, the plan
goldens (q39 is in the payload cover subset, `test_plan_goldens.rs:212`, and the payload bytes do
not move because the wire does not), the CPU backend (`cpu_backend/expr_physical.rs:86-108`
already hands the comparand to DataFusion's `CaseExpr`), `is_ast_able`/`infer_expr_type`, the
search-form fold, and `corpus_cases.inc` — q39 stays `none, none` behind #163.

### How the two backends stay one engine

The CPU evaluates the value form through DataFusion's `case_when_with_expr`; the device through the
fold. They agree by the three rules in section 2, each cited to the contract that implements it.
One behavioural difference is pre-existing and shared with the search form: DataFusion evaluates a
THEN/ELSE only on the rows selected (`evaluate_selection`), the fold on every row and discards. For
q39 the discarded rows are `stdev / 0.0` on Float64 — `inf`, not a fault — and no corpus value-form
CASE has an integer divide in a branch.

Pins, in order of what exists today:

- The gtest in B, on a device — the ticket's own ask ("fix plus a direct gtest").
- `operator-cases.md:29`'s planned `a_value_case_agrees` (`operator-cases-impl.md:229-236`):
  comparand `key` Int32 over a fixture with nulls in every column, both engines through one
  comparator. That is the two-engine, null-bearing pin, and it is already planned; this fix makes
  it land green rather than as a `bug_` test.
- Optional, if this lands before the harness: a `Shape::ValueCase` row in
  `tests/common/executor_cases.inc` — `CASE v WHEN 2 THEN 20 WHEN 4 THEN 40 ELSE v END`, expected
  `a|20 a|40 a|6 b|1 b|3 b|5` — mapped in `test_cpu_executors.rs:179` and
  `test_gpu_executors/contract.rs:72` (~45 lines over three files). The fixture has no nulls, so
  it pins the match and first-wins rules only.

### hacks-audit scaffolding

None of the audit's numbered findings touch this path (it did not read `expr.cpp:470-945`). What
grew around #57 and comes down with it: the guard and its comment (A), the doc clause in
`plan/mod.rs` (C), the model's missing comparand and its sentence (D), and — the root of the
misdiagnosis — the positional literal helpers (B), which are the "reader that stops at the first
match" antipattern in constructor form: correct for every argument written before the insertion,
silently wrong after it.

## 4. Alternatives rejected

- **Rewrite at the translator into search form** (`CASE WHEN x = v₁ …`). Same semantics, C++
  untouched — but it moves q39's plan text in all five goldens and its payload bytes and digest
  (q39 is in the payload cover subset), evaluates the comparand once per branch, leaves the IR's
  `comparand` and the fbs `expr` field with no writer, and fixes nothing the executor was wrong
  about. The executor should honour the wire format it declares — and the form is not only
  user-written: DataFusion 45's own simplifier manufactures one, `CASE (x IS NOT NULL) WHEN true
  THEN true END`, for `x LIKE '%'` over a nullable `x` (`datafusion-optimizer-45.0.0/src/
  simplify_expressions/expr_simplifier.rs:1471-1497`), so a rewrite would have to run after the
  simplifier too.
- **Refuse the value form at plan time** to make the doc in `plan/mod.rs` true. Turns a shape the
  CPU answers today into a refusal on both engines, since the planner is shared.
- **Lower it in the cuDF AST.** The AST has no CASE and no strings; every CASE is on the column
  path already.
- **A gather/scatter over a match-index column** instead of the `copy_if_else` chain. Fewer
  passes for many branches, but a second CASE implementation beside the search form's fold with
  its own null rules to prove; the corpus has two arms at most.
- **Fix only the new test's literals** and leave the helpers positional. Leaves `FilterNation`
  testing `> 0` and the next literal-sensitive test to rediscover the shift.

## 5. Minimum corpus query

Against tpcds sf1, both CASE sites q39 uses, a typed-NULL THEN, and a NULL comparand:

    SELECT d_date_sk,
           CASE (CASE WHEN d_moy = 12 THEN NULL ELSE d_moy END)
                WHEN 2 THEN NULL ELSE d_moy END AS moy
    FROM date_dim
    WHERE d_year = 2001
      AND CASE d_moy WHEN 1 THEN 0 ELSE d_moy END > 1

365 → 334 rows: the filter's value-form CASE drops January (31 rows). In the project, `moy` is
NULL for the 28 February rows (the typed-NULL THEN), 12 for the 31 December rows (NULL comparand →
ELSE; a lowering that let a null match, or that returned NULL for a null comparand, shows here),
and `d_moy` otherwise. All Int64, so neither #183 nor #187 stands between the device and the
unload. Modes: all five (date_dim is under `SMALL_TABLE_BYTES`, so the tp4 modes plan one lane and
the plan is the same shape at each). Backend: device; the cpu answers it today.

Today: plans at every mode — the translator accepts both forms and the wire carries the
comparand — and the device fails the first `CudfProject`/`CudfFilter` call that reaches
`build_column_case` with `value-form CASE not supported in column path`, surfaced by
`execute_node`'s catch (`cpp/src/gpu_executor.cpp:200-203`) as a `RunError::CallFailed` naming the
node and lane (`executor/driver/partitioned.rs:826`). The simplifier's one CASE rule
(`expr_simplifier.rs:1383-1389`) applies to the search form with boolean THENs and fewer than
three arms, so neither CASE here is rewritten; q39's own `(CASE … END) > 1` survives it (the
golden), and the nested comparand is a shape no rule names.

## 6. Cells re-enabled

**None by this fix alone.** q39 is the only user, and:

- cpu × 5 stay off on #163 until that lands; the #163 review (F1) already says q39 must come back
  under `data_fusion_approximate` / `golden_approx_std` and that its `stdev/mean > 1` filter is the
  one place a boundary group can move the row set.
- gpu × 5: behind #163 (the Welford count exports Int64 against a UInt64 declaration on the device
  too, `build-test.md` executors row), then #57 (this), then — expected from the plan shape, not
  yet observed because nothing has run past #163 — #185 at tp1-single (`GpuAggregateBatches`
  reporting its own output as `in_rows`, and q39 has two) and #152 at the other four modes (the
  96-row-group inventory scan probes the item/warehouse/date joins one batch per row group). #184's
  1 → 4 repartition shape is also present at the tp4 modes. String hashing for the
  `w_warehouse_name` group key is supported (`spark_hash_partition.cu:164`).

So registry line 40 loses `57` and keeps `163`; the five gpu cells stay `none`. What comes back
outside the corpus: the operator-cases row lands green; the exec-model gains the value form; the
q39 device run, when #163 and #185/#152 clear, has one fewer wall to name.

## 7. Risks and unknowns

- The root cause is established by reading — the generated signature, the helper, the removed
  test, the symptom — not by re-running 25bec107's test. The reconstruction predicts "every row 0,
  no nulls"; the commit says "0/null". If a rerun of the old code with fixed literals still fails,
  the diagnosis is wrong and the lowering has a second defect nobody has seen.
- `binary_operation` on a DECIMAL128 comparand with a `fixed_point_scalar` of a different scale:
  DataFusion coerces both sides to one decimal type, so the scales should arrive equal; cuDF's
  fixed-point EQUAL across scales was not verified. No corpus value-form CASE is decimal.
- Pre-existing, in the shared fold, not introduced here: a CASE with no ELSE and a DECIMAL128 THEN
  nulls-fills through `make_default_constructed_scalar`, whose fixed-point arm ignores the scale
  (`third_party/cudf/cpp/src/scalar/scalar_factories.cpp:39-46`, `scale_type{0}`), so
  `copy_if_else` would refuse the type mismatch. No corpus query reaches it; worth a `bug_`-shaped
  note or a one-line `make_column_from_scalar` over a null scalar of `last_then->type()` if the
  developer is in the function anyway.
- An untyped `NULL` literal (`DataType_Null`) still throws in `build_scalar` — #198's territory,
  and q39's nulls are typed on the wire (`null::Float64`).
- Whether a build with `-Wconversion`-class warnings enabled would have flagged the int64 → bool
  narrowing was not checked; the helper compiled clean under the current flags for three months.

## 8. Complexity

**S.** Two C++ files (~20 lines net in `expr.cpp`, ~70 in the test file: three helpers, one test,
one assertion), one doc line in Rust, ~15 lines of Python, one CSV cell, one ticket to the archive,
two counts in build-test.md. No C ABI, no `.fbs`, no wire bytes, no declared-schema contract, no
golden regenerated. Verification is one shad-gpu run of `peacock_plan_executor_tests` (the new test
and `FilterNation` at its tightened count) plus the rust-only tier for the doc-only Rust change.
