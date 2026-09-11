# #168 — the fbs `ScalarValue` has no interval, so one join residual has no payload

Read at master c18e063a. Paths are relative to `/media/data/peacockdb`. Nothing was built or run.

## 1. Issue

`testdata/tpch-queries/mixed-join.sql` joins `orders` to `lineitem` with the residual
`l_shipdate BETWEEN o_orderdate AND o_orderdate + INTERVAL '90' DAY`. DataFusion leaves the
literal as `IntervalMonthDayNano { months: 0, days: 90, nanoseconds: 0 }` because it is added to a
column and cannot fold. The plan is fine on the CPU at all five modes. It cannot cross the wire:

- `flatbuffers/gpu_plan.fbs:45-61` — `ScalarValue` has no interval field and `DataType` (`:14-37`)
  no interval member.
- `peacockdb-core/src/wire/serialize.rs:100-102` — `serialize_scalar_value` falls to
  `Err("unsupported scalar value: …")` for every `DfScalarValue` it has no arm for.
- `peacockdb-core/src/wire/expr_writer.rs:32-37` — the literal arm wraps that as
  `PlanError::Unsupported("{why} (#168)")`.
- `peacockdb-core/src/wire/writer.rs:50-60` — a payload that cannot be written fails the whole
  recipe plan, so `attach_recipes` returns `Err` and the plan golden prints
  `not runnable: unsupported: unsupported scalar value: IntervalMonthDayNano(…) (#168) at #3`
  (`testdata/goldens/tpch.sf1/tp1-single.plans.txt:158`, `tp1-rowgroup.plans.txt:158`,
  `tp4-single.plans.txt:254`, `tp4-rowgroup.plans.txt:242`, `tp4-sized.plans.txt:242`).

What it disables, corrected against the code: the five device cells of `tpch/mixed_join`
(`tests/common/corpus_cases.inc:82` `gpu_modes = none`; `testdata/cost-registry.csv:130` tickets
`116 168`). No cpu cell. It is the only query in either bench with an unfolded interval — every
other `INTERVAL` in `testdata/*-queries/*.sql` (tpch q1 q4 q5 q6 q10 q12 q14 q15 q20) sits beside a
date literal and folds before translation; the tpcds goldens carry no `IntervalMonthDayNano` at all.

Ticket-text drift: #168 says the node's payload "reads `unavailable:` with the reason, and the
placeholder adopts the children already taken". The code no longer substitutes anything
(`writer.rs:50-54`, "Nothing is substituted"); the whole plan fails and the golden line reads
`not runnable:`. The mechanism the ticket describes was replaced; the gap it names is unchanged.

## 2. Root cause

One missing spelling on the wire, not a bug in any engine.

1. Translation keeps DataFusion's scalar verbatim: `planner/translator/expr.rs:34-36`
   `Expr::Literal(lit.value().clone())`, and the `Plus` above it carries `out_type = Date32`
   (`:37-46`). The plan IR is complete and correct — the tree and memory sections of the golden
   render, and the CPU backend hands the literal straight back to DataFusion
   (`executor/cpu_backend/expr_physical.rs:54`), which is why the cpu cells are green.
2. The recipe writer is the first consumer that must spell the value in the wire vocabulary.
   `serialize_scalar_value` (`wire/serialize.rs:14-106`) has arms for bool, the integers, floats,
   strings, `Date32`, `Decimal128` and typed nulls; `convert_data_type` (`:108-134`) has the same
   type set. `IntervalMonthDayNano` reaches the `other =>` arm at `:100`.
3. `write_expr` turns that into `PlanError::Unsupported` (`expr_writer.rs:36-37`);
   `Writer::node` appends the seq (`writer.rs:59`); `attach_recipes` fails
   (`wire/attach.rs:26-35`); `recipes_of` in `tests/test_plan_goldens.rs:91-96` prints it.
4. The C++ side has never seen one: `build_scalar` (`cpp/src/expr.cpp:453-496`) and
   `build_expr`'s literal arm (`:157-255`) both end in a throw for an unknown `DataType`.

Where the literal would be evaluated once it crosses, traced so the C++ change is one arm:

- The residual of an Inner hash join is applied after the gather by
  `build_column(join->filter(), inter)` (`cpp/src/operators/join.cpp:352-367`) — the column
  path, not the AST.
