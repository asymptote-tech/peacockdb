# Aggregate cases implementation plan

**Goal:** Every row of `aggregate-cases.md`'s matrix a case — the corpus's group keys, `count(*)`
and expression arguments, every merge arm with rows, the Welford finalize, the project
expressions and casts with no case, and sorted merges on `desc`, nullable and composite keys —
each green or a `bug_` test naming a ticket.

**Architecture:** Cases only, in the harness task 9 left: a hand-built node over `Given` leaves,
a `Script`, `run_both`, `.same(order)`. The aggregate file's builders (`call`, `body`, `init`,
`merge_over`, `state_of`, `welford_state`, `welford_answered`) and the accumulate file's
(`runs`, `sorted_lanes`, `by_id`, `merged`, `one_per_lane`) are what every case is written
with; where a builder holds a key type or a sort order constant, a sibling that takes it as a
parameter is added beside it and the original delegates. `Utf8View` is a declaration on a
`Given` leaf over `Utf8` data (`declaring_view_strings` in `harness_cases.rs`), never a retyped
batch. A case that diverges is run once green-form to read what the engine does, then rewritten
as `bug_…` asserting that, with its ticket above it.

**Tech stack:** Rust at the gpu rung; `shad-gpu` through `scripts/build-test-shadgpu.sh`.

**Spec:** [`aggregate-cases.md`](aggregate-cases.md) — frozen. The matrix is the checklist.

## Global constraints

- **Cases only.** Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside
  `src/tests/gpu_tests/`; `synthetic.rs` untouched. A case that would pass with a production
  change is a ticket and a `bug_` test.
- **No new mechanism.** Parameterised siblings of existing builders only; a case that needs
  more is a finding against the harness, recorded in the detail file.
- **Every divergence gets a ticket before it gets a `bug_` test**, precise as in task 9: the
  wrong slot through `assert_same` against a hand-written batch, or the refusal's message
  through `gpu_refuses()` / `cpu_refuses()`.
- **`WELFORD_RELATIVE` is the one tolerance and stays the one.** A case that needs another is
  a finding, not a second constant.
- **Every `Utf8View` group-key case but the one pin projects the key out of its output.**
- **Every case is named for its shape**; no loop over functions, key types or sort options.
- The kind guard does not move.
- Commit messages at most 10 lines; `rustfmt` on the files you touched.

## The device cycle

```bash
scripts/build-test-shadgpu.sh --build && scripts/build-test-shadgpu.sh --push-binaries --patch
PCK_RUN_CPP=0 PCK_TEST_FILTER='tests::gpu_tests::aggregate_cases' scripts/build-test-shadgpu.sh --run
```

Foreground only. Substitute the family per task.

## File structure

| file | responsibility |
|---|---|
| `src/tests/gpu_tests/aggregate_cases.rs` | group keys, count and expression arguments, merge arms, Welford finalize |
| `src/tests/gpu_tests/exec_cases.rs` | project expressions and casts; `GpuSort` on real keys |
| `src/tests/gpu_tests/accumulate_cases.rs` | `GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions` on `desc`, nullable and composite keys, 4 lanes |
| `llm-wiki/tickets.md`, `llm-wiki/build-test.md`, `aggregate-cases-detail.md` | the record |

`synthetic`'s ordinals: `0 id Int64`, `1 key Int32` (seven values, nulls), `2 i32`, `3 i64`,
`4 f64` (dyadic), `5 s Utf8` (six words), `6 d Date32`, `7 b Boolean`. `Expr` has `Column`,
`Literal(ScalarValue)`, `Binary { left, op, right, out_type }`, `Unary { op, arg }`, `Cast {
expr, target }`, `Like { expr, pattern, negated, case_insensitive }`, `Case { comparand,
when_then, else_expr }`, `ScalarFunction { name, args, return_type, nullable }`. `BinaryOp` has
`Eq NotEq Lt LtEq Gt GtEq Plus Minus Multiply Divide Modulo And Or`; `UnaryOp` has `Not IsNull
IsNotNull Negative Sqrt`. `PlanAgg` has `Sum Min Max Count Mean M2 MergeM2`.

