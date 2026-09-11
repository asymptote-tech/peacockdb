# Review of the #168 proposal

Read at master c18e063a against `168-proposal.md`. Paths relative to `/media/data/peacockdb`.
Every file:line the proposal cites was opened; nothing was built or run.

## 1. Verdict

**Needs changes.** The mechanism is right — one appended enum member, three appended fields, a
writer arm, a `build_scalar` arm, and the C++ trace to `build_column_binary`'s rhs-literal path
holds line for line — but the proposal misses one pinning assertion that goes red the moment the
fix lands, leaves a class of interval shapes moving from a plan-time `not runnable` to a device
run-time throw without saying so (contradicting its own rejected-alternative argument), and its
test plan carries two self-inconsistencies a developer hits in the first hour.

## 2. Findings

### F1 — a pinning assertion the fix reddens is not listed (important)

`peacockdb-core/tests/test_plan_goldens.rs:375-380`, inside
`every_published_seq_addresses_the_kind_its_recipe_claims`:

```rust
    assert_eq!(
        uncrossable,
        ["tpch mixed-join"],
        "the queries this mode plans but cannot cross to the device are #168's, and only its"
    );
```

After the fix `attach_recipes` succeeds for mixed-join, `uncrossable` is `[]`, and this
`assert_eq!` panics. The proposal's 3g lists `:400` (`NOT_RUNNABLE`), `:391-399`, `:553-555`,
`:174`, `:162-164` and `:805-830`, and not this one — although `00-tickets.md`'s #168 row names
`test_plan_goldens.rs:379` explicitly. The comment above it at `:357-360` ("the day a second
query joins mixed-join here") also stops being true.

Correction: `assert!(uncrossable.is_empty(), "every plan this mode plans crosses the wire; \
these did not: {uncrossable:?}")`, and reword the comment at `:357-360` to say an `Err` here
is a plan the wire cannot carry and there are none.

### F2 — three interval shapes move from plan-time refusal to device run-time throw, unstated (important)

Today every `IntervalMonthDayNano` literal fails in `serialize_scalar_value` (`wire/serialize.rs:100`),
so any query carrying one is `not runnable` at plan time and the golden says so. After the fix
the writer accepts a whole-day interval anywhere, and three shapes the device still cannot
evaluate reach it and fail at execute time instead:

- **A semi, anti or mark join's residual.** `cpp/src/operators/join.cpp:98` and `:233` call
  `build_expr` with no `is_ast_able` gate; `build_expr`'s literal arm (`expr.cpp:157-255`) has
  no interval case and throws `unsupported literal type: 21`. Adding an arm would not help: the
  `mixed_*` joins evaluate the residual as a cuDF AST, which never coerces (`expr.cpp:421-424`,
  the repo's own comment), so `timestamp_D + duration_D` has no AST evaluation at all. This is
  reachable from SQL — `EXISTS (SELECT 1 FROM lineitem l WHERE l.l_orderkey = o.o_orderkey AND
  l.l_shipdate > o.o_orderdate + INTERVAL '90' DAY)` — and the capability matrix runs filtered
  semi joins on a device (single-batch probe).
- **A bare interval literal in a project.** `cpp/src/operators/project.cpp:45-49`:
  `is_ast_able(LiteralExpr)` is `!is_string_like_literal` → true → `build_expr` → the same
  throw. The proposal's 3e says the literal arm "is reached only by a semi, anti or mark join's
  residual"; it is reached by every AST-able expression (filter, project, `eval_ast_subtree`),
  and an interval avoids it only where a *binary* wraps it, because `is_ast_able`'s binary arm
  sees `fb_to_type_id → EMPTY` (`expr.cpp:429-430`).
- **`interval + date`, literal on the left.** `build_column_binary`'s lhs-literal path
  (`expr.cpp:618-625`) declares `binop_output_type(Plus, DURATION_DAYS, TIMESTAMP_DAYS)` =
  `DURATION_DAYS`; cuDF's compiled binop refuses the declared output. The proposal names this
  one ("a named limit") — at run time.

None is a corpus shape, so no cell or golden moves. What is wrong is that the proposal's own
"Alternatives rejected" says of refusing months in the C++: "the query would then die at execute
time on a device after running on the CPU; the writer's refusal fails at plan time and prints in
the golden" — and then leaves these three to die exactly that way. The failure is still loud
(`BackendError` → `RunError::CallFailed`), so this is not blocking; it is a refusal-point
regression the spec must state, and the first shape is cheap to keep at plan time.

Correction: in `wire/join.rs`, where a `LeftSemi | LeftAnti | LeftMark` join's `filter` is
written, walk the filter for an `Expr::Literal(ScalarValue::IntervalMonthDayNano(_))` and return
`PlanError::Unsupported("an interval literal in a semi, anti or mark join's residual has no cuDF
AST form")` — the same plan-time, golden-visible refusal every other cannot-cross decision takes.
One case in `wire/tests.rs` beside the re-pointed structural test. Name the other two in the
ticket's archive entry (or file the pair as one ticket: prompts.md says a refusal is production
behaviour), since a bare interval projection also yields an `Interval` sink column the device
cannot export (`fb_to_type_id → EMPTY`), and the writer is not where that is decided.

### F3 — the re-pointed structural test keeps an assertion its new message cannot satisfy (minor)

3g keeps `assert!(said.contains("scalar value"))` at `wire/tests.rs:630` while re-pointing the
fixture at `IntervalMonthDayNano::new(1, 0, 0)`, whose refusal 3b spells
`"an interval with months or a sub-day part has no cuDF duration: {sv:?}"`. That string does
not contain `scalar value`; the test as specified fails.

Correction: keep the prefix every other unwritable scalar carries —
`format!("unsupported scalar value: an interval with months or a sub-day part has no cuDF
duration: {sv:?}")` — so the assertion holds and a golden line stays greppable by the same
words. Or drop the `"scalar value"` assertion and assert `"cuDF duration"`; either way the
proposal has to pick one.

### F4 — the "verify before rewrite" step is sequenced wrong against `PAYLOAD_QUERIES` (minor)

3g adds `("tpch", "mixed-join")` to `PAYLOAD_QUERIES` (`test_plan_goldens.rs:174`) and then says
to run the payload test once *without* `PEACOCK_REWRITE_RECIPE_BYTES` "which is the proof that
the appended fields moved no byte". That run compares `digests_of(&canonical)` against
`digests_of(&text)` as whole `BTreeMap`s (`:291-297`); with mixed-join already in the list the
new map has a key the canonical file lacks and the `assert_eq!` fails for that reason, not for a
moved byte — a red that reads exactly like the failure it is meant to rule out.

Correction: do the schema, writer and C++ change; run the verify step with `PAYLOAD_QUERIES`
untouched (20 entries, every digest equal — that is the proof); *then* add mixed-join and run
once with the rewrite variable. Two sentences in 3g, in that order.

(The byte claim itself holds: `ScalarValueBuilder::add_*` goes through `push_slot`, which skips
a field at its default, and the vtable length is the highest *written* field id — so a scalar
that writes none of the three new fields serializes to the bytes it does today.)

### F5 — a wrong citation into hacks-audit.md (minor)

3h says "audit item 10 (`build_expr`/`build_scalar` build the same scalars twice) is
`typed-nulls`' to fix". Item 10 of `llm-wiki/reports/hacks-audit.md` is the unreachable
`distinct` guard in `aggregate.cpp:148` and the over-wide aggregate name sets. The two-builders
duplication is stated in `llm-wiki/tasks/typed-nulls.md` ("The duplication is the bug"), and the
audit's own #198 entry says it found no scaffolding there. Item 3 (the `wire/read.rs` whitelist)
is cited correctly.

### F6 — "paths the corpus already runs" overstates what a device has run (minor)

2. says the `>=`, the `<=` over two `TIMESTAMP_DAYS` columns and the `NULL_LOGICAL_AND` over two
`BOOL8` columns are "paths the corpus already runs (tpch q17's residual, every date filter in
q3/q10)". q17 is out on both engines on #163 (`corpus_cases.inc`, registry); q3 and q10 have no
device cell (#183/#152/#185); and a date filter such as q6's is AST-able and never takes
`build_column_binary` at all. The column-path `NULL_LOGICAL_AND` over two materialised BOOL8
columns and the column-path timestamp compare are proven today by nothing that CI runs on a
device. The walk test in 3g is the proof; say so rather than lean on cells that are off.

### F7 — an edit scheduled to a frozen spec (minor)

3g asks the helper to edit `llm-wiki/tasks/declared-schemas.md:226-228`. The task is
`approved to build` (`tasks.md:92`), and prompts.md freezes a spec once finalized with exactly one
later write. Leave the bullet: it becomes a falsified sentence for that task's analyst, or the
helper amends it before that chain starts — not in the #168 commit. `typed-nulls.md`'s "no
`.fbs` edit" scope stays true either way (it is #198's scope, not a claim about the tree).

## 3. Claims verified

- The plan: `translator/expr.rs:34-36` keeps the scalar verbatim; the `Plus` above it carries
  `out_type = Date32` because DataFusion derives the result type from arrow's own kernel
  (`Date32 + IntervalMonthDayNano → Date32`), and the golden shows no cast.
- The failure chain: `serialize.rs:100-102` → `expr_writer.rs:36-37` (`(#168)` wrap) →
  `writer.rs:64` `at_seq` → `attach.rs:26-35` → `test_plan_goldens.rs:91-96`. Five goldens carry
  the line at the cited line numbers (`tp1-*:158`, `tp4-single:254`, `tp4-rowgroup:242`,
  `tp4-sized:242`); tpcds goldens carry no `IntervalMonthDayNano` (the `30 days` hits are column
  aliases). Every other corpus interval sits beside a date literal and folds.
- Ticket-text drift is real: `writer.rs:50-54` substitutes nothing; the golden says `not runnable:`.
- The C++ trace: Inner residual applied by `build_column(join->filter(), inter)`
  (`join.cpp:352-367`); `is_ast_able` refuses any binary holding the literal because
  `fb_to_type_id` returns `EMPTY` for an unlisted member (`expr.cpp:74-95`, `:429-430`); the
  refusal propagates up through `&&` so `<=`, then `AND`, take `build_column_binary`; the `Plus`
  takes the rhs-literal fast path (`:610-617`); `binop_output_type` echoes lhs = `TIMESTAMP_DAYS`
  (`:555-577`). One imprecision: `infer_expr_type(Plus)` itself returns `lt` (`TIMESTAMP_DAYS`),
  not `EMPTY` — it is `is_ast_able`'s own `rt` on the `Plus` that is `EMPTY`; the conclusion is
  the same. `build_scalar` already uses `cudf::duration_D` (`:481`) and `valid` (`:456`).
- Filter and project gate on `is_ast_able` (`filter.cpp:22`, `project.cpp:45`), so the
  proposal's minimal query takes the column path; the nested-loop join gates too
  (`join.cpp:433`), so an interval in a NLJ predicate takes cross-then-mask and works.
- `cudf::binary_operation(timestamp_D column, duration_D scalar, ADD, TIMESTAMP_DAYS)`: not
  run here either, but it is the exact call spark-rapids makes for `date_add`
  (`DURATION_DAYS` scalar added to a `TIMESTAMP_DAYS` column, output `TIMESTAMP_DAYS`), and
  libcudf's chrono `Add` is `decltype(lhs + rhs)` for a timestamp and a duration with no common
  type. Risk stated by the proposal; low.
- Arithmetic agreement for whole days: arrow's `Date32Type::add_month_day_nano` shifts zero
  months, adds `days`, adds zero nanoseconds; cuDF adds epoch days. Months would diverge
  (calendar vs none); a sub-day part is truncated by chrono's `NaiveDate + Duration`. The
  writer's refusal of both keeps the device off those shapes.
- Appending is safe for the positional gtest helpers (`test_plan_executor.cpp:52`, `:61`):
  three trailing defaulted parameters. (Those helpers are already mis-positioned by the earlier
  `is_null` insertion — `typed-nulls.md` step 3 — which this change neither fixes nor worsens.)
- Nothing generated is committed: `peacockdb-core/build.rs` runs flatc into `OUT_DIR`,
  `generated.rs` is an `include!`, `cpp/CMakeLists.txt:117-129` regenerates on the schema.
- `serialize_schema` uses `convert_data_type(..).unwrap_or(Null)`; with the new arm an interval
  *column* maps to `IntervalMonthDayNano` instead of `Null`, and both reach `EMPTY` in the C++
  (`union.cpp:42`), so no behaviour moves and no corpus schema has one.
- One `ScalarValue` construction site on the Rust side (`serialize.rs:18`); `null_literal`
  (`node_writer.rs:299`) goes through `write_expr`. `scalar_text` (`fb_text.rs:342-365`) would
  otherwise print `int_val()`'s `0` for the interval — the 3d arm is needed.
- `PAYLOAD_QUERIES` is `[(&str, &str); 20]` (`:174`); the cover test filters `not runnable`
  shapes at `:553-555`; `every_refusal_names_a_ticket_that_exists` reads both ticket files
  (`:794`), so archiving #168 breaks no link; the cost-report resolves registry tickets across
  all three files (`cost-report/src/main.rs:327-341`), so `116` (archived) staying in the
  registry cell is as it is for `hash_join` today.
- The recipe walk: `resolve` conflates copy with original (`:212-216`), the one-probe-batch
  assert is at `:438-446`, `PROVEN` (`:798`) already holds every kind the proposed query emits
  (Scan, CoalescePartitions, HashJoin Inner, both Aggregate arms, Finalize project), and
  `SUM_BY_FLAG` already exports a `Decimal128(25,2)` sum through the raw ABI, so #187 does not
  reach the walk. `test_gpu_recipe_walk` is staged for the CI GPU job (build-test.md).
- Registry and corpus: `cost-registry.csv:130` is `tpch,1,mixed_join,…,116 168`; `:127`
  `hash_join` is `116 152 187`; `corpus_cases.inc:80-82` is the three-line comment. mixed-join's
  probe is one batch only at tp1-single (`partition_groups` one chunk), so #187 there and #152
  at the other four is the same account `hash_join` carries.
- `declared-schemas.md:226-228` is the bullet described; `typed-nulls.md` step 1 dispatches a
  `cudf::scalar` to `cudf::ast::literal`, whose `duration_scalar<T>&` constructor covers the new
  arm; the landing-order note holds.
- `architecture.md` and `build-test.md`: nothing this change falsifies. "Two appended fbs
  values buy that" is about the aggregate design and stays true; the `binary-op output type`
  row ("the wider input") is loose for timestamp + duration but not false.
- Not in the proposal but worth having: the ticket itself says "Not proposed: one query is a
  thin case for a surface change, and T21 does not need it." The proposal re-enables no cell;
  it says so honestly in 6.

## 4. Corrected proposal — sections that change

### 3. Localized fix (additions and corrections only)

**3b** — message keeps the prefix:

```rust
        DfScalarValue::IntervalMonthDayNano(Some(v)) => {
            if v.months != 0 || v.nanoseconds != 0 {
                return Err(format!(
                    "unsupported scalar value: an interval with months or a sub-day part has \
                     no cuDF duration: {sv:?}"
                ));
            }
            args.type_ = fb::DataType::IntervalMonthDayNano;
            args.interval_months = v.months;
            args.interval_days = v.days;
            args.interval_nanos = v.nanoseconds;
        }
```

**3e (new paragraph)** — *Where the plan-time refusal moves.* The writer now accepts a whole-day
interval in any expression, and three shapes the device cannot evaluate would fail at execute
time: a semi/anti/mark residual (`join.cpp:98`, `:233` build a cuDF AST ungated, and the AST has
no `timestamp_D + duration_D`), a bare interval literal in a project (`project.cpp:45-49` sends a
bare literal to `build_expr`), and `interval + date` with the literal on the left
(`build_column_binary:618-625` declares a duration output). The first stays plan-time:
`wire/join.rs`, when writing the `filter` of a `LeftSemi | LeftAnti | LeftMark` join, refuses an
`Expr::Literal(IntervalMonthDayNano(_))` anywhere in it by name. The other two are named in the
archive entry for #168 (or one new ticket for the pair); neither is a corpus shape.

**3g additions**

- `tests/test_plan_goldens.rs:375-380` — the `uncrossable` `assert_eq!` becomes
  `assert!(uncrossable.is_empty(), …)`; the comment at `:357-360` stops naming mixed-join.
- `wire/tests.rs:630-632` — with the 3b message above the `"scalar value"` and `" at #"`
  assertions stand; only the `(#168)` one goes.
- `wire/tests.rs` — one new case: a `GpuHashJoin{LeftSemi}` over two scans whose filter adds an
  interval to a build column is refused by `attach_recipes` with a message naming the residual
  and `" at #"`.
- Golden order: run the payload test under `UPDATE_CANONICAL=1` alone **with `PAYLOAD_QUERIES`
  still at 20 entries** — every digest equal is the byte proof — and only then add
  `("tpch", "mixed-join")` and run once more with `PEACOCK_REWRITE_RECIPE_BYTES=1`.
- Wording: the AND-over-BOOL8 column path and the column-path timestamp compare are proven by
  the walk test, not by q17/q3/q10, which have no enabled device cell.

**3h** — cite `typed-nulls.md` for the two-builders duplication; hacks-audit item 3 is the only
audit item this change must respect.

**Wiki** — drop the `declared-schemas.md` edit from this change's commit (frozen spec); leave a
line in the archive entry that the bullet at `:226-228` is now false.

### 7. Risks and unknowns (one added)

- **Refusal point.** Three interval shapes (3e) no longer print `not runnable` in a golden; the
  semi/anti/mark residual is kept plan-time by the writer, the other two fail on the device at
  execute time with the C++ message. Named, and neither is in either bench.

## 5. Complexity

**M**, agreeing with the proposal. F1–F4 add perhaps twenty lines and one unit test; what sets
the size is unchanged — an fbs append, five golden sections, the payload golden under its second
variable on the symlinked host, and one shad-gpu cycle for the walk plus a `PCK_TEST_FILTER=mixed_join`
device corpus run to record the #187 verdict rather than infer it.