- `build_column` (`expr.cpp:820-942`) asks `is_ast_able` (`:403-451`). For
  `l_shipdate <= (o_orderdate + <interval>)`: `infer_expr_type` (`:346-401`) on the inner `Plus`
  gives `rt = fb_to_type_id(<interval>) = EMPTY` (`:74-95`, `default`), so the `Plus` is not
  AST-able (`:429-430`), so neither is the `<=` above it nor the `AND` above that. Every level goes
  to `build_column_binary` (`:579-632`).
- `build_column_binary` for the `Plus` takes the rhs-literal fast path (`:610-617`):
  `lcol = build_column(o_orderdate)` (a `TIMESTAMP_DAYS` column copy), `rscalar =
  build_scalar(literal)`, `out = binop_output_type(Plus, TIMESTAMP_DAYS, <scalar type>)`
  (`:555-577` — not a predicate, not decimal, so it echoes lhs: `TIMESTAMP_DAYS`), then
  `cudf::binary_operation(lcol, *rscalar, ADD, TIMESTAMP_DAYS)`.
- So the single C++ gap is `build_scalar` returning a `cudf::duration_scalar<cudf::duration_D>`.
  cuDF's compiled binop adds a `timestamp_D` and a `duration_D` into a `timestamp_D`, which is
  exactly Arrow's `Date32 + IntervalMonthDayNano{0, d, 0}`: both are epoch-day integer addition.
- The `>=` half and the `<=` comparison of two `TIMESTAMP_DAYS` columns, and the `NULL_LOGICAL_AND`
  over two `BOOL8` columns, are paths the corpus already runs (tpch q17's residual, every date
  filter in q3/q10).

`fb_to_type_id` returning `EMPTY` for the new member is what keeps the literal off the AST, and it
is the right routing: cuDF's AST has no coercion and would refuse `timestamp_D + duration_D` unless
proven otherwise, while the column path is proven for timestamps.

## 3. Localized fix

