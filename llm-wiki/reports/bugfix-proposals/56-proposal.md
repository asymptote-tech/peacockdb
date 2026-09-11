# #56 — proposal: the defect cannot exist on this wire; close it with one device pin

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. cuDF read from `/home/dmitry/cudf`
(25.06 tree) for the `binary_operation`, `copy_if_else` and `make_default_constructed_scalar`
contracts; shad-gpu runs 25.02, and each contract cited is dated below. Nothing built or run.

## 1. Issue

The ticket (`llm-wiki/tickets.md:321-325`) records, against the legacy executor: the partial-phase
`GpuAggregate` evaluating `sum(CASE WHEN d_day_name = 'Sunday' THEN sales_price ELSE NULL END)` hit
cuDF's binaryop `Unsupported operator`, "string comparand in the aggregate AST path". tpcds q2 is the
case: seven such sums per CTE copy, the CTE planned twice, so fourteen CASEs over one `Utf8View`
column inside two `CudfAggregate{Partial}` nodes (`testdata/goldens/tpcds.sf1/tp1-single.plans.txt`
`== q2`, the `GpuAggregate` lines: `aggs=[sum(CASE WHEN d_day_name@2 = Sunday THEN sales_price@0 END)
…]`, recipes `#12` and `#32`).

Commit f944c2cd grounded the class in code: "the aggregate materializes its serialized argument
expression against whatever table its phase holds (`aggregate.cpp` ~L194) … a string comparand in
that argument hits cuDF binaryop 'Unsupported operator' (#56)". So the mechanism as filed is #55's
with a string instead of a cast: the Final phase re-evaluated the init's argument against the
partial state, where ordinal 2 is a `DECIMAL128` sum, and `DECIMAL128 = 'Sunday'` is an
`Equal<decimal128, string_view>` cuDF's `is_supported_operation` rejects
(`/home/dmitry/cudf/cpp/src/binaryop/binaryop.cpp:207-211`, `compiled/util.cpp:160-175`). The
ticket's "AST path" wording is a second reading of the same failure — the AST cannot hold a string
at all, so an argument routed there would die the same way — and today neither route exists.

What it disables (00-tickets.md row, corrected): nothing on its own. `testdata/cost-registry.csv:3`
carries `56 152` on `tpcds/q2`; five cpu cells on and green (`data_fusion_exact`), five gpu cells off
(`peacockdb-core/tests/common/corpus_cases.inc:207`, gpu modes `none`). The comment at `:201-203`
attributes every gpu cell to #152 alone, tp1-single included — the probe side of the join under the
init unions web_sales and catalog_sales, so that join sees two batches at every mode. Two
corrections to the row: #187 is not on tpcds/q2 (its sink is `Int64` + seven `Float64`, no decimal
reaches the unload; the `q2` under #187 is `tpch/q2`, registry line 102), and the code-path column
"partial-phase AST" names a path that does not exist — the aggregate's argument never enters
`build_expr`.

**Correction to the row: #56 is not a live defect.** The current translator, wire and C++ make the
recorded mechanism unreachable by construction; every primitive the live path composes has run on
a device; and the one device run of q2 the tree records (T19 batch 12) is an outcome only reachable
through a `Partial` that succeeded. What is missing is a pin that stays green in CI.

## 2. Root cause

Two mechanisms are named by the ticket and its grounding, and both are closed.

**The phase re-evaluation (the grounded mechanism) — three layers, each sufficient.**

- Translator. `decompose` (`peacockdb-core/src/planner/translator/aggregate.rs:139-142`) translates
  the argument once, against the partial's input schema, and attaches it to the init calls only
  (`:159-165`). The merge calls take `state_columns` — `Expr::column(state_at + offset)`, ordinals
  into `[keys…, state…]` (`:167-189`). The golden shows it: init `sum(CASE WHEN d_day_name@2 =
  Sunday THEN sales_price@0 END)`, merge `sum(`sum(CASE …)`@1)`, `final=[…@1, …]`.
- Wire. `wire/attach.rs:172` writes a `GpuAggregate` as `Phase::Init`, `:255` a
  `GpuAggregateBatches` as `Phase::Merge`; `wire/aggregate_writer.rs:79-80` maps those to
  `AggregateMode::Partial` and `::Merge` and nothing else. `Final` is never written. Args are
  written verbatim (`:165-169`).
- C++. `execute_aggregate`'s grouped path: `reads_state` (`cpp/src/operators/aggregate.cpp:509`) is
  true for `Merge`, and every state-reading arm takes `tv.column(in_off)` positionally
  (`:564`, `:577`, `:706`) — `func->args()` is not read there. The keyless path
  (`get_values_col`, `:184-215`) reads `args` when `!is_final`, and a merge's arg is a `ColumnRef`
  whose ordinal is its own position, so it resolves to the same column.

So the CASE is evaluated exactly once on a device, in `Partial`, against the table it was written
for: `arg_col` (`:447-454`) → `build_column(arg, tv)` (`:452`) for anything that is not a bare
`ColumnRef`, consumed at `:731` by the plain `sum` request.

**The AST routing (the ticket's title) — closed by `is_ast_able`, twice over.**

`build_column` (`cpp/src/expr.cpp:820`) sends an expression to `compute_column` only when
`is_ast_able` says so (`:861`). It says no for a `CaseExprNode` unconditionally (`:406`), and no for
a `BinaryExprNode` with a string-like literal on either side (`:419`, `is_string_like_literal`
`:326`). Inside the CASE the same gate runs again per branch. So on the live path:

    arg_col → build_column(CASE)            is_ast_able: false (:406)  → build_column_case (:773)
      c->expr() null, search form                                        (:779 guard not taken)
      else_expr null → last_then = build_column(sales_price@0)          column copy, DECIMAL128 e=-2
                       null fill: make_default_constructed_scalar(type)  (:793) scale preserved
                                  make_column_from_scalar(null, n)
      cond = build_column(d_day_name@2 = 'Sunday')                       (:799)
             is_ast_able: false (:419) → build_column_binary (:579)
             column-scalar fast path (:609-616):
               build_scalar(Utf8View "Sunday") → string_scalar (:477-480)
               binop_output_type(Eq, STRING, STRING) → BOOL8 (:555-557)
               cudf::binary_operation(STRING col, string_scalar, EQUAL, BOOL8)
      then = build_column(sales_price@0)                                 column copy
      result = copy_if_else(then, result, cond)                          (:801)
    → groupby SUM over a DECIMAL128 column                              (:731-742)

Every contract in that chain holds in cuDF: `EQUAL` on `string_view` with a `BOOL8` output is
supported (`compiled/util.cpp:160-175`, `bool_op` requires `out.id() == BOOL8`, which
`binop_output_type` guarantees for every predicate op); `copy_if_else(column, column, mask)`
requires `have_same_types` (`copying/copy.cu:355-368`), and the no-else fill is built from
`last_then->type()` with the scale preserved for fixed-point
(`scalar/scalar_factories.cpp:131-139`, in the tree since v21.12, so on shad-gpu's 25.02); a null
mask entry selects the else side, which is SQL's `CASE WHEN NULL`. The `then` column is
`DECIMAL128` at the CASE because the loader widens narrow decimals at the scan
(`cpp/src/operators/scan.cpp:98-108`); its scale is the declared one, so the state column's type is
the `Decimal128(17,2)` the plan declares, and the sink never sees a decimal.

**What has run on a device, piece by piece.** A string-literal equality through the column path:
`tpch/q19` at tp1-single, a green device cell, whose `GpuFilter` predicates
(`p_brand@1 = Brand#12`, `l_shipinstruct@4 = `DELIVER IN PERSON``) take `filter.cpp:27` →
`build_column`, and whose Inner join residual takes `join.cpp:366` → `build_column` — both the
`:609-616` arm above. A computed init argument through `arg_col` → `build_column`: q19's
`sum(l_extendedprice@0 * (1 - l_discount@1))` and q6's, both on the device. A search-form CASE
through `build_column_case`'s fold: `PlanExecutor.ProjectSqrtThroughTheColumnPath`
(`cpp/tests/gpu/test_plan_executor.cpp:718-771`), integer condition, with an ELSE. A grouped
decimal `SUM` merged across a shuffle: the walk's `SUM_BY_FLAG` at two lanes
(`peacockdb-core/tests/test_gpu_recipe_walk.rs:684-698`). The bare cuDF chain the init would run —
`binary_operation(string col, string_scalar, EQUAL, BOOL8)` → `copy_if_else` on decimals → groupby
`SUM` — is `TpchSf40.Q8` (`cpp/tests/gpu/test_tpch.cpp:703-717`), on shad-gpu. Unproven: the no-else
arm (`:790-795`) on a device, and the composition inside a `Partial`.

**The composition has in fact run once, and the tree says so without naming it.** T19 batch 12 ran
tpcds/q2 at tp1-single on a device and recorded #152 — "a join refusing its second probe batch"
(`corpus_cases.inc:201-203`; `llm-wiki/archive/archived-tasks.md:1832`, "`tpcds/q71` and
`tpcds/q2` refuse at `bp-tp1-single`"). That refusal is `build_copy`
(`peacockdb-core/src/executor/gpu_backend/join.rs:304-313`): the first probe batch gets the original
build handle and the join call runs; only the second is refused. The driver picks the runnable node
of smallest height (`executor/driver/scheduler.rs:1-7,79-85`), so the first probe batch's output
climbs `#11 CudfProject` → `#12 CudfAggregate{Partial}` → `GpuAggregateBatches` (an accumulator,
which holds it) before the merge at greater height forwards web_sales' sibling batch and the second
probe call refuses. The cpu section of that mode (`tp1-single-mini.cpu.txt` `== q2`) shows the shape:
the join's `in_rows=[[73049],[2160932]]` over a merge whose `batch_rows=[[719384,1441548]]`, two
probe batches, the first 719,384 rows of joined web_sales. Had #56 been live, the run would have
failed at `#12` with cuDF's text before probe batch 2 existed, and the cell would carry another
number. The raw error text is not preserved in the tree; the inference rests on the scheduler rule
and the recorded ticket.

**Relation to #55.** Same three layers, same closure shape (`55-proposal.md`); the difference is
which primitive the init evaluates — a decimal divide there, a string-conditioned CASE here. Nothing
in #163's plan touches either: the CASE is DataFusion's own `CaseExpr` translated with the type
DataFusion computed, and the CPU (`executor/cpu_backend/expr_physical.rs:86-103`) rebuilds it as a
`CaseExpr`, which is why q2's cpu cells are exact by identity.

## 3. Localized fix

No engine change. One pin on a device, one pin on the CPU tier, then the registry stops naming it.
Four edits.

### 3a. A recipe-walk test that pins the shape on a device — `peacockdb-core/tests/test_gpu_recipe_walk.rs`

Beside `SUM_BY_FLAG` (`:634`):

```rust
/// q2's aggregate shape without its joins: a sum whose argument is a search-form CASE
/// conditioned on a string equality, the `ELSE NULL` dropped by DataFusion's simplifier
/// as q2's golden shows, so the C++ takes the no-else null-fill arm. Two conditions over
/// one column, since q2 fans seven over one.
const SUM_BY_FLAG_CASE: &str = "SELECT l_linenumber, \
     sum(CASE WHEN l_returnflag = 'A' THEN l_extendedprice ELSE NULL END) AS a_price, \
     sum(CASE WHEN l_returnflag = 'R' THEN l_extendedprice ELSE NULL END) AS r_price \
     FROM lineitem GROUP BY l_linenumber";
```

and one test after `each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them`
(`:684-698`), the same shape at the same knobs:

```rust
/// The CASE runs in the init and nowhere else: the merge above it sums state. Two lanes,
/// so the state crosses a real merge. The compare is on rendered digits — a condition
/// evaluated against the wrong table, a null fill at the wrong scale or a string compare
/// the column path refused all fail here, and none can on a CPU.
#[tokio::test]
async fn a_sum_over_a_string_conditioned_case_is_evaluated_by_the_init_alone() {
    let calls = assert_walk_matches_datafusion(SUM_BY_FLAG_CASE, TWO_LANES).await;
    assert_eq!(times(&calls, FbKind::Aggregate { merge: false }), 2,
        "one init per lane evaluates the CASE: {}", trail(&calls));
    assert_eq!(times(&calls, FbKind::Aggregate { merge: true }), 4,
        "the state crossed both merges, as SUM_BY_FLAG's does: {}", trail(&calls));
}
```

Why it is clean of every other ticket: the group key is `Int64`, so no string is exported (#183
cannot fire, and the walk's digest keys on names anyway, `tests/common/result_text.rs:87-98`); the
sums are `Decimal128(25,2)` exported at 38 and compared as digits (#187 cannot fire — `SUM_BY_FLAG`
already passes on exactly that); no join (#152), no sort (the walk's `:788` refusal), two lanes
(`Repartition { lanes: 2 }` is in `PROVEN`, `:798-811`); `l_linenumber ∈ 1..7`, `l_returnflag ∈
{A, N, R}`, so every group has rows on both sides of each condition and the `NULL` arm is exercised
in every batch. The kinds it makes are all in `PROVEN`, so
`the_kinds_a_device_has_run_are_the_kinds_this_file_claims` needs no change; add
`(SUM_BY_FLAG_CASE, TWO_LANES)` to its list (`:816-828`) so the cover reads it.

Plan shape, from `SUM_BY_FLAG`'s at the same knobs: per lane one `CudfAggregate{Partial}` over one
batch, the shuffle on `[l_linenumber]`, a `Merge` per lane below and above it, a finalize per lane.
The `4` mirrors the assertion `:686-691` already makes for that shape; if a planner change moves
it, both tests move together, which is the point of asserting the same number.

### 3b. Two arms in the CPU-tier routing pin — `cpp/tests/cpu/test_executor.cpp`, `AstRouting.IsAstAble` (`:81-119`)

The gate that keeps the ticket's title mechanism unreachable is `is_ast_able`, and the test that
pins it names four decisions and neither of the two this ticket rests on. Add, in the same style:

```cpp
  // string column = string literal → column path (compute_column has no string output and
  // no string literal input on this route; build_column_binary carries it).
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_binary_col_str(b, fb::BinaryOp_Eq, "Sunday");
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::STRING})};
    EXPECT_FALSE(peacock::is_ast_able(expr, cudf::table_view{cols}));
  }
  // A CASE is never AST-able, whatever its branches hold.
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_case_int_gt(b);   // CASE WHEN c@0 > 0 THEN c@0 END
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::INT32})};
    EXPECT_FALSE(peacock::is_ast_able(expr, cudf::table_view{cols}));
  }
```

with two helpers beside `make_binary` (`:56-66`). The string literal must be built through
`fb::ScalarValueBuilder` — `add_type(fb::DataType_Utf8)`, `add_string_val(...)` — not the
positional `CreateScalarValue`, whose second parameter is `is_null` (`cpp/build/generated/
gpu_plan_generated.h:954-967`): `typed-nulls.md:76-93` records the gtest helpers that shipped one
position off. This runs on every push (`ctest -L cpu`, both legs) and needs no device.

### 3c. The registry and the ticket

- `testdata/cost-registry.csv:3`: `56 152` → `152`. The column names what still holds a cell off;
  the cost report resolves a number through all three ticket files
  (`cost-report/src/main.rs:440-460`), so it is safe either way.
- `llm-wiki/tickets.md:321-325`: remove #56; the Contents row (`:19`) drops to 13 and loses `#56`.
- `llm-wiki/archive/archived-tickets.md`, under Stale, with its anchor: "Stale <date>. Filed against
  the legacy executor's Final phase re-evaluating the init's argument; unreachable since the
  aggregate sequence — the merge takes state column refs (`translator/aggregate.rs:167-189`), the
  wire never writes `Final` (`wire/aggregate_writer.rs:79-80`), the C++ reads a merge's input
  positionally (`aggregate.cpp:564,577,706`) — and the init's CASE never enters the AST
  (`expr.cpp:406,419`). Pinned on a device by `a_sum_over_a_string_conditioned_case_…` in
  `test_gpu_recipe_walk.rs` and on the CPU tier by `AstRouting.IsAstAble`. q2's device cells stay on
  #152."
- `00-tickets.md:56` (scratch): the code-path column is wrong as written.

### 3d. Comments and pages that name #56 (no logic)

`peacockdb-core/src/plan/mod.rs:61`, `llm-wiki/architecture.md:60-63` and
`scripts/exec_model/operators/frame.py:29` name "#55/#56/#63" as the reason init and merge see
different tables. True as rationale; the numbers resolve to the archive. Leave. No spec lists #56
among walk refusals (grep over `llm-wiki/tasks/`), so nothing else moves.

### What it deliberately does not touch

`planner/translator/aggregate.rs`, `wire/`, `cpp/src/expr.cpp`, `cpp/src/operators/aggregate.cpp`
— nothing. Not `is_string_like_literal`'s comment ("cuDF AST has no string ops"), which may be stale
against 25.x but is the reason for a conservative route this proposal relies on. No `bug_` test: the
behaviour is right. No corpus cell for the probe: through the corpus tier it would go red at the
unload on #187 (two `(25,2)` sums exported at 38), which is not this ticket's. No row in
`operator-cases.md`'s `GpuAggregate` line (`llm-wiki/tasks/operator-cases.md:34`): that spec is
approved to build and is the helper's; when it builds, "an argument that is a CASE over a string
equality, no ELSE" is a one-batch case there and the second pin, but 3a is not a prerequisite for it
and this proposal does not edit the spec.

### CPU and GPU agreement

The CPU runs DataFusion's `CaseExpr` inside DataFusion's `AggregateExec` (`cpu_backend/
expr_physical.rs:86-103`) and matches the oracle by identity — q2's five green cpu cells. The
device's chain is the section-2 trace; 3a is its proof, on the same IR, in which the CASE sits on
the init and the merge is `sum(state)`.

### Hacks-audit scaffolding

None for #56 in either pass (grep `56` over `cpp/src`, `peacockdb-core/src`, the test trees: no
branch, no `bug_` test, no fixture). Adjacent and untouched: finding 10 (`aggregate.cpp`'s dead
spellings and the `distinct` guard) and finding 5 (`stddev_ddof` re-derived from a name) sit in the
same file off this path. The audit did not read `expr.cpp:470-945`; the reading above is the first
of `build_column_case` since, and found no scaffolding there either.

## 4. Alternatives rejected

- Flip tpcds/q2's gpu tp1-single cell and read the driver's error — the single run the ticket
  asks for, and it has already happened (T19 batch 12, outcome #152). A repeat would confirm the
  ordering argument at the cost of a device run and leave nothing green in CI.
- A `bug_` test — no wrong behaviour to assert.
- Re-route string comparisons or CASEs into the AST now that cuDF 25.x carries `string_view` in
  `expression_evaluator.cuh` — a routing change with no ticket behind it, on a path four green
  device cells and q19's residual depend on; and whether 25.02 has it is unverified.
- A validator that a merge's arguments are state column refs — the one constructor never builds
  anything else (`:167-189`); a rule for a shape the type permits but no code makes.
- A gtest driving `CudfAggregate{Partial}` with the CASE over `tpch.minimal` — a third copy of
  what 3a proves against a real oracle; `operator-cases.md` is where that shape belongs once the
  harness exists.

## 5. Minimum corpus query

The probe in 3a, against tpch sf1 `lineitem`:

    SELECT l_linenumber,
           sum(CASE WHEN l_returnflag = 'A' THEN l_extendedprice ELSE NULL END) AS a_price,
           sum(CASE WHEN l_returnflag = 'R' THEN l_extendedprice ELSE NULL END) AS r_price
    FROM lineitem GROUP BY l_linenumber

Plans today at every mode; nothing in it is refused by the planner (a grouped sum over a translated
`CaseExpr`, as q2 already plans). The `ELSE NULL` is dropped before translation — q2's golden
renders `… THEN sales_price@0 END` and `plan_text/expr_text.rs:67-69` would print an ELSE it had —
so the wire carries no `else_expr` and the C++ takes the null-fill arm q2 takes. Backend: the device,
through the recipe walk at `TWO_LANES` (the merge above the init is what proves the CASE ran only
in the init); the CPU is exact by identity. Predicted: seven rows equal to DataFusion's to the cent.
What would reopen #56, each naming its step in the walk's trail: a non-zero `rc` on a
`CudfAggregate{Partial}` carrying `Unsupported operator for these types` (the binop),
`Both inputs must be of the same type` (the fold's null fill at another scale),
`unsupported scalar type in column path` (the literal), or seven rows whose sums disagree (the
mask's null handling).

Through the corpus tier instead, the same query at tp1-single on a device fails at the unload —
`expected Decimal128(25, 2) but found Decimal128(38, 2)`, #187's text — and that failure is itself
the answer: the `Partial` at `#…` returned 0 before it. A failure at the `Partial` is #56 alive.

The run as the ticket names it, if wanted anyway: flip `corpus_cases.inc:207`'s gpu modes to
`tp1_single` and the registry's `gpu_tp1_single` cell together, `PCK_TEST_FILTER=q2` through
`scripts/build-test-shadgpu.sh`, read the driver's error. It must be `#10 CudfHashJoin{Inner}` (or
`#30`), `probe batch 2 has no build side left … (#152)`. Revert both edits after.

## 6. Cells re-enabled

None now. `tpcds/q2` gpu × 5 stay off on #152 at every mode (the union under the join, two probe
batches at tp1-single, more elsewhere). Dropping `56` from the row is what lets q2 come back with no
further triage when #152 clears — today a reader of `56 152` looks for a second fix that does not
exist. Behind #152 for q2, and not this ticket's: the per-batch lists at `#10`/`#30` — DataFusion
chunks the join's 2,153,556 output rows into 8192-row batches on the CPU
(`tp1-single-mini.cpu.txt` `== q2`, `batch_rows=[[8192,8192,…]]`) and the device emits one table per
probe call, the unticketed divergence `152-review.md` F2 names; and, unproven on a device, the
final project's `Decimal128(17,2) / Decimal128(17,2)` pre-scaled to DataFusion's declared scale then
`CAST … AS Float64` and `round` through `build_column_scalar_fn`. The walk gains one query; the walk
is not a registry cell.

## 7. Risks and unknowns

- The composition — a CASE with a string condition inside a `Partial` argument, its state merged
  above — is asserted by inference (section 2's scheduling argument over a recorded #152) and not by
  a preserved error text. 3a's first run is the direct observation. If it goes red at a
  `CudfAggregate{Partial}`, the ticket is live with a mechanism other than the recorded one, and the
  message names the step.
- The no-else arm (`expr.cpp:790-795`) has no device run behind it; its correctness is read off cuDF
  source (`make_default_constructed_scalar` preserving fixed-point scale since v21.12,
  `copy_if_else`'s `have_same_types`). 3a is the first.
- shad-gpu is 25.02; the tree read is 25.06. The three contracts cited are unchanged between them by
  their history in that tree; not diffed against a 25.02 checkout.
- The `4` merge calls assume the probe plans exactly as `SUM_BY_FLAG` does at `TWO_LANES`. An `Int64`
  key instead of a string one changes no lowering rule read here (`translator/aggregate.rs:317-377`),
  but the plan was not rendered.
- A neighbouring shape this reading noticed and did not chase: `string_col = string_col` with no
  literal passes `is_ast_able` (`:419-435`: equal `STRING` types, both refs) and would reach
  `compute_column`. No corpus query puts that on a device today and no test pins either outcome. Not
  #56's shape; worth a line in `operator-cases.md`'s `GpuFilter` row when that spec is next edited.
- 3b's helpers are sketched, not compiled; the `ScalarValueBuilder` form is the one `typed-nulls.md`
  prescribes and `test-layout.md` may move the walk file before 3a lands.

**#57 and #63, the neighbours.** #63 shares #56's mechanism exactly one level up: an expression the
legacy executor evaluated against a table it was not written for — the wrong phase here, separately
evaluated scalar subqueries there. Today's q9 plan (`tp1-single.plans.txt` `== q9`) puts every CASE
branch as a `ColumnRef` into one 1-row cross-joined table, and no `build_column` arm answers other
than `table.num_rows()` rows, so the "1-row branch vs other-sized branch" pair cannot be built; its
closure is this proposal's shape — one observation, no code — but q9 sits behind #163 on both
engines, so the observation is a `GpuProject` case, not a corpus run. #57 is different in kind: a
real unimplemented arm behind a guard (`expr.cpp:779`) with a recorded wrong answer, sharing only
the site (`build_column_case`) and the pin location (`operator-cases.md:29`, "CASE in both forms",
which already names it); q39 is behind #163 too. Nothing here touches either.

## 8. Complexity

**S.** One Rust test file gains a constant and a test (~25 lines); one C++ CPU-tier gtest gains two
cases and two helpers (~35 lines); one CSV cell loses a number; one ticket moves to the archive. No
engine code, no C ABI, no `.fbs`, no wire format, no declared-schema contract, no golden
regenerated (`recipe-payloads.txt` is untouched: no new payload query). The cost is one shad-gpu run
of the walk and one `ctest -L cpu`.
