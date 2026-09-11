# #45 — proposal: fixed the day after filing; the ticket outlived its fix. Close it with one device pin

Read at master 188c23ce. Paths relative to `/media/data/peacockdb`. The filing commit read through
`git show 2d07e908:…`, the fix through `git show 257377b0`, the history of q24's device cell through
`git log -S`. cuDF read from `/home/dmitry/cudf` (v25.06 tree); DataFusion from the 45.0.0 crates
in `~/.cargo/registry`. Nothing built or run.

**Verdict up front.** The error the ticket records was a `cudf::cast` to STRING in `build_column`'s
`CastExprNode` arm, hit by q24's join *residual filter* — not a join key. It was fixed on
2026-06-11, one day after filing, by commit 257377b0 ("q24 goes green with only this fix"), and
q24 then ran green on the legacy device tier for three months until that tier was deleted
(3c0750ee, 2026-09-08). The fix is in today's `cpp/src/expr.cpp:912-934`, still citing #45. The
ticket survived because the GitHub issue was never closed and the registry's `tickets` column
carried `45` from its first commit even while q24's device cell was `enabled`. No join-key cast
mechanism exists on either side, and none ever did. What is missing is a pin in the current tree.

## 1. Issue

`llm-wiki/tickets.md:309-313`: "q24 GpuHashJoin: 'Unary cast type must be fixed-width'. A join-key
cast targets string; cuDF's unary cast rejects non-fixed-width. Diagnose the emitted cast and
handle string keys by hashing rather than casting (`cpp/src/operators/join.cpp`)." Filed from
2d07e908 (2026-06-10, "TPC-DS Bucket D"), whose test file said in place: "q24: NestedLoopJoin
works, but blocked by a GpuHashJoin cuDF failure ('Unary cast type must be fixed-width' — a cast
to a non-fixed-width/string type) — distinct blocker, not a join issue (issue #45)".

**Which cast, and on what.** Not a key. Both the filing commit's golden
(`git show 2d07e908:testdata/plans-tpcds.sf1/q24.txt`, lines 16 and 71) and today's
(`testdata/goldens/tpcds.sf1/tp1-single.plans.txt:2287` and `:2312`, the same node in q24's two
CTE copies) render the join as

    GpuHashJoin: join_type=Inner, on=[(ca_address_sk@0, c_current_addr_sk@9), (ca_zip@2, s_zip@3)],
      filter=c_birth_country@probe:12 != CAST(upper(ca_country@build:3) AS Utf8View), …

Every key is a bare column. The one cast is in the residual filter, on the build side, wrapping
`upper(ca_country)`, and its target is `Utf8View` — cuDF `STRING` (`cpp/src/expr.cpp:87-89`). It is
DataFusion's own coercion: under DataFusion 45 `upper` returns `Utf8` for a `Utf8View` argument
(`datafusion-functions-45.0.0/src/utils.rs:34-70`, `get_optimal_return_type`; `string/upper.rs:75-77`),
and the comparison against the `Utf8View` column casts the function's result up. The same cast is
the only string-targeted cast in the whole corpus: `grep "AS Utf8" testdata/goldens/*/*.plans.txt`
finds q24's two lines in each of the five tpcds files and nothing in tpch or
`recipe-payloads.txt`.

**What it disables today: nothing.** `testdata/cost-registry.csv:25` carries `45 163` on
`tpcds/q24`; all ten cells are `disabled`, and `peacockdb-core/tests/common/corpus_cases.inc:283-288`
attributes every one to #163 ("All seven refuse on #163 at every mode"). q24 *is* in the
exec-model corpus (`scripts/exec_model/tests/plans_tpcds_weeks.py:196-262`), which lowers the
predicate as `Binary("!=", Col("c_birth_country"), Upper(Col("ca_country")))` with no cast, pandas
having one string type. No code references #45 except the fix's own comment (`expr.cpp:919`); two
specs do (§3d).

Corrections to `00-tickets.md:60`: the "one-line problem" ("join-key cast") and the code-path
column ("`join.cpp`, hash string keys instead of casting") both name a mechanism that does not
exist; the cast is a filter expression and its arm is `expr.cpp`.

## 2. Root cause

**At the filing commit.** `cpp/src/plan_executor.cpp` at 2d07e908: the Inner join's residual
filter was evaluated by `build_column(join->filter(), inter)` (`:1709`), and `build_column`'s
`CastExprNode` arm (`:947-958`) was

    auto inner = build_column(cast->expr(), table);
    auto target_id = fb_to_type_id(cast->target_type());
    cudf::data_type target = target_id == DECIMAL128 ? {target_id, -scale} : {target_id};
    return cudf::cast(inner->view(), target);

with no string case. `cudf::cast` opens with
`CUDF_EXPECTS(is_fixed_width(type), "Unary cast type must be fixed-width.")`
(`/home/dmitry/cudf/cpp/src/unary/cast_ops.cu:454`), and `STRING` is not fixed-width. That is the
message the ticket quotes, thrown from inside `execute_hash_join` — hence "GpuHashJoin" in the
title — for a cast that never touched a key. The routing was already the column path: `is_ast_able`
at that commit (`:498-505`) sent any cast whose target is not INT64/FLOAT64 there, as today.

**Fixed the next day.** 257377b0 (2026-06-11) added to that arm:

    if (target_id == cudf::type_id::STRING) {
      if (inner->type().id() == cudf::type_id::STRING) return inner;
      throw std::runtime_error("cast to STRING from a non-string type not supported in column path");
    }

and re-enabled q24 on the device: "q24 (#45): … string->string cast as identity … q24 goes green
with only this fix. Re-enabled." The commit message attributes the cast to "q24's `s_zip = ca_zip`
join" keys — the golden at its own parent disproves that; the code change is the one arm, and it
is the arm the filter reaches. The message names #45 without closing it.

**Never re-filed, never re-tested, still open.** q24's device line stayed enabled through every
harness rewrite: `gpu_result_test_tpcds!(test_gpu_tpcds_q24, "q24")` (257377b0), `gpu_test!(tpcds, 1,
q24, tp1_standard, golden_exact)` (`test_gpu.rs:162` at 3435b45b, 2026-07-31 — the same commit that
created `cost-registry.csv` with `full_table_gpu=enabled` for q24 *and* `45` in its tickets
column), `gpu_case!(tpcds, 1, q24, full_table_tp1_standard, golden_exact)` (`gpu_cases.inc:88` at
3c0750ee^). 3c0750ee (2026-09-08) deleted the legacy modes and with them the only tier that ran the
string cast on a device; T19's rollout of the current corpus tier then met #163 at every mode
(`corpus_cases.inc:283-288`) before the join was reached. The ticket text was migrated verbatim
from the issue on 2026-08-04 (90732a36) with the fix already two months old.

**Today's path for q24's filter, read through.** `cpp/src/operators/join.cpp:348-368`: the join
gathers `[left…, right…]`, builds the filter's intermediate view from `filter_columns`, and calls
`build_column(join->filter(), inter)` (`:366`). Then in `cpp/src/expr.cpp`:

    build_column(NotEq)                     is_ast_able (:403): BinaryExprNode arm —
                                             lt = STRING (ColumnRef), rt = STRING (CastExprNode,
                                             :385-386); equal, not decimal; recurse →
                                             is_ast_able(Cast): target STRING ∉ {INT64, FLOAT64}
                                             → false (:439-447)  → column path
      build_column_binary (:579)             neither side a literal → both to columns (:626-630)
        lcol = ColumnRef copy                (:839-858), STRING
        rcol = build_column(Cast)            (:912)
          inner = build_column(ScalarFn)     → build_column_scalar_fn (:909)
            "upper" → cudf::strings::to_upper(:734-738), STRING
          target STRING, inner STRING → return inner (:921-923)   ← the #45 arm
        binop_output_type(NotEq, STRING, STRING) = BOOL8 (:555-557)
        cudf::binary_operation(col, col, NOT_EQUAL, BOOL8)
    apply_boolean_mask (join.cpp:367)

cuDF supports the last call: `NOT_EQUAL` is a `bool_op` for any operand pair the operator is
defined on, `string_view` included (`binaryop/compiled/util.cpp:170-186`), and the column-column
string arm is `binary_ops.cu:170-180`. So the recorded mechanism is unreachable, and every step of
the live path but two has run on a device: the residual-filter machinery itself (filter_columns,
`build_column` over `inter`, the mask) is tpch/q19's green tp1-single cell (`corpus_cases.inc:25`,
whose Inner join carries a residual filter — `tpch.sf1/tp1-single.plans.txt`, q19's `GpuHashJoin`
line), and string-column comparisons through `binary_operation` are q19's too. Unproven in the
current tree: the identity arm `:921-923` and `to_upper` `:738`. Both ran on the legacy tier for
three months; neither has a test since.

