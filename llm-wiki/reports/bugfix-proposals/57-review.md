# #57 — review of the proposal

Read at master 188c23ce. Every cite below was opened; nothing built or run.

## 1. Verdict

**Needs changes.** The root cause is right — and stronger than the proposal knows, since the tree
already holds a device run of q39 through this exact lowering — but the fix's helper repair (3B.1,
3B.2) is step 3 of the approved `typed-nulls` task, same file, same lines, same fix, and the proposal
neither names that task nor states the dependency.

## 2. Findings

**1. The helper repair is already an approved task's — important.**
`llm-wiki/tasks/tasks.md:101` (`typed-nulls.md`, state `approved to build`, closes #198) and
`llm-wiki/tasks/typed-nulls.md` "3. The gtest literal helpers address the fields they name": the same
diagnosis (`is_null` inserted as field 2, `make_int64_literal` always builds 0,
`make_float64_literal` lands in `uint_val`), the same fix (`ScalarValueBuilder`, "as the Rust writer
does with `ScalarValueArgs::default()`"), and the same consequence ("`FilterNation…` currently runs
`n_regionkey > 0` … with the literal repaired it runs the `> 2` it always claimed"). Its scope
table names `cpp/tests/gpu/test_plan_executor.cpp:49-66` explicitly. Two approved changes to the
same eighteen lines is a conflict for whichever lands second, and the #57 test cannot be written
before the repair (typed-nulls says so itself: "Fix this before writing the test in step 4").
Correction: drop 3B.1 and 3B.2 from this fix; state in Scope that it lands after `typed-nulls`
(or as that chain's child) and adds only `make_null_literal` and the new test. What typed-nulls
does *not* say — that this shift is what condemned the value-form lowering — is this proposal's
real contribution and belongs in the archived ticket line either way.

**2. The decisive evidence is in the history and uncited — minor.**
`git show 8f471cc0 -- peacockdb-core/tests/test_gpu_executor.rs` — the commit that introduced the
comparand lowering (`-S'comparand' -- cpp/src/plan_executor.cpp` finds it first there, 2026-06-11
10:06) — records in the same commit: "q39 cov 1.0561770587198125 vs 1.0561770587198123 … flips the
~53 rows whose cov straddles the `cov > 1` filter boundary". That is q39's `CASE mean WHEN 0 THEN
NULL ELSE stdev/mean END` (project) and `(CASE mean WHEN 0 THEN 0 ELSE … END) > 1` (filter) run on a
device through the value-form arm, agreeing with DataFusion to the last ULP of the stddev. The
serializer at that commit forwarded `case.expr()` (`plan_serializer.rs:1018-1040` at 8f471cc0), and
before it the C++ threw, so no other path could have produced that number. Section 7's first risk
("if a rerun … still fails, the diagnosis is wrong") is answered by this: the lowering ran on the
only real user and was right. Cite it — it is one line and it is what stops the next reader of
25bec107 re-deriving section 2.

**3. The corpus query's mode claim is wrong at tp4-single — minor.**
`planner/translator/nodes.rs:381-389`, `lanes_for`: `Batching::Off => t.target_partitions` before
the `bytes < small_table_bytes => 1` arm, and the doc says "It bites only while batching is on".
So at tp4-single date_dim plans `lanes=4, partition_groups=[[[0]],[],[],[]]` — every small table in
`tpch.sf1/tp4-single.plans.txt` does (`:68`, `:70` region and nation). The query still reaches
`build_column_case` at all five modes (three empty lanes are never runnable; q6 runs the shape
today), but "the plan is the same shape at each" is not true. Say "reaches the arm at all five;
tp4-single carries three empty lanes". Aside for the helper, not this fix: `architecture.md:97-100`
("drops to one lane even at tp4") omits the batching condition the code carries — a drift.

**4. build-test.md bookkeeping is incomplete — minor.**
3D adds one model test. `build-test.md:49` "Exec-model prototype (Python)" 216 → 217, the header's
Python 369 → 370, and the grand total 1569 → 1571 once the C++ +1 is counted. 3E names only the C++
row and the C++ header figure. Nothing machine-checks the table (`grep 1569` over tests finds
nothing), so this is prose agreement, which the shared rules require per commit.

**5. The model's value form re-implements a rule the model already has — minor.**
3D: "`comparand_values == when_values` (pandas `==` is False on NaN/None …)". The model's `Binary`
already returns pandas' nullable boolean with SQL null semantics, pinned at
`scripts/exec_model/tests/test_operators.py:156-170` ("`None != 'x'` … SQL and cuDF answer NULL");
a raw `==` on a nullable `Int64` column returns `<NA>`, not False, and `.where` on that mask is the
pandas default the file exists to avoid. Build each arm's condition as `Binary("==", comparand,
when)` and feed the existing `fillna(False)` fold at `expressions.py:363-366` — one rule in one
place, which is also the shape the C++ arm has (`EQUAL` then the shared `copy_if_else` fold).

**6. Two cites are off — minor.**
`cpp/src/gpu_executor.cpp:200-203` is `begin_plan`'s catch; `execute_node`'s is `:229-237` (and it
resets the session, which the message the driver sees does not say). `FilterNation`'s assertions
are `test_plan_executor.cpp:309-310`, not `:304-306`.

**7. "Minimum" is the semantic pin, not the minimum — minor.**
`SELECT CASE d_moy WHEN 1 THEN 0 ELSE d_moy END FROM date_dim` reaches the throw at the first
`CudfProject`. The proposal's query is the right one to keep — it pins the NULL comparand and the
typed-NULL THEN, which no corpus row exercises — but call it what it is.

## 3. Claims verified

- The guard: `cpp/src/expr.cpp:773-780` throws on `c->expr()`; the search fold is `:785-803`;
  `is_ast_able` routes every `CaseExprNode` off the AST (`:405-408`); `infer_expr_type` types a
  CASE by its first THEN (`:389-393`); `build_scalar` at `:453` reads `is_null` and `int_val`;
  `<cudf/binaryop.hpp>` at `:7`; the column–scalar binop arm at `:609-626`; callers
  `project.cpp:45-52`, `filter.cpp:22-28`, `aggregate.cpp:203`, `join.cpp:366`/`:477`, `sort.cpp:41`.
- The shift: `flatbuffers/gpu_plan.fbs:45-62` has `is_null` second; `git log -p -- '*.fbs'` shows it
  inserted by 7ece0548 (2026-06-02, "#24"), which updated the four join calls (`/*filter_columns=*/0`)
  and not the literal helpers; the generated positional constructor
  (`cpp/build/generated/gpu_plan_generated.h:954-967`) is `(fbb, type, is_null, bool_val, int_val,
  uint_val, float_val, …)`; the helper text at 12ad4999 (2026-05-25) is byte-identical to master's
  `test_plan_executor.cpp:50-66`; only these two sites build a `ScalarValue` in `cpp/`.
- The removed test (`git show 25bec107`): added in d33454c0 and removed unchanged; literals 0, 100,
  2, 200, −1 all through `make_int64_literal`; at 25bec107^ `build_scalar` read `int_val` and the
  literal arm of `build_column` was `make_column_from_scalar`, so every literal was `Int64(0)` and
  every expected value non-zero — five mismatches, actual 0, the commit's "0/null".
- The contracts: `copy_if_else` "Null element represents false" (`rapids-cuda-12.2/include/cudf/
  copying.hpp:606-631`); `binary_operation` validity is "the logical AND of the validity of the two
  operands except NullMin and NullMax" (`binaryop.hpp:152-153`); DataFusion 45
  `case_when_with_expr` (`case.rs:198-266`): `compare_with_eq`, `prep_null_mask_filter`, else on
  `base_nulls ∪ remainder`; `EvalMethod::WithExpression` is the only dispatch for a comparand
  (`:493-499`); `coerce_case_expression` at `type_coercion.rs:808`; the simplifier's one CASE rule
  requires `expr: None` and boolean THENs, < 3 arms (`expr_simplifier.rs:1383-1389`); the `LIKE '%'`
  rewrite manufactures a value-form CASE (`:1471-1497`); `is_binary_operation_supported` takes the
  common type (`/home/dmitry/cudf/cpp/src/binaryop/compiled/util.cpp:79-118`);
  `make_default_constructed_scalar`'s fixed-point arm uses `scale_type{0}`
  (`third_party/cudf/cpp/src/scalar/scalar_factories.cpp:39-46`).
- The Rust side: translator forwards the comparand (`translator/expr.rs:86-107`); the writer sets
  `CaseExprNode.expr` (`wire/expr_writer.rs:114-149`); the CPU backend hands it to `CaseExpr::try_new`
  (`cpu_backend/expr_physical.rs:86-108`); `PlanError::Unsupported`'s doc names "a value-form CASE
  (#57)" (`plan/mod.rs:52-54`) and nothing refuses it — `test_planner_join_refusals.rs` has no CASE
  case and no test or source in the tree pins the throw message.
- The corpus: four `CASE <expr> WHEN` sites per tpcds mode golden, none in tpch; q39 `none, none`
  at `corpus_cases.inc:298` behind #163 (`:293-296`); registry line 40 `57 163`; q39 in the payload
  cover subset (`test_plan_goldens.rs:212`); payloads show `then null::Float64` and `then 0` with
  `scalar_text` printing Float64 via `to_string()` (`fb_text.rs:351`), so the THEN/ELSE types agree
  and `copy_if_else` will not refuse; q39 absent from `plans_tpcds*.py`; the model's `Case` doc
  names #57 (`expressions.py:349-355`); `registry.rs:271-283` requires a ticket on a disabled row;
  `tickets.md:19` lists #57 in a count of 14; build-test.md rows 27 and 65; string keys hash
  (`spark_hash_partition.cu:164`); `operator-cases-impl.md:229-236` plans `a_value_case_agrees`
  green-first.
- hacks-audit: it did not read `expr.cpp:470-945` (its "What I did not read"), and its #198 entry
  finds no scaffolding around the literal path — so nothing there is left in place or fought.
- The corpus query: d_year=2001 is 365 rows, January 31, February 28, December 31; date_dim's
  projected columns are declared Int64 in every golden; the predicate goes to the column path as a
  whole (`NULL_LOGICAL_AND` of an AST-able comparison and a `Gt` whose left is a CASE), so the first
  `CudfFilter` call reaches `build_column_case`.

## 4. Corrected proposal

Only the sections that change.

### 2. Root cause — one paragraph added

The lowering is not merely "never shown wrong": 8f471cc0, the commit that introduced it, ran q39 on
a device through it and recorded cov 1.0561770587198125 against DataFusion's 1.0561770587198123
(`test_gpu_executor.rs` comment at that commit) — a stddev ULP, on the only corpus user, before the
gtest with zeroed literals was written nine days after the field shift.

### 3B. `cpp/tests/gpu/test_plan_executor.cpp` — after `typed-nulls`

This fix lands on a tree where `typed-nulls` step 3 has already rebuilt `make_int64_literal` and
`make_float64_literal` through `ScalarValueBuilder` and moved `FilterNation` to its true count. Here:
add `make_null_literal(fbb, type)` (`add_type`, `add_is_null(true)`), and the value-form test as
written in the proposal — expected `[100, -1, NULL, -1, -1]`, `null_count() == 1`, the four values
by row. If the sequencing cannot be honoured and this lands first, say so in the detail file: the
helper repair then ships here and `typed-nulls` finds its step 3 done.

### 3D. `scripts/exec_model/operators/expressions.py`

`Case` gains `comparand: Expr | None = None`. In `evaluate`, when it is set, each arm's condition is
`Binary("==", self.comparand, when)` and the existing fold consumes it unchanged; the docstring's
last sentence goes. One model test with a null comparand row.

### 3E. Bookkeeping — two more lines

`build-test.md`: Exec-model prototype (Python) 216 → 217, header Python 369 → 370, grand total
1569 → 1571. The archived ticket line names 8f471cc0's q39 run as the evidence, not only the shift.

### Scope

Add: **depends on `typed-nulls` (tasks.md #11)** for the literal helpers; touches
`test_plan_executor.cpp` only to add a helper and a test.

### 5. Minimum corpus query

Keep the query; change the mode sentence to: "all five — tp4-rowgroup and tp4-sized plan one lane
under the small-table rule, tp4-single plans four with three empty, and the arm is reached at each".
Name `SELECT CASE d_moy WHEN 1 THEN 0 ELSE d_moy END FROM date_dim` as the strict minimum.

### 7. Risks

Drop the first bullet (answered by 8f471cc0). Add: if this lands before `typed-nulls`, the helper
lines are edited twice across two branches and the second rebase carries a conflict in a test file.

## 5. Complexity

**S**, agreed — and smaller than stated once 3B.1/3B.2 are removed: `expr.cpp` ~20 lines, one
helper and one test (~50), one doc line, ~10 lines of Python, one CSV cell, the ticket move, three
counts. One shad-gpu run of `peacock_plan_executor_tests` plus the rust-only and python tiers.
