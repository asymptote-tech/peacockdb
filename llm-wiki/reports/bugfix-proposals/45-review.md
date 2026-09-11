# #45 — review of the proposal

Read at master 188c23ce. Every `file:line` below was opened; `git show` for 2d07e908, 257377b0,
3435b45b, 3c0750ee^ and 90732a36; DataFusion 45.0.0 / datafusion-functions 45.0.0 /
arrow-schema 54.3.1 from the cargo registry; cuDF from `/home/dmitry/cudf`. Nothing built or run.

## 1. Verdict

**Needs changes.** The diagnosis is right and verified end to end — the ticket records a fixed
defect, the mechanism it names never existed, the arm is live and cited. The one code artifact
the proposal adds, the device pin, asserts a plan shape that `183-proposal.md` (same batch, root
cause accepted by `183-review.md`) removes from every plan on purpose, and neither proposal says
so. The pin has to be written in a form that survives #183 or it is red on its first run after it.

## 2. Findings

### F1 — the pin's plan-text precondition is deleted by #183 (blocking)

3a asserts `text.contains("AS Utf8View)")` before walking, and its doc says a plan without the
cast "reddens this rather than passing with nothing to prove". The cast exists only because the
column is `Utf8View` and `upper` answers `Utf8`:

- `datafusion-functions-45.0.0/src/utils.rs:34-70` — `utf8_to_str_type` maps `Utf8View` input to
  `Utf8` output; `string/upper.rs:75-77` uses it.
- `datafusion-expr-common-45.0.0/src/type_coercion/binary.rs:1145-1157` — `string_coercion`:
  `(Utf8View, Utf8) → Utf8View`, `(Utf8, Utf8) → Utf8`.