**No join-key cast mechanism exists, on either side.** Rust: `hash_join`
(`planner/translator/nodes.rs:246-300`) reads each key through `column_ordinal_of`
(`translator/common.rs:72-83`), which refuses anything but a bare column at plan time ("join key …
is an expression rather than a column"). DataFusion never sends one: an expression key is wrapped
in a `ProjectionExec` beneath the join and the join's `on` becomes columns
(`datafusion-45.0.0/src/physical_planner.rs:908-926`, `wrap_projection_for_join_if_necessary`), so a
key coercion would arrive as a `GpuProject` carrying `CAST(... AS Utf8View)` — and a project's
expressions take the same `build_column` arm (`project.cpp`), the same identity. C++:
`execute_hash_join` accepts `ColumnRef` keys only (`join.cpp:52-63`, "only ColumnRef keys
supported") and casts nothing. The "hash key normalization" `architecture.md` mentions is the
shuffle kernel's (`spark_hash_partition.cu:131-140`, "FOR HASHING ONLY") and never sees a join. The
ticket's proposed fix — "hash string keys rather than casting" — is aimed at a mechanism that did
not exist at the filing commit either (`plan_executor.cpp:1413-1425` at 2d07e908, the same
ColumnRef-only loop).

**The CPU twin.** The join filter is rebuilt as DataFusion's own `CastExpr` over a
`ScalarFunctionExpr` (`executor/cpu_backend/expr_physical.rs:70-74`, `:110-121`) inside a
`HashJoinExec`, so the CPU matches the oracle by identity — which is why q24's cpu cells were
`data_fusion_exact` before #163 and will be again after it.

**What is live nearby and is not #45.** `expr.cpp:924-925` refuses a *non-string* → STRING cast
("cast to STRING from a non-string type not supported in column path"): `cudf::cast` cannot make
strings, and no `cudf::strings::from_*` conversion is wired. No corpus query has one. That is the
refusal `declared-schemas.md:218` row 11 (`SELECT CAST(n_nationkey AS VARCHAR) FROM nation`) will
observe, and the row attributes it to #45; it is a different shape with a different cause (§3d, §7).

## 3. Localized fix

No engine change. One device pin, one CPU-tier routing pin, the registry and the ticket. Four edits.

### 3a. A recipe-walk test that pins q24's filter shape on a device — `peacockdb-core/tests/test_gpu_recipe_walk.rs`

The walk is the right instrument: it drives the plan the writer emits against DataFusion on the
same SQL, its result compare digests names and rendered rows rather than types
(`tests/common/result_text.rs:92`, `:117-140`), so `Utf8View` declared against `Utf8` exported
(#183) does not redden it — `INNER_JOIN` (`:630`) already exports two string columns this way.

Beside `INNER_JOIN` (`:630-631`):

```rust
/// q24's residual-filter shape: a string column compared with `upper(...)` of another.
/// DataFusion 45's `upper` returns `Utf8` for a `Utf8View` argument, so the comparison
/// coerces with `CAST(upper(r_name) AS Utf8View)` — the one string-targeted cast in the
/// corpus. `>` rather than q24's `<>`: on this data `<>` keeps all 25 rows, `>` drops the
/// four Middle East nations that sort below their region, so a filter never applied shows.
const INNER_JOIN_STRING_CAST: &str = "SELECT n_name, r_name FROM nation JOIN region \
     ON n_regionkey = r_regionkey AND n_name > upper(r_name)";
```

and one test after `an_inner_join_matches_the_oracle_as_a_multiset` (`:658-661`):

```rust
/// A cast to a string type reaches the column path's identity arm (`expr.cpp:921`) and not
/// `cudf::cast`, which has no STRING target. The plan text is asserted first so a DataFusion
/// whose `upper` answers `Utf8View` — and so emits no cast — reddens this rather than
/// passing with nothing to prove.
#[tokio::test]
async fn a_cast_to_a_string_type_in_a_join_filter_is_an_identity_on_a_device() {
    let ctx = context(1).await;
    let plan = ctx.sql(INNER_JOIN_STRING_CAST).await.expect("datafusion plans it")
        .create_physical_plan().await.expect("datafusion lowers it");
    let (tree, _) = planner::plan(&plan, ONE_LANE).expect("this mode plans it");
    let text = peacockdb_core::plan_text::render_plan(tree.as_ref());
    assert!(
        text.contains("AS Utf8View)"),
        "the shape under test is a cast to a string type, and the plan carries none:\n{text}"
    );
    let calls = assert_walk_matches_datafusion(INNER_JOIN_STRING_CAST, ONE_LANE).await;
    assert_eq!(
        times(&calls, FbKind::HashJoin { join_type: JoinType::Inner }), 1,
        "the filter rides the one Inner call: {}", trail(&calls)
    );
}
```

Expected answer: 21 of the 25 (nation, region) pairs — every nation but EGYPT, IRAN, IRAQ and
JORDAN, whose names sort below `MIDDLE EAST`; the oracle compare asserts it, the number is for
the reader. Every kind it makes (`Scan`, `HashJoin{Inner}`, possibly `PlainProject`) is already in
`PROVEN` (`:798-811`), so `the_kinds_a_device_has_run_are_the_kinds_this_file_claims` needs no new
entry; add `(INNER_JOIN_STRING_CAST, ONE_LANE)` to its query list (`:816-828`) so the cover reads
it. `render_plan` is `plan_text/mod.rs:24`, already a public facade item; `planner::plan` and
`context` are what `walk` (`:546-557`) uses.

Why it is clean of every other ticket: both tables are one lane and one batch (`nation` 25 rows,
`region` 5), so one probe batch and no #152; no aggregate (#163, #185); no decimal (#187); the walk
does not go through the unload (#183); the join type is Inner (`capability`, `plan/join.rs:416`,
streams with a filter). What would reopen it, each naming its step in the trail: a non-zero `rc`
on the `CudfHashJoin{Inner}` carrying `Unary cast type must be fixed-width` (the arm gone),
`cast to STRING from a non-string type` (the `upper` result typed as something other than STRING),
`unsupported CAST target type` (the cast routed to the AST), or 25 rows against the oracle's 21
(the mask not applied).

### 3b. One arm in the CPU-tier routing pin — `cpp/tests/cpu/test_executor.cpp`, `AstRouting.IsAstAble` (`:81-119`)

The precondition of 3a's path is that a STRING-targeted cast is routed *off* the AST, whose cast
arm throws for every target but INT64/FLOAT64 (`expr.cpp:295-307`). `is_ast_able`'s cast arm
(`:439-447`) decides it and the test that pins the function names four decisions and not this one.
Add, in the file's style, with a helper beside `make_binary` (`:56-66`):

```cpp
// Build `CAST(c@0 AS <target>)`.
flatbuffers::DetachedBuffer make_cast_of_col(flatbuffers::FlatBufferBuilder& b,
                                             fb::DataType target) {
  auto cr = fb::CreateColumnRef(b, 0, b.CreateString("c"));
  auto inner = fb::CreateExpr(b, fb::ExprNode_ColumnRef, cr.Union());
  auto cast = fb::CreateCastExprNode(b, inner, target);
  auto e = fb::CreateExpr(b, fb::ExprNode_CastExprNode, cast.Union());
  b.Finish(e);
  return b.Release();
}
```

and in the test body:

```cpp
  // CAST(string AS Utf8View) → column path: the AST casts only to INT64/FLOAT64, and the
  // string→string identity lives in build_column (expr.cpp, the #45 arm).
  {
    flatbuffers::FlatBufferBuilder b;
    auto buf = make_cast_of_col(b, fb::DataType_Utf8View);
    auto* expr = flatbuffers::GetRoot<fb::Expr>(buf.data());
    std::vector<cudf::column_view> cols{typed_col(cudf::data_type{cudf::type_id::STRING})};
    EXPECT_FALSE(peacock::is_ast_able(expr, cudf::table_view{cols}));
  }
```

`CreateCastExprNode(b, inner, target)` is the positional form `test_plan_executor.cpp:78-83`
already uses. Runs on every push (`ctest -L cpu`, both legs), no device.

### 3c. The registry and the ticket

- `testdata/cost-registry.csv:25`: `45 163` → `163`. The column names what holds a cell off; the
  widget resolves a number through both ticket files (`cost-report/src/main.rs`,
  `TicketIndex::path_for`), so the edit is safe either way.
- `llm-wiki/tickets.md:309-313`: remove #45; the Contents row (`:19`) drops to 13 and loses `#45`.
- `llm-wiki/archive/archived-tickets.md`, under **Done**, with its anchor: "**Done 2026-06-11**, by
  257377b0, recorded <date>: the cast was `CAST(upper(ca_country) AS Utf8View)` in q24's join
  residual filter, not a key; `build_column`'s cast arm called `cudf::cast` with a STRING target,
  which cuDF refuses as non-fixed-width. The arm now returns a string input unchanged for a string
  target (`cpp/src/expr.cpp:921-923`). q24 ran green on the legacy device tier from that commit
  until the tier was dropped (3c0750ee); the issue was never closed and the registry carried the
  number. Pinned on a device by `a_cast_to_a_string_type_in_a_join_filter_is_an_identity_on_a_device`
  in `test_gpu_recipe_walk.rs` and on the CPU tier by `AstRouting.IsAstAble`. q24's device cells stay
  on #163, then #152 and #183/#187." Done rather than Stale: a change was made for it and it names
  the ticket.
- `00-tickets.md:60` (scratch): the problem and code-path columns are wrong as written.

### 3d. Comments and pages that name #45

- `cpp/src/expr.cpp:916-920`, the arm's own comment, says "DataFusion's coercion of two char keys" —
  the fix commit's misattribution, carried. Comment-only, the coordinator's: "…so DataFusion's
  coercion of a string expression to a column's string type (q24's `<> upper(ca_country)`) has
  nothing to convert." The `(#45)` may stay; numbers resolve to the archive.
- `llm-wiki/tasks/declared-schemas.md:218` row 11 and `declared-schemas-derived.md:101` name #45
  for `SELECT CAST(n_nationkey AS VARCHAR) FROM nation` — "a cast target that is not fixed-width,
  refused at the join". The refusal that query meets is `expr.cpp:924-925`, the unimplemented
  non-string → STRING conversion, in a project and not a join; the fixed-width throw is guarded
  off by `:921`. Both specs are `approved to build` and the helper's. The row should keep its query
  and change its attribution: either a new ticket for "the device refuses a non-string → STRING
  cast the CPU answers" — it is a refusal a user can reach, so it qualifies — or "no ticket:
  unimplemented conversion, `expr.cpp:924`", if the human prefers it unfiled until a query needs it.
- `llm-wiki/tasks/walk-drives-every-plan.md:125` and `walk-drives-every-plan-impl.md:106` list #45
  among "four known refusals" that "abort the walk today". No walk query produces a string cast, so
  the walk has never met #45's shape; after 3a it carries one that passes. Drop #45 from both
  lists; if that plan wants a refusal to drive, `CAST(n_nationkey AS VARCHAR)` still refuses at
  `expr.cpp:924` under whatever number row 11 above settles on. `55-proposal.md` §3c asks the same
  edit for #55 on the same two lines.

### What it deliberately does not touch

`cpp/src/expr.cpp`, `cpp/src/operators/join.cpp`, `planner/`, `wire/` — nothing. Not the
non-string → STRING refusal (`:924`): implementing it is a capability with no ticket and no corpus
user. Not the semi/anti/mark filter path, which hands its predicate to `build_expr` unconditionally
(`join.cpp:98`, `:233`) and would throw `unsupported CAST target type` on a string cast — a
neighbouring gap with no corpus query, named in §7. No `bug_` test: the behaviour is right. No
corpus cell for the probe: through the corpus tier it goes red at the unload on #183 (two
`Utf8View` columns), which is not this ticket's. No row in `operator-cases.md`: its `GpuHashJoin`
line (`:37`) already includes "a residual filter where the matrix allows one", and "a cast" sits on
the `GpuProject` line (`:29`); when that task builds, a string-targeted cast is one case on each,
and 3a is not a prerequisite for it.

### CPU and GPU agreement

The CPU evaluates DataFusion's `CastExpr(Utf8 → Utf8View)` over DataFusion's `upper` inside
DataFusion's `HashJoinExec` and matches the oracle by identity. The device's chain is the trace in
§2, whose only type-changing step is arrow's `Utf8 → Utf8View`, a distinction cuDF does not have
(`fb_to_type_id`, `expr.cpp:87-89`). 3a is the proof on the same IR.

### Hacks-audit scaffolding

None for #45 in either pass (`grep -rn "45" cpp/src peacockdb-core/src peacockdb-core/tests
cpp/tests` finds only the arm's comment). The identity arm is not a workaround: it states a fact
about cuDF's type system, and the non-string refusal beside it is a named unimplemented
conversion, not a bandaid. Adjacent and untouched: hacks-audit finding 12 (the result comparator
hashes names and no types), which is precisely what lets 3a compare `Utf8` against `Utf8View`; if
the digest ever hashes types again, 3a needs the same allowance the walk's other string exports
will need.

## 4. Alternatives rejected

- Flip `tpcds/q24`'s gpu tp1-single cell and read the error — refused on #163 before any join is
  reached; after #163, the unload on #183/#187. The join never runs through that tier today.
- A gtest in `cpp/tests/gpu/test_plan_executor.cpp` mirroring q24's join with the filter over
  `tpch.minimal` — a third copy of what 3a proves against a real oracle, with a hand-built fb
  buffer that pins the writer's shape by assumption rather than by planning it.
- A `bug_` test — no wrong behaviour to assert.
- Implement `CAST(<non-string> AS VARCHAR)` on the device (`cudf::strings::from_integers` and
  friends) while here — a capability, not this ticket, and no corpus query needs it.
- Route string-targeted casts into the AST — cuDF's AST has no string cast; the routing is right.
- Keep #45 open as the number for the non-string refusal — the ticket's title, mechanism and cited
  file are all wrong for that defect; a reader would fix the wrong thing.

## 5. Minimum corpus query

The probe in 3a, against tpch sf1 `nation` and `region`:

    SELECT n_name, r_name FROM nation JOIN region
    ON n_regionkey = r_regionkey AND n_name > upper(r_name)

Plans today at every mode: an Inner `GpuHashJoin` with a residual filter, both inputs one lane
(both under `SMALL_TABLE_BYTES`), the filter `n_name@… > CAST(upper(r_name@…) AS Utf8View)` as
q24's. Nothing in it is refused by the planner (`capability(Inner, true)` streams). Backend: the
device, through the recipe walk at `ONE_LANE`; the CPU is exact by identity. Predicted: 21 rows,
equal to DataFusion's. Wrong behaviour it shows today: none — that is the finding. What it would
have shown at 2d07e908: the join call returning non-zero with `Unary cast type must be
fixed-width`.

Through the corpus tier instead, the same query at tp1-single on a device fails at the unload with
#183's text (`Utf8` exported against `Utf8View` declared) — and that failure is itself the answer,
since the join returned 0 before it. A failure at the join is #45 alive.

## 6. Cells re-enabled

None now. `tpcds/q24` gpu × 5 and cpu × 5 stay off on #163. Behind #163, q24's device cells meet
#152 at the four multi-batch modes (store_sales is the probe of every join) and, at tp1-single,
#183 (three `Utf8View` sink columns) and #187 (`sum(ssales.netpaid)` declared `Decimal128(27,2)`)
at the unload — none of them this ticket's. Dropping `45` from the row is what lets q24 come back
with no further triage when those clear; today a reader of `45 163` looks for a join fix that
does not exist. The walk gains one query; the walk is not a registry cell.

## 7. Risks and unknowns

- The identity arm and `to_upper` have no device run behind them in the current tree; the evidence
  is the legacy tier's green q24 from 257377b0 to 3c0750ee and a reading of the code. 3a's first
  run is the direct observation. If it goes red at the `CudfHashJoin{Inner}`, the message names
  which step, and the ticket is live with a mechanism other than the recorded one.
- 3a's plan-text assertion rests on DataFusion 45's `upper` returning `Utf8`. A DataFusion upgrade
  that answers `Utf8View` removes the cast from the plan, and the test goes red by design; the
  right response then is to find another expression the version still coerces, or to retire the
  pin with the shape.
- The `>` predicate's 21 rows were counted by hand from the standard TPC-H nation and region
  names; the oracle compare does not depend on the count, only the doc comment does.
- Neighbour, not chased: a LeftSemi/LeftAnti/LeftMark join whose residual filter carries a string
  cast (or any string op) goes to `build_expr` unconditionally (`join.cpp:98`, `:233`) and would
  throw on a device. `EXISTS (… AND c_name <> upper(o_comment))` is the shape. No corpus query has
  it; no test pins either outcome. Worth a line on `operator-cases.md`'s `GpuHashJoin` row when that
  spec is next edited.
- Neighbour, not chased: the non-string → STRING refusal at `expr.cpp:924` is a real device
  capability gap the CPU does not have; `declared-schemas.md` row 11 will observe it and needs a
  number or a "no ticket" note to attribute it to (§3d).
- cuDF read at 25.06; shad-gpu runs 25.02. `cudf::cast`'s fixed-width precondition and the
  string `binary_operation` arms both predate 25.02 by years and were not diffed.
- 3a and 3b are sketched, not compiled. `test-layout.md` may move `test_gpu_recipe_walk.rs` before
  3a lands; the test goes with the file.

## 8. Complexity

**S.** One Rust test file gains a constant and a test (~30 lines) and one entry in a list; one
C++ CPU-tier gtest gains a helper and a case (~20 lines); one CSV cell loses a number; one ticket
moves to the archive; two spec lines and one code comment are the helper's and coordinator's to
correct. No engine code, no C ABI, no `.fbs`, no wire format, no declared-schema contract, no
golden regenerated (`recipe-payloads.txt` is untouched — the walk's queries are not payload
queries). The cost is one shad-gpu run of the walk and one `ctest -L cpu`.