---

### Task 1: Group keys

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`

Matrix row 1. `init(grouped, aggs)` groups on `key: Int32` (`state_of` writes the key field as
`("key", Int32)`, `group_by` is `Expr::column(1, "key")`). Add the parameterised sibling:

- [ ] **Step 1: `init_by` and `state_by`**

```rust
/// The key columns a grouped aggregate groups on, as `(ordinal, name, declared type)`.
type GroupKey = (u32, &'static str, DataType);

/// `state_of` with the group keys given: `[keys…, state…]`.
fn state_by(keys: &[GroupKey], aggs: &[AggCall]) -> Schema {
    let mut fields: Vec<(&str, DataType)> =
        keys.iter().map(|(_, name, ty)| (*name, ty.clone())).collect();
    for call in aggs {
        for field in &call.outputs {
            fields.push((field.name().as_str(), field.data_type().clone()));
        }
    }
    columns(&fields)
}

/// `init` grouped on `keys`, over a leaf declaring `declared` (the fixture's schema, or one
/// with `s` declared `Utf8View`).
fn init_by(declared: Schema, keys: &[GroupKey], aggs: Vec<AggCall>) -> GpuAggregate {
    let state = state_by(keys, &aggs);
    let group_by = keys.iter().map(|(i, name, _)| Expr::column(*i, name)).collect();
    GpuAggregate::new(
        Given::of(declared, BatchLayout::MultipleBatches),
        body(group_by, aggs, None),
        state.clone(),
        state,
    )
}

fn sum_i64() -> AggCall {
    call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)
}
```

Make `init` call `init_by(Schema::new(schema()), &[(1, "key", DataType::Int32)], aggs)` when
grouped, so there is one body.

- [ ] **Step 2: Four init cases and their merges**

```rust
operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_a_date_agrees() {
        let node = init_by(Schema::new(schema()), &[(6, "d", DataType::Date32)], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_an_int64_agrees() {
        let node = init_by(Schema::new(schema()), &[(0, "id", DataType::Int64)], vec![sum_i64()]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_two_columns_agrees() {
        let node = init_by(
            Schema::new(schema()),
            &[(1, "key", DataType::Int32), (7, "b", DataType::Boolean)],
            vec![sum_i64()],
        );
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}
```

> **Superseded by Task 8.** The spec no longer declares a view type; the two cases below and the helpers they need are retired there.

The `Utf8View` key: the leaf declares `s` as `Utf8View`, the state declares the key `Utf8View`,
and the aggregate is followed by nothing — so the output carries the key and the export refuses
(#183). Two cases: the pin, and the observable one grouped on `s` *and* `key` with a finalize
that keeps only `key` and the sum. `init_by` has no finalize; build it directly:

```rust
// #183 — the group key declared Utf8View comes back Utf8; the one pin for the aggregate
// family. The case below it is the same grouping with the key projected away.
operator_case! {
    GpuAggregate,
    fn bug_a_sum_grouped_on_a_declared_utf8view_key_is_refused_at_the_export() {
        let declared = super::harness_cases::declaring_view_strings(&schema());
        let node = init_by(declared, &[(5, "s", DataType::Utf8View)], vec![sum_i64()]);
        let why = run_both(&node, Script::Exec(vec![input()])).gpu_refuses();
        assert!(why.contains("0 s: Utf8View vs Utf8"), "{why}");
    }
}

operator_case! {
    GpuAggregate,
    fn a_sum_grouped_on_a_declared_utf8view_key_agrees_past_the_key() {
        let declared = super::harness_cases::declaring_view_strings(&schema());
        let keys: &[GroupKey] = &[(5, "s", DataType::Utf8View), (1, "key", DataType::Int32)];
        let aggs = vec![sum_i64()];
        let state = state_by(keys, &aggs);
        let output = columns(&[("key", DataType::Int32), ("sum(i64)", DataType::Int64)]);
        let finalize = vec![
            NamedExpr::new(Expr::column(1, "key"), "key"),
            NamedExpr::new(Expr::column(2, "sum(i64)"), "sum(i64)"),
        ];
        let node = GpuAggregate::new(
            Given::of(declared, BatchLayout::MultipleBatches),
            body(vec![Expr::column(5, "s"), Expr::column(1, "key")], aggs, Some(finalize)),
            state,
            output,
        );
        run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
    }
}
```

Read `init`'s single-node-shortcut cases in the file before this: a finalize on `GpuAggregate`
is the shortcut shape task 9 already drives.

- [ ] **Step 3: The merges over those states**

`merge_sum(finalize)` merges `[key Int32, sum(i64)]`. Add `merge_sum_by(keys)` — the same over
`state_by(keys, …)` with `group_by` the key ordinals `0..keys.len()` — and feed it states cut
from `synthetic`, as `sum_state` does (`source.column(1)`, `source.column(3)`): for the date
key `source.column(6)`, for `id` `source.column(0)`, for two columns `1` and `7`. One case
each: `a_sum_merge_grouped_on_a_date_agrees`, `…on_an_int64…`, `…on_two_columns…`, over
`Script::Accumulate(vec![state(32, 1), state(32, 2)])`, `Order::Any`.

- [ ] **Step 4: Device cycle, tickets, pins, commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs llm-wiki/tickets.md
git commit -m "aggregate cases: the corpus's group keys, init and merge"
```

---

### Task 2: Count and expression arguments

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`

Matrix row 2. `count(*)` and `count(1)` are `PlanAgg::Count` over a literal argument
(`aggregate.cpp` takes `tv.column(0)` or a broadcast literal for it); `count(col)` over nulls
is `Count` on `key`; `sum(a * b)` and `sum(CASE)` are `Sum` over an expression.

- [ ] **Step 1: Five cases, grouped, plus `count(*)` global**

```rust
fn count_star() -> AggCall {
    call(PlanAgg::Count, Expr::Literal(ScalarValue::Int64(Some(1))), "count(*)", DataType::Int64)
}

operator_case! {
    GpuAggregate,
    fn a_grouped_count_star_agrees() {
        run_both(&init(true, vec![count_star()]), Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_global_count_star_agrees() {
        run_both(&init(false, vec![count_star()]), Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_grouped_count_of_a_nullable_column_skips_its_nulls() {
        let aggs = vec![call(PlanAgg::Count, Expr::column(1, "key"), "count(key)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}

operator_case! {
    GpuAggregate,
    fn a_grouped_sum_of_a_product_agrees() {
        let product = Expr::binary(Expr::column(3, "i64"), BinaryOp::Multiply, Expr::column(3, "i64"), DataType::Int64);
        let aggs = vec![call(PlanAgg::Sum, product, "sum(i64 * i64)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}

// #56 — `sum(CASE WHEN b THEN i64 ELSE 0 END)`, the corpus's shape 48 times over.
operator_case! {
    GpuAggregate,
    fn a_grouped_sum_of_a_case_expression_agrees() {
        let case = Expr::Case {
            comparand: None,
            when_then: vec![(Expr::column(7, "b"), Expr::column(3, "i64"))],
            else_expr: Some(Box::new(Expr::Literal(ScalarValue::Int64(Some(0))))),
        };
        let aggs = vec![call(PlanAgg::Sum, case, "sum(case)", DataType::Int64)];
        run_both(&init(true, aggs), Script::Exec(vec![input()])).same(Order::Any);
    }
}
```

The empties from the spec's table: `count(*)` over a zero-row batch, grouped and global —
`Script::Exec(vec![synthetic(0, 1)])`, named `a_grouped_count_star_over_zero_rows_…` and
`a_global_count_star_over_zero_rows_…`; the global one may land on #199 (its identity row).

- [ ] **Step 2: Device cycle, tickets, pins, commit**

#56 is open: if the CASE case diverges, its `bug_` cites #56; if it agrees, say so in the
detail file — #56 may be about a shape the harness cannot make.

```bash
git add peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs llm-wiki/tickets.md
git commit -m "aggregate cases: count(*), count over nulls, sums of expressions"
```

---

### Task 3: Merge arms with rows

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`

Matrix row 3. `merge_over(state, body, output)` with `body(group_by, aggs, finalize)`; a keyless
merge is `group_by: vec![]` over a state with no key column. The `cudf::reduce` arm runs for
it, and only #199's no-arrival pin drives it today.

- [ ] **Step 1: Keyless states and one builder**

```rust
/// A keyless state `[sum(i64)]`, `[count(*)]`, `[min(i32)]`, `[max(i32)]` cut from `synthetic`
/// — one row per partial, as an init over one batch per lane would emit.
fn keyless_state(rows: usize, seed: u64, column: usize, name: &str, ty: DataType) -> RecordBatch {
    let source = synthetic(rows, seed);
    RecordBatch::try_new(
        columns(&[(name, ty)]).fields.clone(),
        vec![source.column(column).clone()],
    )
    .expect("one of the fixture's columns")
}

fn keyless_merge(func: PlanAgg, name: &str, ty: DataType) -> GpuAggregateBatches {
    let state = columns(&[(name, ty.clone())]);
    let aggs = vec![call(func, Expr::column(0, name), name, ty)];
    merge_over(state.clone(), body(Vec::new(), aggs, None), state)
}
```

- [ ] **Step 2: The cases**

```rust
operator_case! {
    GpuAggregateBatches,
    fn a_keyless_sum_merge_over_arrivals_with_rows_agrees() {
        let arrivals = vec![
            keyless_state(8, 1, 3, "sum(i64)", DataType::Int64),
            keyless_state(8, 2, 3, "sum(i64)", DataType::Int64),
        ];
        run_both(&keyless_merge(PlanAgg::Sum, "sum(i64)", DataType::Int64), Script::Accumulate(arrivals))
            .same(Order::Any);
    }
}
```

Likewise `…count_merge…` (`PlanAgg::Count`, column 3 as `count(*)`, `Int64`),
`…min_merge…` and `…max_merge…` (column 2, `Int32`). Grouped `Min`/`Max` merge: `merge_sum`'s
shape with `PlanAgg::Min` over `[key, min(i32)]` cut from columns 1 and 2 — one case each.

Keyless `MergeM2` with rows: `welford_merge()` groups on `key`; write `welford_merge_keyless()`
over `welford_state()` minus its key field (build the `Schema` with `agg_state` as
`welford_state` does, one `AggStateColumns` whose columns shift down by one), fed
`welford_partial(1)` and `welford_partial(2)` with their key column dropped
(`batch.project(&[1, 2, 3])`). Assert with `welford_answered`'s shape — count exact, mean and
m2 to `WELFORD_RELATIVE` — but with no key column to match on there is one row: compare slot 2
(the finish) directly.

The merge-level finalize that computes: `merge_over` with `finalize` =
`[key, sum(i64) / count]` as `Float64` — the `avg` shape — over a state `[key, sum(i64),
count(i64)]`; one case, `a_merge_finalize_that_divides_agrees` (`Order::Any`; the division is
of dyadic integers by small counts, exact in `Float64` for the fixture's range — if the
comparator disagrees on the last digit, that is a finding for the detail file, not a tolerance).

The merge over grouping-set state: `grouping_sets(sets)` builds the init; read its state
schema (`[keys, __grouping_id, state]`) and write `merge_over` it with `group_by` on the keys
and the id, one case.

The empty: `a_keyless_sum_merge_over_zero_row_arrivals_answers_…` — arrivals
`keyless_state(0, 1, …)` twice; #199 is the no-arrival pin, so this is the zero-row neighbour
and may be green, may join #199's family.

- [ ] **Step 3: Device cycle, tickets, pins, commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs llm-wiki/tickets.md
git commit -m "aggregate cases: every merge arm with rows, keyless and grouped"
```

---

### Task 4: Welford and its finalize

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`

Matrix row 4. `welford_init()` is grouped; the global init has no case. The stddev finalize is
what DataFusion emits for `stddev(x)` over the triple: `CASE WHEN count <= 1 THEN NULL ELSE
sqrt(m2 / (count - 1)) END` — the typed NULL, `Sqrt`, and a `UInt64` count in arithmetic, over
the state the merge leaves. #163 says the cpu refuses the decimal `avg` finalize; whether it
refuses this one is the finding.

- [ ] **Step 1: The global init**

`welford_init()` groups on `key`; write `welford_init_global()` with `group_by: vec![]` over
`welford_state()` minus its key, and assert with a keyless `welford_answered` (one row, slot
0). The count pin (#163, `Int64` for `UInt64`) applies here too: expect the same `bug_` shape
and name it `bug_a_global_welford_init_exports_its_count_as_int64`.

- [ ] **Step 2: The finalize, grouped and global**

```rust
/// DataFusion's `stddev` finalize over a Welford triple, as `planner::translator` emits it:
/// `CASE WHEN count <= 1 THEN NULL::Float64 ELSE sqrt(m2 / CAST(count - 1 AS Float64)) END`.
fn stddev_finalize(count: u32, m2: u32) -> Expr {
    let count_col = || Expr::column(count, "stddev(f64)$count");
    let one = || Expr::Literal(ScalarValue::UInt64(Some(1)));
    let denominator = Expr::Cast {
        expr: Box::new(Expr::binary(count_col(), BinaryOp::Minus, one(), DataType::UInt64)),
        target: DataType::Float64,
    };
    Expr::Case {
        comparand: None,
        when_then: vec![(
            Expr::binary(count_col(), BinaryOp::LtEq, one(), DataType::Boolean),
            Expr::Literal(ScalarValue::Float64(None)),
        )],
        else_expr: Some(Box::new(Expr::Unary {
            op: UnaryOp::Sqrt,
            arg: Box::new(Expr::binary(
                Expr::column(m2, "stddev(f64)$m2"),
                BinaryOp::Divide,
                denominator,
                DataType::Float64,
            )),
        })),
    }
}
```

Confirm the exact shape against a plan golden first: `grep -n 'stddev' testdata/goldens/tpcds.sf1/tp1-single.plans.txt | head`
shows the finalize project as the planner renders it; match its operand order and cast. Then
`merge_over(welford_state(), body(vec![key], vec![merge_m2 call as in welford_merge()],
Some(vec![NamedExpr::new(Expr::column(0, "key"), "key"), NamedExpr::new(stddev_finalize(1,
3), "stddev(f64)")])), columns(&[("key", Int32), ("stddev(f64)", Float64)]))`, fed
`welford_partial(1)` and `welford_partial(2)`. The comparison of a `Float64` stddev is the
one place the exact comparator may disagree on the last digit: assert count-side exactness
through the state, and the stddev column through `welford_answered`'s `close` — extend that
helper to take the column index rather than adding a constant. Global likewise, keyless.

- [ ] **Step 3: Device cycle, tickets, pins, commit**

Expected: #163 for anything the cpu refuses by declared type; a `Sqrt` or typed-NULL
divergence on the device is new (the C++ `Sqrt` arm has device-only tests; the typed NULL is
`typed-nulls`' fix, which this chain carries).

```bash
git add peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs llm-wiki/tickets.md
git commit -m "aggregate cases: the global Welford init and the stddev finalize on both"
```

---

### Task 5: Project expressions

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`

Matrix row 5. `project(exprs)` takes `(Expr, name, DataType)` triples over `given()`. One case
per expression, each named for it, green-form first. The literal helpers `lit_i32`, `lit_i64`,
`lit_str` exist.

- [ ] **Step 1: Arithmetic and logic as columns**

```rust
operator_case! {
    GpuProject,
    fn a_minus_and_a_modulo_agree() {
        let node = project(vec![
            (Expr::binary(Expr::column(3, "i64"), BinaryOp::Minus, Expr::column(0, "id"), DataType::Int64), "i64 - id", DataType::Int64),
            (Expr::binary(Expr::column(3, "i64"), BinaryOp::Modulo, lit_i64(7), DataType::Int64), "i64 % 7", DataType::Int64),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn a_comparison_and_a_conjunction_as_columns_agree() {
        let gt = Expr::binary(Expr::column(2, "i32"), BinaryOp::Gt, lit_i32(0), DataType::Boolean);
        let both = Expr::binary(gt.clone(), BinaryOp::And, Expr::column(7, "b"), DataType::Boolean);
        let either = Expr::binary(gt.clone(), BinaryOp::Or, Expr::column(7, "b"), DataType::Boolean);
        let node = project(vec![(gt, "gt", DataType::Boolean), (both, "and", DataType::Boolean), (either, "or", DataType::Boolean)]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuProject,
    fn the_unary_forms_agree() {
        let unary = |op: UnaryOp, i: u32, name: &str| Expr::Unary { op, arg: Box::new(Expr::column(i, name)) };
        let node = project(vec![
            (unary(UnaryOp::IsNull, 1, "key"), "key is null", DataType::Boolean),
            (unary(UnaryOp::IsNotNull, 1, "key"), "key is not null", DataType::Boolean),
            (unary(UnaryOp::Not, 7, "b"), "not b", DataType::Boolean),
            (unary(UnaryOp::Negative, 3, "i64"), "-i64", DataType::Int64),
        ]);
        run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
    }
}
```

- [ ] **Step 2: The casts, one case each**

`Expr::Cast { expr: Box::new(Expr::column(3, "i64")), target: DataType::Decimal128(20, 0) }`
declared `Decimal128(20, 0)` — expected to land on #55 (device) or #187 (precision 38 on
export); `decimals(64, 1)` is the batch for `Decimal128 → Float64` (leaf: `Given::of(Schema::new(decimals(0, 0).schema()), …)`,
so build the node by hand rather than through `project`); `Date32 → Utf8` and `Utf8 → Date32`
over `d` and a string of dates — `Utf8 → Date32` needs a parseable string: cast `d` to `Utf8`
in the fixture first with `arrow::compute::cast` and hand that batch in, the leaf declaring
`Utf8` for it. Names: `a_cast_to_decimal_…`, `a_decimal_cast_to_float64_…`,
`a_date_cast_to_text_…`, `a_text_cast_to_date_…`; #203 is the existing "cannot cast a number
to text" pin — a date to text may be the same refusal (cite #203) or a different one (new).

- [ ] **Step 3: The functions**

```rust
fn function(name: &str, args: Vec<Expr>, return_type: DataType) -> Expr {
    Expr::ScalarFunction { name: name.to_string(), args, return_type, nullable: true }
}
```

Cases: `date_part("year", d)` declared `Int32` — #191 says the device answers `Int16`;
`substr(s, 2, 3)` (`Utf8`); `coalesce(key, 0)` (`Int32`); `concat(s, s)` (`Utf8`); `lower(s)`
(`Utf8`); `round(f64, 1)` (`Float64`). Read `planner/translator/expr.rs` for the exact function
names the planner emits (`date_part` vs `extract`, argument order) and use those. One case
each, `Order::AsEmitted`.

- [ ] **Step 4: LIKE's other forms, CASE with a typed-NULL branch, a string literal column**

`Expr::Like { negated: true, .. }` and `case_insensitive: true` over `s` with pattern
`lit_str("b%")`; `Expr::Case` whose `then` is `Expr::Literal(ScalarValue::Int64(None))` and
`else` is `i64` — the `typed-nulls` fix is on this chain, so green is expected and a `bug_`
is a regression finding; `project(vec![(lit_str("x"), "x", DataType::Utf8)])` — a broadcast
literal column.

- [ ] **Step 5: Sorts on real keys** (same file)

`sort(keys, fetch)` with `by(column, ascending, nulls_first)`: `fetch` with `desc` on `id`
(`Some(5)`); `fetch` at the row count (`Some(64)`) and past it (`Some(100)`); `fetch 0`;
a string key (`by(5, true, false)`); a date key (`by(6, true, false)`). `Order::AsEmitted`;
ties on a string key are broken by adding `by(0, true, false)` second, so the order is total.

- [ ] **Step 6: Device cycle, tickets, pins, commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/exec_cases.rs llm-wiki/tickets.md
git commit -m "exec cases: the expressions and casts the corpus emits, and sorts on real keys"
```

---

### Task 6: Sorted merges on real keys

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs`

Matrix row 6. `sorted(fetch)` and `merged(lanes, fetch)` order by `by_id()` — `id` ascending,
never null. #202's second site is the device's merge over a `desc` key.

- [ ] **Step 1: Parameterised siblings**

```rust
/// `sorted_lanes` declaring `order` as each batch's sort.
fn lanes_ordered(n: usize, order: Vec<ColumnOrder>) -> Box<dyn GpuNode> {
    Given::with_layout(
        Schema::new(schema()),
        PartitionLayout { sort_order: SortOrder::batch_sorted(order.clone()), ..PartitionLayout::new(n) },
    )
}

fn sorted_by(order: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuAccumulateBatchesAndSort {
    GpuAccumulateBatchesAndSort::new(lanes_ordered(1, order.clone()), order, fetch)
}

fn merged_by(lanes: usize, order: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuMergeSortedPartitions {
    GpuMergeSortedPartitions::new(lanes_ordered(lanes, order.clone()), order, fetch)
}

fn by(column: u32, ascending: bool, nulls_first: bool) -> ColumnOrder {
    ColumnOrder { column, ascending, nulls_first }
}

/// `runs`, each run sorted by `order` rather than by `id`, so the input is what the node
/// declares. Sort each run with `lexsort_to_indices` over the order's columns
/// (`use datafusion::arrow::compute::{SortColumn, SortOptions, lexsort_to_indices};`).
fn runs_ordered(rows: usize, seed: u64, n: usize, order: &[ColumnOrder]) -> Vec<RecordBatch> {
    runs(rows, seed, n)
        .into_iter()
        .map(|run| {
            let columns: Vec<SortColumn> = order
                .iter()
                .map(|o| SortColumn {
                    values: run.column(o.column as usize).clone(),
                    options: Some(SortOptions { descending: !o.ascending, nulls_first: o.nulls_first }),
                })
                .collect();
            let indices = lexsort_to_indices(&columns, None).expect("the key sorts");
            take_record_batch(&run, &indices).expect("a permutation")
        })
        .collect()
}
```

Make `sorted` and `merged` call the `_by` forms with `by_id()`.

- [ ] **Step 2: The cases, one per shape per node**

For each of `GpuAccumulateBatchesAndSort` (script `Script::Accumulate(runs_ordered(48, 1, 3,
&order))`) and `GpuMergeSortedPartitions` (script `Script::Lanes(one_per_lane(runs_ordered(48,
1, lanes, &order)))`):

- a `desc` key: `vec![by(0, false, false)]` — `…_on_a_descending_key_…`
- a nullable key nulls first: `vec![by(1, true, true), by(0, true, false)]` — `key` has nulls;
  `id` second keeps the order total — `…_on_a_nullable_key_nulls_first_…`
- nulls last: `vec![by(1, true, false), by(0, true, false)]`
- two keys: `vec![by(7, true, false), by(0, true, false)]`
- 4 lanes (merge only): `merged_by(4, by_id(), None)` over four runs — `…_over_four_lanes_…`

All `Order::AsEmitted`. The empty: `a_descending_merge_with_one_lane_empty_…` — `merged_by(3,
vec![by(0, false, false)], None)` over runs where lane 1 is `synthetic(0, 1)`.

- [ ] **Step 3: Device cycle, tickets, pins, commit**

Expected: the `desc` merge diverges on the device (#202's second site — its `bug_` cites #202
and asserts the wrong order by slot); a `nulls_first` divergence is either #202's family or new
— read the ticket's text before deciding.

```bash
git add peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs llm-wiki/tickets.md
git commit -m "accumulate cases: sorted merges on desc, nullable and composite keys, four lanes"
```

---

### Task 7: The record

**Files:**
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tasks/aggregate-cases-detail.md`

- [ ] **Step 1: Counts and rows** — the `Operator harness` entry's count, the gpu rung's
  `--lib -- gpu_tests::` figure and the grand total grow by the cases added; every new `bug_`
  is a line in the detail file's register (name, what it asserts, ticket).
- [ ] **Step 2: The full family run** — `PCK_RUN_CPP=0 PCK_TEST_FILTER='_cases'
  scripts/build-test-shadgpu.sh --run`; every module green, the kind guard green; paste the
  `test result:` lines into the detail file.
- [ ] **Step 3: Commit**

```bash
git add llm-wiki/build-test.md llm-wiki/tasks/aggregate-cases-detail.md
git commit -m "aggregate cases: the record — counts, the bug_ register, the device run"
```

### Task 8: Retire the view cases

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`, `aggregate_cases.rs`
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tasks/aggregate-cases-detail.md`

**Why:** the spec (amended) declares no view type. `Utf8View` reaches a plan only through
DataFusion's parquet option `schema_force_view_types`; the task that turns it off retires
`harness_cases.rs`'s #183 pin and the corpus's view types together. The four green cases below
are exact twins of the `*_on_a_string_agrees` cases at `aggregate_dimension_cases.rs:123-211`.

**Interfaces:**
- Consumes: `init_by` (Task 1), `run_both`, `sum_i64`.
- Produces: `init_by(keys, aggs)` without its `declared` parameter — every remaining caller
  passes `Schema::new(schema())`, so the parameter folds back inside.

- [ ] **Step 1: Delete the cases** — five, by name, in `aggregate_dimension_cases.rs`:
`a_sum_grouped_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key`,
`a_sum_merge_grouped_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key`,
`a_sum_grouped_on_an_int32_and_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key`,
`a_sum_merge_grouped_on_an_int32_and_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key`,
`bug_a_sum_grouped_on_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device`; and the
`// #183` block and the comment at `:101-105`.

- [ ] **Step 2: Delete the helpers**: the `declaring_view_strings` import (`:21`), `VIEW`
(`:44`), `device_on_a_declared…` (`:46-85`), `schema_declaring` (`:87-92`); in
`aggregate_cases.rs:246-258` drop `init_by`'s `declared` parameter and its doc.

- [ ] **Step 3: Compile and run rust-only** — `cargo test --features rust-only -p peacockdb-core
--lib -- tests::gpu_tests --no-run`, then `--test test_module_layout`. Expected: green;
`grep -rn utf8view peacockdb-core/src/tests/gpu_tests/aggregate*` finds nothing.

- [ ] **Step 4: One device run** — `PCK_RUN_CPP=0
PCK_TEST_FILTER='tests::gpu_tests::aggregate' scripts/build-test-shadgpu.sh --run`.
Expected: green; the family's count is Task 7's minus five.

- [ ] **Step 5: The record** — `build-test.md`: grand total, Rust and `--lib -- gpu_tests::`
each minus five, the harness row likewise; strike the sentence naming "one `Utf8View` pin per
family". Detail file: the retired names, one line each, with this task's reason.

- [ ] **Step 6: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests llm-wiki/build-test.md llm-wiki/tasks/aggregate-cases-detail.md
git commit -m "aggregate cases: the view-declared cases are retired; group keys are Utf8"
```