`183-proposal.md` §3a sets `schema_force_view_types = false`, after which every parquet string is
`Utf8`, `upper(Utf8)` is `Utf8`, the comparison coerces to nothing, and there is no cast. The 183
proposal says this itself (§3d table, `.plans.txt` row: "tpcds q24's join filter … loses its cast
(the only `AS Utf8View` in any golden)"). `183-review.md` accepts that root cause and the two-line
fix. So whichever of the two lands second breaks the other: #45 first, and #183's golden
regeneration makes 3a red by design; #183 first, and 3a is red on its first shad-gpu run. §7 of
this proposal names only "a DataFusion upgrade" as the way the cast disappears, and does not name
#183. The proposal's §1 "the same cast is the only string-targeted cast in the whole corpus" also
becomes "there is none" after #183 — so the shape the pin chose is one the corpus stops producing.

Correction: write the cast explicitly so the shape does not depend on which reader option the
session carries. `arrow_cast(upper(r_name), 'Utf8View')`:

- `datafusion-functions-45.0.0/src/core/arrow_cast.rs:148-171` — `ArrowCastFunc::simplify`
  rewrites the call to `Expr::Cast` whenever the source type differs from the target (and to the
  bare argument when it does not); `:117-135` types the call as the named target at analysis time,
  so coercion sees `Utf8View` on that side.
- `arrow-schema-54.3.1/src/datatype_parse.rs:443` — `"Utf8View"` parses.
- Today (`n_name: Utf8View`, `upper → Utf8`): the plan is `n_name@… > CAST(upper(r_name@…) AS
  Utf8View)`, byte-identical to the proposal's implicit form. After #183 (`n_name: Utf8`):
  `CAST(n_name@… AS Utf8View) > CAST(upper(r_name@…) AS Utf8View)` — two casts through the
  same identity arm, and the assertion still holds. `wire/serialize.rs:130` carries the tag either
  way, and the C++ folds it (`expr.cpp:87-89`).

The alternative the proposal rejected in §4 — a device gtest in `cpp/tests/gpu/test_plan_executor.cpp`
hand-building `CastExprNode(Utf8View)` over a string column — stops being "a third copy" once no
corpus SQL produces the shape; it is the honest pin of the C++ arm on its own. Either is
acceptable; the `arrow_cast` walk keeps the oracle compare and the trail, so it is the one
recommended below. Whichever is chosen, the consolidator must sequence #45 and #183 and say in
both which lands first.

### F2 — a third step is unproven in the current tree, not two (minor)

§2 says the residual-filter machinery and "string-column comparisons through `binary_operation`
are q19's too". q19's filter is `p_brand@build:1 = Brand#12 …` (`tpch.sf1/tp1-single.plans.txt`,
q19's `GpuHashJoin` line): column against a string *literal*, which `build_column_binary` takes on
the column-scalar fast path (`expr.cpp:626-641`). The probe's `n_name > CAST(upper(…))` is
column against column (`:644-648`), whose cuDF arm exists (`binaryop/compiled/binary_ops.cu:170-180`)
but has no device run in this tree either. So the list at §2 "unproven in the current tree" is the
identity arm, `to_upper`, and the column-column string compare. Nothing changes in the fix; the
risk statement in §7 should say three.

### F3 — the rewritten arm comment names a shape #183 removes (minor)

3d's replacement for `expr.cpp:916-920` cites "q24's `<> upper(ca_country)`" as the coercion
that reaches the arm. After #183 that predicate carries no cast, and the comment names a corpus
example that no longer produces the shape. Word it without a query: "a string→string cast —
DataFusion coercing between Arrow's string spellings, or a user's `arrow_cast` — has nothing to
convert". Comment-only, the coordinator's, as the proposal says.

### F4 — `build-test.md` counts are not listed (minor)

3a adds one test to `test_gpu_recipe_walk.rs`: the "Recipe walk on a device" row (`build-test.md:19`,
N = 10) becomes 11 and the grand total (`:7`, 1569 / Rust 1135) moves by one. 3b adds a case inside
an existing `TEST`, so the C++ row does not move. The proposal lists every other page edit and not
this one; the shared rule is that the commit keeps the page true.

### F5 — the neighbour's predicted message is wrong (minor)

§3 "What it deliberately does not touch" and §7 say a semi/anti/mark filter carrying a string cast
would throw `unsupported CAST target type` from `build_expr`. `build_expr` recurses into
`cast->expr()` first (`expr.cpp:295-297`), and `upper` is a `ScalarFunctionExprNode`, which the
AST builder has no arm for — it throws `unsupported expression node type` (`:310-312`) before the
cast is reached. The point (a neighbouring gap with no corpus query, not chased) stands; the
message a reader would grep for does not.

### F6 — the row-11 decision is already answered by the rules (minor)

3d leaves `declared-schemas.md:218` row 11's attribution as "either a new ticket … or no ticket".
`prompts.md` Shared rules: a refusal a user can reach is production behaviour and gets a ticket;
`SELECT CAST(n_nationkey AS VARCHAR) FROM nation` refuses on a device at `expr.cpp:924-925` and the
CPU answers it. Recommend the ticket, with row 11 re-pointed at it, so the declared-schemas
developer does not write a `bug_` test against a number that is in the archive as Done.

## 3. Claims verified

- **Filing commit 2d07e908** (2026-06-10, DataFusion 44.0.0): `test_gpu_executor_tpcds.rs:241-244`
  carries the quoted note; `plans-tpcds.sf1/q24.txt:16` and `:71` render the join with bare keys and
  `filter=c_birth_country@0 != CAST(upper(ca_country@1) AS Utf8View)`; `plan_executor.cpp:946-958`
  is the cast arm with no string case, ending in `cudf::cast`; `:498-505` routes any non-INT64/FLOAT64
  cast off the AST; `:1413-1425` accepts ColumnRef keys only; `:1695-1711` evaluates the Inner
  residual filter with `build_column(join->filter(), inter)`.
- **cuDF** `unary/cast_ops.cu:454` — `CUDF_EXPECTS(is_fixed_width(type), "Unary cast type must be
  fixed-width.")`; STRING is not fixed-width.
- **Fix commit 257377b0** (2026-06-11): the only C++ change is the 14-line arm in
  `plan_executor.cpp`; the commit re-enabled q24 in `test_gpu_executor.rs`; its message attributes
  the cast to "q24's `s_zip = ca_zip` join" and its own q24 golden (unchanged in the commit, keys
  bare) disproves that.
- **History**: `test_gpu.rs:162` at 3435b45b (2026-07-31) enabled q24 on the device and the same
  commit's `cost-registry.csv:25` reads `full_table_gpu=enabled` with tickets `45`; `gpu_cases.inc:88`
  at 3c0750ee^ enabled; 3c0750ee (2026-09-08) "drop the six legacy execution modes"; 90732a36
  (2026-08-04) migrated the ticket text.
- **Today**: `cpp/src/expr.cpp:912-934` is the arm, `:919` the only code reference to #45;
  `:87-89` folds `Utf8`/`LargeUtf8`/`Utf8View` to STRING; `is_ast_able` `:403-447` sends the
  `NotEq` to the column path (both operands infer STRING, the cast arm returns false);
  `build_column_scalar_fn` `:734-738` is `to_upper`; `join.cpp:52-63` ColumnRef-only keys,
  `:98` and `:233` `build_expr` for semi/mark filters, `:348-368` the Inner residual filter through
  `build_column`; `project.cpp:45-52` the same routing for a project.
- **Goldens**: `grep "AS Utf8" testdata/goldens/*/*.plans.txt recipe-payloads.txt` — ten hits, all
  q24's two CTE copies in the five tpcds files; no `LargeUtf8` or dictionary cast anywhere.
- **Rust**: `translator/common.rs:72-83` `column_ordinal_of` refuses a non-column key at plan time;
  `nodes.rs:258-259` uses it for both sides; DataFusion `physical_planner.rs:908-926` wraps
  expression keys in a projection; `expr_physical.rs:70-74`, `:110-121` rebuild `CastExpr` and
  `ScalarFunctionExpr` on the CPU; `spark_hash_partition.cu:131-140` is the only key normalization
  and is hash-only.
- **Registry and corpus**: `cost-registry.csv:25` `45 163`, ten cells disabled; `corpus_cases.inc:283-288`
  attributes q24 to #163; `plans_tpcds_weeks.py:235` lowers the predicate with no cast;
  `registry.rs:263-281` only requires a disabled row to name some ticket, so `163` alone passes.
- **Wiki**: `tickets.md:19` Contents row (14, lists #45) and `:309-313`; `declared-schemas.md:218`,
  `declared-schemas-derived.md:101`, `walk-drives-every-plan.md:125`, `-impl.md:106` are the only
  other mentions besides two archive lists; `archived-tickets.md` has the `## Done` section and
  the `<a id>` + `### #NN —` form `cost-report/src/main.rs:304-345` resolves (three files, not two).
- **3a's instruments**: `test_gpu_recipe_walk.rs:43-49` `ONE_LANE`; `:536-557` `context`/`walk`
  (`ctx.sql → create_physical_plan → planner::plan(&plan, knobs)`, `(tree, _)`); `:582-607`
  `assert_walk_matches_datafusion` returns the calls and asserts rows > 0; `:610` `times`, `:630-631`
  `INNER_JOIN`, `:658-661` the inner-join test, `:667-680` the `FbKind::HashJoin { join_type }`
  idiom, `:798-811` `PROVEN` (Scan, PlainProject, HashJoin{Inner} present), `:816-828` the cover
  list; `plan_text/mod.rs:24` `pub fn render_plan(&dyn GpuNode)`; `result_text.rs:92-99` hashes
  names only. `plan/join.rs:416` Inner streams with a filter.
- **3b's instruments**: `test_executor.cpp:56-66` `make_binary`, `:74-76` `typed_col` (a 0-row view,
  legal for STRING — cuDF `column_view.cpp:126-138`, `:167-179`), `:81-119` the test; fbs
  `CastExprNode{expr, target_type, decimal_precision, decimal_scale}` (`gpu_plan.fbs:138-147`) and
  `test_plan_executor.cpp:78-83` the positional `CreateCastExprNode(fbb, inner, target)`.
- **21 rows**: counted by hand from the TPC-H nation/region names — every nation sorts above its
  region's name except EGYPT, IRAN, IRAQ and JORDAN under MIDDLE EAST.

## 4. Corrected proposal

Only the sections that change.

### 3a. The probe writes its cast

```rust
/// A string column compared with a string expression cast to another Arrow string
/// spelling — the shape `expr.cpp`'s identity arm exists for. The cast is written with
/// `arrow_cast` rather than left to coercion: DataFusion inserts one only where two
/// spellings meet, and which spellings a session carries is a reader option (#183),
/// so the explicit form keeps the shape under either. `>` rather than `<>`: on this
/// data `<>` keeps all 25 rows, `>` drops the four Middle East nations that sort below
/// their region, so a filter never applied shows.
const INNER_JOIN_STRING_CAST: &str = "SELECT n_name, r_name FROM nation JOIN region \
     ON n_regionkey = r_regionkey AND n_name > arrow_cast(upper(r_name), 'Utf8View')";
```

The test body is the proposal's; its doc drops the sentence about a DataFusion whose `upper`
answers `Utf8View`, since the shape no longer rests on that, and says instead: "the plan text is
asserted first so a simplifier that folds the cast away leaves a red rather than a pass with
nothing to prove". The plan-text assertion is better aimed at the join line — find the line
containing `GpuHashJoin` and assert it contains `AS Utf8View)` — so a cast that migrated into a
project would not satisfy it.

Expected plan today: `n_name@… > CAST(upper(r_name@…) AS Utf8View)`, identical to the implicit
form. After #183: `CAST(n_name@… AS Utf8View) > CAST(upper(r_name@…) AS Utf8View)`. Both go to
the column path (`is_ast_able`'s cast arm) and through `expr.cpp:921-923`. 21 rows either way.

### 3d. The arm's comment

Replace "DataFusion's coercion of two char keys" with wording that names no corpus query:
"a string→string cast — DataFusion coercing between Arrow's string spellings, or a user's
`arrow_cast` — has nothing to convert, since cuDF has one STRING type". Row 11 of
`declared-schemas.md` gets a new ticket for the non-string → STRING refusal (`expr.cpp:924-925`),
per F6, and the row's link moves to it.

### Sequencing (new)

State in both `45-*.md` and `183-*.md` which lands first. With the `arrow_cast` probe the order does
not matter for 3a; it still matters for the archive text, which should not claim q24 carries the
cast in the present tense — write "carried" — and for `build-test.md`'s counts (F4), which each
proposal moves by its own number.

### 7. Risks

Replace the second bullet ("3a's plan-text assertion rests on DataFusion 45's `upper` returning
`Utf8`") with: 3a rests on `arrow_cast` simplifying to a `Cast` in whatever DataFusion the tree
carries (`ArrowCastFunc::simplify`, present since the function was added); a version that stops
doing so leaves a `ScalarFunctionExpr` the translator refuses, which is a plan-time red naming
the function. Add the column-column string compare to the "no device run in the current tree"
list (F2).

## 5. Complexity

**S**, as proposed. The correction changes one string literal and two doc comments in 3a, and
adds two lines of page arithmetic; the shad-gpu run and the `ctest -L cpu` run are the same.