Append one `DataType` member and three `ScalarValue` fields to the fbs, write them, read them in
one C++ arm. No ABI symbol, no ordinal moves, no existing payload byte moves (a FlatBuffers table
omits a field at its default, so every scalar written today serializes to the same bytes; the
payload golden's verify-before-rewrite step proves it during the regen — see below). This is the
close the ticket itself names, "on the terms the other two took" (`UnaryOp.Sqrt`,
`AggregateMode.Merge`).

### 3a. `flatbuffers/gpu_plan.fbs`

After `BinaryView,` in `enum DataType` (`:36`):

```
  /// Appended, so no ordinal moves. Arrow's month-day-nanosecond interval, which is what
  /// DataFusion gives an `INTERVAL '90' DAY` literal added to a column. Carried by literals
  /// only; no column in either bench has the type.
  IntervalMonthDayNano,
```

After `decimal_scale: int8;` in `table ScalarValue` (`:60`):

```
  // IntervalMonthDayNano: the three components as Arrow holds them. The writer sends whole
  // days only — cuDF adds a day duration to a date and has no calendar month — so a reader
  // refuses months or nanoseconds rather than dropping them.
  interval_months: int32;
  interval_days: int32;
  interval_nanos: int64;
```

Both generated files come from this schema at build time (`peacockdb-core/build.rs:10-25`,
`cpp/CMakeLists.txt:117-129`); nothing generated is committed. The C++ positional
`fb::CreateScalarValue(...)` gains three trailing defaulted parameters, so the gtest helpers that
call it positionally (`cpp/tests/gpu/test_plan_executor.cpp:50-56`) are unaffected — the trap
`typed-nulls.md` step 3 records is for an *inserted* field, and this appends.

### 3b. `peacockdb-core/src/wire/serialize.rs`

Add `IntervalUnit` to the `datafusion::arrow::datatypes` import. In `serialize_scalar_value`,
before the `other if other.is_null()` arm (`:92`):

```rust
        DfScalarValue::IntervalMonthDayNano(Some(v)) => {
            // cuDF adds a day duration to a date and has no calendar month, and a sub-day
            // part on a Date32 is a truncation arrow performs that cuDF would not.
            if v.months != 0 || v.nanoseconds != 0 {
                return Err(format!(
                    "an interval with months or a sub-day part has no cuDF duration: {sv:?}"
                ));
            }
            args.type_ = fb::DataType::IntervalMonthDayNano;
            args.interval_months = v.months;
            args.interval_days = v.days;
            args.interval_nanos = v.nanoseconds;
        }
```

In `convert_data_type` (`:108-134`), one arm beside `BinaryView`:

```rust
        ArrowDataType::Interval(IntervalUnit::MonthDayNano) => fb::DataType::IntervalMonthDayNano,
```

That arm is what makes a typed-null interval (`IntervalMonthDayNano(None)`, through the
`other if other.is_null()` arm at `:92-99`) and the fb `DataType` of a `Cast` target agree with the
non-null literal; it does not give an interval *column* a cuDF type (the C++ maps it to `EMPTY`,
the same class as `Binary`/`Null`/`Float16` that `declared-schemas.md` and #200 already name).

Refusing months and sub-day parts here rather than in the C++ keeps the decision where every
other cannot-cross decision is made and tested: the plan stays runnable on the CPU, the device
never receives a plan it cannot run, and the golden says `not runnable:` by name — the state
mixed-join is in today, narrowed to the shapes cuDF genuinely lacks. No corpus query has one
(tpch q5's `+ interval '1' year` folds).

### 3c. `peacockdb-core/src/wire/expr_writer.rs:32-37`

Delete the three comment lines and the `(#168)` wrap:

```rust
        Expr::Literal(value) => {
            let scalar = serialize_scalar_value(b, value).map_err(PlanError::Unsupported)?;
```

The next unwritable scalar (a `Timestamp`, #200's family) then reads
`unsupported scalar value: … at #N` with no ticket, and the meta test at
`tests/test_plan_goldens.rs:805-830` goes red the day one lands in a golden — which is the guard
working, not a gap.

### 3d. `peacockdb-core/src/wire/fb_text.rs:342-365` `scalar_text`

One arm before the `_ =>` fallback, so the payload golden renders the wire's three fields rather
than `int_val()`'s `0`:

```rust
        fb::DataType::IntervalMonthDayNano => format!(
            "interval({} mons, {} days, {} nanos)",
            value.interval_months(),
            value.interval_days(),
            value.interval_nanos()
        ),
```

### 3e. `cpp/src/expr.cpp:453-496` `build_scalar`

One arm before `default:`:

```cpp
    case fb::DataType_IntervalMonthDayNano: {
      // The writer sends whole days: cuDF adds a duration_D to a timestamp_D and has no
      // calendar month, so a month or sub-day part here is a payload no writer produces.
      if (sv->interval_months() != 0 || sv->interval_nanos() != 0)
        throw std::runtime_error(
            "interval literal with months or nanoseconds has no cuDF duration");
      return std::make_unique<cudf::duration_scalar<cudf::duration_D>>(
          cudf::duration_D{sv->interval_days()}, valid);
    }
```

`valid` is the existing `!sv->is_null()` at `:456`; `cudf::duration_D` is already used at `:481`.
Nothing else in the C++ changes:

- `fb_to_type_id` (`:74-95`) keeps its `default: EMPTY`, which is what routes any expression
  holding the literal to the column path (2. above).
- `build_expr`'s literal arm (`:157-255`) is left alone. It is reached only by a semi, anti or
  mark join's residual (`join.cpp:98`, `:233`) — no corpus query adds an interval there — and
  `typed-nulls.md` (board task 11) replaces that arm with a delegation to `build_scalar` through a
  type dispatch whose `duration_scalar` case is the one `cudf::ast::literal` constructor already
  covers. Adding the arm twice now is the duplication that task exists to remove.
- `binop_output_type` (`:555-577`) already answers `TIMESTAMP_DAYS` for
  `(Plus, TIMESTAMP_DAYS, DURATION_DAYS)` by echoing lhs. The lhs-literal path (`interval + date`)
  would echo `DURATION_DAYS` and cuDF would refuse the declared output loudly; DataFusion writes the
  corpus shape date-first, so this is a named limit, not part of the fix.

### 3f. How CPU and GPU stay one engine

The CPU backend is untouched: `expr_physical.rs:54` hands the same `ScalarValue` back to
DataFusion, which is also the oracle. The device adds `duration_D{d}` to `timestamp_D`. For a
whole-day interval both are the same integer addition on epoch days, so the two agree by
arithmetic, and the recipe-walk test in 3g compares them on a device against DataFusion. The
shapes where the arithmetic would *not* agree — calendar months, sub-day parts truncated by
arrow on a `Date32` — never reach the device (3b), so no device answer can differ from the CPU's.

### 3g. Tests, goldens, registry rows and comments that move with it

Rust, `rust-only` tier:

- `wire/expr_writer/tests.rs` — two cases beside
  `the_literal_kinds_the_corpus_produces_all_write` (`:92`):
  `an_interval_of_whole_days_writes_its_days` (90 days → `type_() == IntervalMonthDayNano`,
  `interval_days() == 90`, `interval_months() == 0`) and
  `an_interval_with_months_or_a_sub_day_part_is_refused_by_name` (`new(1, 0, 0)` and
  `new(0, 0, 1)` both `Err` whose message contains `cuDF duration`).
- `wire/tests.rs:578-632` — the structural test stays, because what it proves (a payload failure
  in a node with an input fails the plan and names the seq) is not about intervals. Re-point
  `filter_over_scan_with_an_interval` at `IntervalMonthDayNano::new(1, 0, 0)`, rename it
  `filter_over_scan_with_a_month_interval`, reword the doc (`:578-584`) — the one literal shape
  the writer still refuses — and in `a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where`
  drop `assert!(said.contains("(#168)"))` (`:631`), keeping `"scalar value"` and `" at #"`.
- `wire/attach.rs:22-23` and `wire/mod.rs:330-331` — the doc example "an expression the wire has
  no shape for (#168)" becomes "a timestamp literal, #200" or drops the parenthesis.
- `tests/test_plan_goldens.rs:400` — `const NOT_RUNNABLE: &[(&str, &str, &str)] = &[];` and
  the doc at `:391-399` loses its "when #168 closes" sentence. The test above it keeps guarding:
  a golden line starting `not runnable` with no declaration still panics.
- `tests/test_plan_goldens.rs:553-555` — keep the filter, reword the comment: a `not runnable`
  line is a plan with no payload to show, declared in `NOT_RUNNABLE`; stop naming mixed-join.
- `tests/test_plan_goldens.rs:174` — `PAYLOAD_QUERIES` becomes `[(&str, &str); 21]` with
  `("tpch", "mixed-join")` and a comment: the one interval literal in either bench, and the byte
  pin on what the C++ is handed for it, since no device cell runs the query (6. below). The
  doc's list of expression features (`:162-164`) gains "an interval".
- Goldens: `UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test
  test_plan_goldens` rewrites the mixed-join `--- recipes ---` section of the five
  `testdata/goldens/tpch.sf1/*.plans.txt` (the tree and memory sections do not move; expect
  `git diff --stat` to show five files, one section each, the same recipe lines hash-join carries
  at that mode with mixed-join's seqs). Run it once *without* `PEACOCK_REWRITE_RECIPE_BYTES` first:
  the payload test then verifies every existing `sha256=` against the new build, which is the
  proof that the appended fields moved no byte. Then once *with* it to add the mixed-join section
  to `testdata/goldens/recipe-payloads.txt`, on the fixed `/tmp` symlink build-test.md requires.
  No execution golden, cost golden or result golden moves: the CPU run is unchanged.

Device, shad-gpu:

- `tests/test_gpu_recipe_walk.rs` — add
  `const DATE_RESIDUAL: &str = "SELECT count(*), sum(l_quantity) FROM orders o JOIN lineitem l \
   ON o_orderkey = l_orderkey AND l_shipdate BETWEEN o_orderdate AND o_orderdate + INTERVAL '90' DAY"`
  and `a_join_residual_that_adds_days_to_a_date_answers_on_the_device`, calling
  `assert_walk_matches_datafusion(DATE_RESIDUAL, ONE_LANE)` and asserting
  `times(&calls, FbKind::HashJoin { join_type: JoinType::Inner }) == 1`. Add the pair to the list in
  `the_kinds_a_device_has_run_are_the_kinds_this_file_claims` (`:816-828`); it produces no kind
  outside `PROVEN`. At `OneBatchPerLane` the probe is one batch, so the `#152` assertion at `:440`
  holds, and the compare is on rendered digits (`common/result_text.rs`), so the sum's
  `Decimal128(38,2)` export (#187) does not redden it. This is the only place the corpus can prove
  the device arm end to end today, because both of mixed-join's device blockers outlive #168.
- Optional, host-only C++ (`cpp/tests/cpu/test_executor.cpp`, runs on every push): in
  `AstRouting.IsAstAble` a `TIMESTAMP_DAYS column + interval literal` case expecting `false`, and in
  `DecimalScale.BinopOutputType` `binop_output_type(Plus, {TIMESTAMP_DAYS}, {DURATION_DAYS}).id()
  == TIMESTAMP_DAYS`. Build the literal with `fb::ScalarValueBuilder` field by field, not the
  positional `CreateScalarValue`.

Registry and corpus comments:

- `testdata/cost-registry.csv:130` — tickets `116 168` → `116 152 187`, gpu cells stay
  `disabled`. Same column as `hash_join` (`:127`), which is the same plan minus the residual.
- `tests/common/corpus_cases.inc:80-82` — replace the three lines: mixed-join's `none` now means
  what hash-join's does — `#187` at tp1-single (the sums' declared `Decimal128(25,2)` come back
  38) and `#152` at the four modes whose probe is more than one batch. Say the tp1-single verdict
  was read off a device run, not inferred from hash-join (7. below).

Wiki, in the same commit:

- `llm-wiki/tickets.md:211-224` — #168 to `archive/archived-tickets.md`; the index row at `:19`
  drops it (14 → 13).
- `llm-wiki/tasks/declared-schemas.md:226-228` — the bullet saying
  `SELECT l_shipdate + interval '1 day' FROM lineitem` never reaches a device becomes false; it
  becomes a walk query (a `Date32` sink column produced by arithmetic) or is dropped. The task is
  `approved to build`, so this is the helper's edit.
- `architecture.md` and `build-test.md`: nothing falsified. The wire section's "Two appended fbs
  values buy that" is about the aggregate design and stays true.

### 3h. hacks-audit scaffolding

The audit names nothing for #168. What this change removes is the ticket's own residue listed
above (the `(#168)` wrap, the `NOT_RUNNABLE` entry, the two doc examples, the payload-cover
comment). What it must respect: audit item 10 (`build_expr`/`build_scalar` build the same scalars
twice) is `typed-nulls`' to fix, so this change adds its arm to `build_scalar` only and does not
touch `build_expr`; and audit item 3 (`wire/read.rs` child-walk whitelist) is untouched because
no node kind is added.

## 4. Alternatives rejected

- **Planner rewrite, no wire change** — translate `Date32 ± IntervalMonthDayNano{0,d,0}` into
  `CAST(CAST(date AS Int32) ± d AS Date32)` in `translator/expr.rs`. Works on both backends
  (arrow and cuDF both cast `Date32`/`TIMESTAMP_DAYS` ↔ `Int32`), touches one arm and the
  goldens. Rejected: it is coding-style.md's "building around a bug" exactly — a rewrite whose
  only reason is the wire gap, baked into plan shape; the golden stops saying what the SQL said;
  months must then be refused on the CPU too; and the wire still cannot carry an interval
  anywhere else (a project, a CASE branch).
- **A days-only field under a non-Arrow name** (`IntervalDays`) — smaller, but the `DataType`
  enum is "a subset of Arrow data types", and a device that later learns months would need a
  second wire change. The triple costs two fields that are always zero today and read by the C++.
- **Serialize the interval as `Int32` days** — cuDF will not add an `INT32` to a `TIMESTAMP_DAYS`,
  and it hides a type change the plan did not declare.
- **Teach the device months and sub-day parts** (`cudf::datetime::add_calendrical_months` plus a
  truncation rule matching arrow's `Date32 + nanos`) — no corpus query needs it; the writer's
  refusal names the shape when one does.
- **Add the AST literal arm in `build_expr` now** — reached only by semi/anti/mark residuals, none
  in the corpus, and `typed-nulls` deletes that arm.
- **`IntervalDayTime` / `IntervalYearMonth` writer arms** — DataFusion 45's parser gives SQL
  interval literals `MonthDayNano`; no producer of the other two.
- **Refuse months in the C++ only, letting the plan cross** — the query would then die at execute
  time on a device after running on the CPU; the writer's refusal fails at plan time and prints
  in the golden.

## 5. Minimum corpus query

```sql
SELECT count(*) FROM orders
WHERE o_orderdate + INTERVAL '90' DAY < DATE '1993-01-01';
```

tpch sf1. Column + literal, so nothing folds; `count(*)` is `Int64`, so neither #183 nor #187 is
in the way, and no join, so neither is #152. It plans today at all five modes and runs on the CPU
(DataFusion evaluates the literal); on the device it is `not runnable` at every mode — the whole
plan fails in `attach_recipes` at the filter's seq, `#1`, with the same message mixed-join's
golden shows. After the fix it crosses at all five modes and, read against q6 (a filter plus a
keyless aggregate, green on the device at all five), should run at all five on a device. Whether
it joins the corpus as a new query is the spec author's call: it would be the only cell-bearing
proof of the device arm, at the cost a new corpus query carries (five plan and execution goldens,
a result section, the DuckDB cost oracle under the 1.5.4 pin). The recipe-walk test in 3g proves
the arm without any of that.

The corpus query that exposes it as it stands is `tpch/mixed-join` — the same shape inside an
Inner join's residual, at every mode, on the CPU tier for the plan and on no device tier.

## 6. Cells re-enabled

None directly. What comes back:

- The plan crosses the wire at all five modes: the five `not runnable` lines become recipe
  sections, `NOT_RUNNABLE` empties, `recipe-payloads.txt` gains the query and pins the interval's
  bytes, and the walk runs the shape on a device against DataFusion.
- `tpch/mixed_join`'s five gpu cells move from "cannot cross" to "crossed and refused", the state
  `tpch/hash_join` is in: tp1-single behind #187 (the decimal sums widened to 38 at the unload),
  tp1-rowgroup, tp4-single, tp4-rowgroup and tp4-sized behind #152 (the join's probe is many
  batches and the recipe's `build copy` has no ABI symbol — board task 1, `refcounted-tables`,
  closes it). When #187 closes, mixed_join's tp1-single comes back with hash_join's; when #152
  closes, the other four with hash_join's.
- `declared-schemas`' excluded interval query becomes drivable.

## 7. Risks and unknowns

- **cuDF `binary_operation(timestamp_D column, duration_D scalar, ADD, TIMESTAMP_DAYS)`** on
  25.02 and 26.02. Read as supported (libcudf's chrono binops, the same path cuDF's own
  `datetime + timedelta` takes) but not run here. The walk test in 3g is the proof; if it fails,
  the fallback is `cudf::cast` both sides to `INT32`, add, cast back — inside `build_column_binary`,
  still no wire change.
- **Appended fields move no existing payload byte.** Argued from FlatBuffers omitting default
  fields and the Rust builder's `push_slot` skipping defaults; proven by the verify-before-rewrite
  regen step in 3g. If a digest moves there, stop — the schema change is wrong, not the golden.
- **#187 at tp1-single is inferred from hash-join's identical output schema**, not observed. One
  `PCK_TEST_FILTER=mixed_join` run of `test_gpu_corpus` on shad-gpu with the tp1-single gpu cell
  enabled should show the `Decimal128(38, 2)` refusal at the unload; the corpus comment must say
  which it was.
- **`typed-nulls` landing order.** If it lands first, its "walk every type `convert_data_type`
  can emit" test needs a row for the interval, and its scalar-to-`ast::literal` dispatch must
  cover `duration_scalar` (it says it does). If it lands after, it inherits the arm. Either order
  is one line; a rebase across it is not a conflict.
- **DataFusion's simplifier leaving the minimal query's `o_orderdate + interval` unfolded** was
  reasoned (no rule moves arithmetic across a comparison), not run; the planner test that pins
  its plan text settles it.
- **`interval + date` with the literal on the left** — `binop_output_type` echoes lhs and would
  declare a duration output; cuDF refuses loudly. Not a corpus shape; named, not fixed.
- Nothing in the memory estimator, validator or null analysis reads a literal's type in a way an
  interval changes (`grep` over `peacockdb-core/src` finds `Interval` only in `plan_text` and
  the wire test); asserted by reading, not by a run.

## 8. Complexity

**M.** The code is small — about 25 production lines across four files (`gpu_plan.fbs`,
`wire/serialize.rs`, `wire/expr_writer.rs`, `wire/fb_text.rs`) and one C++ arm (`cpp/src/expr.cpp`),
roughly 80 lines of tests across four test files plus one comment, one registry cell and two
wiki edits. What lifts it past S: a frozen surface changes (the fbs, appended — no ABI symbol, no
wire byte moves), five plan goldens are regenerated in one section each, `recipe-payloads.txt` is
rewritten under the second environment variable on the symlinked host, and the device arm is
proven only by a shad-gpu run of the walk plus a one-off corpus filter run to record the #187
verdict.
