# Operator cases implementation plan

**Goal:** Every seq-bearing operator through the harness, each row of the spec's matrix a green
comparison or a `bug_` test naming a ticket.

**Architecture:** Cases only. Each is one `operator_case!` — a hand-built node over `Given` leaves,
a `Script` of synthetic batches, `run_both`, `.same(order)`. One file per backend family, as the
backends are laid out. A case that diverges is run once in its green form to read what the engine
actually does, then rewritten as `bug_…` asserting that, with the ticket above it. The kind guard's
`PENDING` list shrinks a family at a time and is deleted at the end.

**Tech stack:** Rust at the gpu rung, `shad-gpu` through `scripts/build-test-shadgpu.sh`. One local
parquet writer for the source family.

**Spec:** [`operator-cases.md`](operator-cases.md), and the harness it uses:
[`operator-harness.md`](operator-harness.md). The matrix is the checklist; every row below cites it.

## Global constraints

- **No new mechanism.** `Script`, `run_both`, `Device`, `synthetic`, `prefixed`, `Given` and
  `assert_same` are what there is; a case that needs more is a finding against the harness task.
  The one exception is `write_parquet` in the source file.
- **Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `src/tests/gpu_tests/`.**
  A case that would pass with a production change is a ticket and a `bug_` test.
- **Every divergence gets a ticket before it gets a `bug_` test.** An existing ticket where the
  defect is the same; a new one in `llm-wiki/tickets.md`, fifteen lines at most, otherwise. A
  `bug_` test asserts the wrong behaviour precisely: the wrong slot through `same` against a
  hand-written expectation, or the refusal's message through `gpu_refuses()` / `cpu_refuses()`.
- **No fix, anywhere.** Not in an operator, not in the harness by casting or filtering a
  divergence away. Every case is green or `bug_`, and the production tree is not touched.
- **Empty inputs are separate cases**, one per shape, named for the shape, never a loop over
  types: a red one must say which combination reached the limit.
- **The join scripts follow the capability matrix:** build first and one batch, probe streamed,
  `build: None` for the `without_build` route.
- **Synthetic data only.** The source family writes its parquet under `std::env::temp_dir()` and
  removes it.
- Commit messages at most 10 lines; `rustfmt` on the files you touched.

## File structure

| file | responsibility |
|---|---|
| `src/tests/gpu_tests/exec_cases.rs` | `GpuFilter`, `GpuProject`, `GpuSort` |
| `src/tests/gpu_tests/aggregate_cases.rs` | `GpuAggregate`, `GpuAggregateBatches`, and the state-batch fixture |
| `src/tests/gpu_tests/accumulate_cases.rs` | `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions` |
| `src/tests/gpu_tests/emit_cases.rs` | `GpuEmitPartitions` |
| `src/tests/gpu_tests/join_cases.rs` | `GpuHashJoin`, `GpuCrossJoin`, `GpuNestedLoopJoin` |
| `src/tests/gpu_tests/source_cases.rs` | `GpuLoadParquet`, `write_parquet` |
| `src/tests/gpu_tests/coverage.rs` | `PENDING` shrinks, then goes |
| `llm-wiki/tickets.md`, `llm-wiki/build-test.md`, `operator-cases-detail.md` | the record |

Every file opens the same way; it is repeated per task rather than referenced:

```rust
use datafusion::arrow::datatypes::DataType;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::common::ScalarValue;

use super::script::{run_both, Script};
use crate::plan::{BatchLayout, BinaryOp, Expr, NamedExpr, PartitionLayout, Schema};
use crate::tests::{columns, decimals, prefixed, synthetic, Given, Order};
```

`declared_decimal_types()` in the exec file is two `DataType` constants written down after one
planning run, with the SQL that produced them in the comment above — not a call into the planner
per test.

`synthetic`'s ordinals, used by every expression below: `0 id Int64`, `1 key Int32`, `2 i32`,
`3 i64`, `4 f64`, `5 s Utf8`, `6 d Date32`, `7 b Boolean`. `decimals` is `0 id Int64`,
`1 dec Decimal128(18,2)`, and every case over it is expected to land as `bug_` under #187 — the
device exports a decimal at precision 38 — until the casts chain merges; write the green form,
watch the schema line `same` reports, then pin it.

The working rhythm for every task: write the cases green-form, run the family on shad-gpu with
`PCK_TEST_FILTER='tests::gpu_tests::<family>_cases'`, read each failure, decide ticket, rewrite as
`bug_`, run again, remove the family's kinds from `PENDING`, commit. The run line is

```bash
PCK_TEST_FILTER='tests::gpu_tests::exec_cases' timeout 7200 scripts/build-test-shadgpu.sh --all
```

with the family name substituted, and the guard is run with the family (`tests::gpu_tests` matches
both) so `PENDING` is checked in the same run.

---

### Task 1: `GpuFilter`, `GpuProject`, `GpuSort`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/exec_cases.rs`
- Modify: `peacockdb-core/src/tests/gpu_tests/mod.rs` (`mod exec_cases;`)
- Modify: `peacockdb-core/src/tests/gpu_tests/coverage.rs` (`PENDING` loses three)

Matrix rows: `GpuFilter` (int, string, null-yielding, with projection, all pass, none pass);
`GpuProject` (copy, int/float/decimal arithmetic, cast, CASE both forms, LIKE, scalar function,
typed NULL literal — #57, #198); `GpuSort` (asc/desc, nulls first/last, two keys, fetch).

- [ ] **Step 1: The filter cases**

```rust
fn input() -> RecordBatch {
    synthetic(64, 1)
}

fn given() -> Box<dyn crate::plan::GpuNode> {
    Given::of(Schema::new(input().schema()), BatchLayout::MultipleBatches)
}

fn filter(predicate: Expr, projection: Option<Vec<u32>>) -> crate::plan::GpuFilter {
    let schema = match &projection {
        None => Schema::new(input().schema()),
        Some(keep) => Schema::new(std::sync::Arc::new(input().schema().project(
            &keep.iter().map(|i| *i as usize).collect::<Vec<_>>(),
        ).unwrap())),
    };
    crate::plan::GpuFilter::new(given(), predicate, projection, schema)
}

fn gt(ordinal: u32, name: &str, literal: ScalarValue) -> Expr {
    Expr::binary(Expr::column(ordinal, name), BinaryOp::Gt, Expr::Literal(literal), DataType::Boolean)
}

operator_case! { GpuFilter, fn a_filter_on_an_int_keeps_the_same_rows() {
    let node = filter(gt(2, "i32", ScalarValue::Int32(Some(0))), None);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

operator_case! { GpuFilter, fn a_filter_on_a_string_keeps_the_same_rows() {
    let predicate = Expr::binary(
        Expr::column(5, "s"), BinaryOp::Eq,
        Expr::Literal(ScalarValue::Utf8(Some("beta".into()))), DataType::Boolean,
    );
    run_both(&filter(predicate, None), Script::Exec(vec![input()])).same(Order::Any);
}}

/// `i32` is null on every fifth row; a null predicate is not true, so those rows go on both.
operator_case! { GpuFilter, fn a_null_predicate_drops_the_row_on_both() {
    let node = filter(gt(2, "i32", ScalarValue::Int32(Some(-1000))), None);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

operator_case! { GpuFilter, fn a_filter_also_projects() {
    let node = filter(gt(0, "id", ScalarValue::Int64(Some(10))), Some(vec![0, 5, 7]));
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

operator_case! { GpuFilter, fn every_row_passing_is_the_input() {
    let node = filter(gt(0, "id", ScalarValue::Int64(Some(-1))), None);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuFilter, fn no_row_passing_is_zero_rows_under_the_schema() {
    let node = filter(gt(0, "id", ScalarValue::Int64(Some(1000))), None);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}
```

- [ ] **Step 2: The project cases**

The declared type of each expression is what DataFusion would coerce to; where a case's type is
in doubt, plan the SQL through `planner::plan` in a scratch test and read the type off the node,
because a wrong declaration is a harness-side error rather than a divergence.

```rust
fn project(exprs: Vec<(Expr, &str, DataType)>) -> crate::plan::GpuProject {
    let schema = columns(&exprs.iter().map(|(_, name, ty)| (*name, ty.clone())).collect::<Vec<_>>());
    let named = exprs.into_iter().map(|(expr, name, _)| NamedExpr::new(expr, name)).collect();
    crate::plan::GpuProject::new(given(), named, schema)
}

fn lit_i32(v: i32) -> Expr { Expr::Literal(ScalarValue::Int32(Some(v))) }

operator_case! { GpuProject, fn a_column_copy_is_the_column() {
    let node = project(vec![(Expr::column(5, "s"), "s", DataType::Utf8), (Expr::column(0, "id"), "id", DataType::Int64)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn int_arithmetic_agrees() {
    let sum = Expr::binary(Expr::column(2, "i32"), BinaryOp::Plus, lit_i32(7), DataType::Int32);
    let product = Expr::binary(Expr::column(3, "i64"), BinaryOp::Multiply, Expr::Literal(ScalarValue::Int64(Some(3))), DataType::Int64);
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (sum, "sum", DataType::Int32), (product, "product", DataType::Int64)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn float_arithmetic_agrees_bit_for_bit() {
    let scaled = Expr::binary(Expr::column(4, "f64"), BinaryOp::Multiply, Expr::Literal(ScalarValue::Float64(Some(2.0))), DataType::Float64);
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (scaled, "scaled", DataType::Float64)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

// #187 — over `decimals`, so expected to pin the export's precision 38. The declared types are
// DataFusion's own: the cpu ignores `out_type` and holds the result to the declaration, so a
// wrong one is a cpu refusal and a harness error. Plan `SELECT dec + dec, dec / 2.00 FROM t`
// over a table declaring `dec DECIMAL(18,2)` and copy the two types off the physical plan —
// the add is (19,2); the divide is not (38,6), and is read rather than guessed.
operator_case! { GpuProject, fn decimal_arithmetic_lands_on_the_declared_scale() {
    let dec = decimals(64, 1);
    let node = {
        let (add_type, divide_type) = declared_decimal_types(); // the two read off the planner
        let doubled = Expr::binary(Expr::column(1, "dec"), BinaryOp::Plus, Expr::column(1, "dec"), add_type.clone());
        let halved = Expr::binary(Expr::column(1, "dec"), BinaryOp::Divide, Expr::Literal(ScalarValue::Decimal128(Some(200), 3, 2)), divide_type.clone());
        let schema = columns(&[("id", DataType::Int64), ("doubled", add_type), ("halved", divide_type)]);
        crate::plan::GpuProject::new(
            Given::of(Schema::new(dec.schema()), BatchLayout::MultipleBatches),
            vec![NamedExpr::new(Expr::column(0, "id"), "id"), NamedExpr::new(doubled, "doubled"), NamedExpr::new(halved, "halved")],
            schema,
        )
    };
    run_both(&node, Script::Exec(vec![dec])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn a_cast_lands_on_the_target_type() {
    let widened = Expr::Cast { expr: Box::new(Expr::column(2, "i32")), target: DataType::Int64 };
    let as_text = Expr::Cast { expr: Box::new(Expr::column(1, "key")), target: DataType::Utf8 };
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (widened, "widened", DataType::Int64), (as_text, "as_text", DataType::Utf8)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn a_search_case_agrees() {
    let case = Expr::Case {
        comparand: None,
        when_then: vec![(Expr::column(7, "b"), lit_i32(1))],
        else_expr: Some(Box::new(lit_i32(0))),
    };
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (case, "flag", DataType::Int32)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

// #57 — value-form CASE produces wrong results on the GPU column path. Green form first; if it
// diverges, rename `bug_a_value_case_…` and assert the wrong column `same` reports.
operator_case! { GpuProject, fn a_value_case_agrees() {
    let case = Expr::Case {
        comparand: Some(Box::new(Expr::column(1, "key"))),
        when_then: vec![(lit_i32(1), Expr::Literal(ScalarValue::Utf8(Some("one".into()))))],
        else_expr: Some(Box::new(Expr::Literal(ScalarValue::Utf8(Some("other".into()))))),
    };
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (case, "word", DataType::Utf8)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn like_agrees() {
    let like = Expr::Like {
        expr: Box::new(Expr::column(5, "s")),
        pattern: Box::new(Expr::Literal(ScalarValue::Utf8(Some("%a".into())))),
        negated: false, case_insensitive: false,
    };
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (like, "ends_in_a", DataType::Boolean)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

/// `upper` takes the column path in the C++ (`expr.cpp`, `build_column`), not the AST one.
operator_case! { GpuProject, fn a_scalar_function_agrees() {
    let upper = Expr::ScalarFunction { name: "upper".into(), args: vec![Expr::column(5, "s")], return_type: DataType::Utf8, nullable: true };
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (upper, "loud", DataType::Utf8)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

// #198 — a typed NULL inside an AST expression is a typed zero on the device. Green form
// first: `i32 + NULL` is NULL on the cpu; if the device answers `i32`, the bug_ form asserts
// that slot against a hand-built expectation. The bare literal column is a second case.
operator_case! { GpuProject, fn a_typed_null_in_arithmetic_is_null() {
    let plus_null = Expr::binary(Expr::column(2, "i32"), BinaryOp::Plus, Expr::Literal(ScalarValue::Int32(None)), DataType::Int32);
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (plus_null, "plus_null", DataType::Int32)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuProject, fn a_typed_null_literal_is_a_null_column() {
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (Expr::Literal(ScalarValue::Int64(None)), "nothing", DataType::Int64)]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::AsEmitted);
}}
```

- [ ] **Step 3: The sort cases**

`id` is unique, so it is every single-key sort's key or tie-breaker; `i32` carries nulls and
duplicates, so it is the nulls-first/last key with `id` behind it.

```rust
use crate::plan::{ColumnOrder, GpuSort};

fn by(column: u32, ascending: bool, nulls_first: bool) -> ColumnOrder {
    ColumnOrder { column, ascending, nulls_first }
}

fn sort(keys: Vec<ColumnOrder>, fetch: Option<usize>) -> GpuSort {
    GpuSort::new(given(), keys, fetch)
}

operator_case! { GpuSort, fn a_descending_sort_emits_the_same_order() {
    run_both(&sort(vec![by(0, false, false)], None), Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuSort, fn nulls_first_then_by_id() {
    run_both(&sort(vec![by(2, true, true), by(0, true, false)], None), Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuSort, fn nulls_last_then_by_id() {
    run_both(&sort(vec![by(2, false, false), by(0, true, false)], None), Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuSort, fn a_fetch_keeps_the_same_top_n() {
    run_both(&sort(vec![by(3, true, false), by(0, true, false)], Some(5)), Script::Exec(vec![input()])).same(Order::AsEmitted);
}}

operator_case! { GpuSort, fn each_batch_is_sorted_on_its_own() {
    let stream = vec![synthetic(16, 1), synthetic(16, 2), synthetic(8, 3)];
    run_both(&sort(vec![by(0, false, false)], None), Script::Exec(stream)).same(Order::AsEmitted);
}}

// Empty inputs, each its own case.
operator_case! { GpuFilter, fn a_filter_over_a_zero_row_batch_is_zero_rows_on_both() {
    let node = filter(gt(2, "i32", ScalarValue::Int32(Some(0))), None);
    run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuFilter, fn a_zero_row_batch_between_two_with_rows_is_filtered_on_its_own() {
    let node = filter(gt(2, "i32", ScalarValue::Int32(Some(0))), None);
    run_both(&node, Script::Exec(vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)])).same(Order::AsEmitted);
}}
operator_case! { GpuProject, fn a_project_over_a_zero_row_batch_is_zero_rows_on_both() {
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (Expr::binary(Expr::column(2, "i32"), BinaryOp::Plus, lit_i32(1), DataType::Int32), "next", DataType::Int32)]);
    run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuProject, fn a_zero_row_batch_between_two_with_rows_is_projected_on_its_own() {
    let node = project(vec![(Expr::column(0, "id"), "id", DataType::Int64), (Expr::column(5, "s"), "s", DataType::Utf8)]);
    run_both(&node, Script::Exec(vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)])).same(Order::AsEmitted);
}}
operator_case! { GpuSort, fn a_sort_over_a_zero_row_batch_is_zero_rows_on_both() {
    run_both(&sort(vec![by(0, false, false)], None), Script::Exec(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuSort, fn a_zero_row_batch_between_two_with_rows_is_sorted_on_its_own() {
    run_both(&sort(vec![by(0, false, false)], Some(5)), Script::Exec(vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)])).same(Order::AsEmitted);
}}
```

- [ ] **Step 4: Run the family, turn divergences into `bug_` tests, shrink `PENDING`**

Run line above with `exec_cases`. For each red: read `same`'s message; find or file the ticket;
rewrite the case as `bug_<what it does wrong>` with the ticket in a comment above it and the wrong
behaviour asserted (the wrong slot via `same(&[expected_wrong], &gpu, order)` from `Outcome.gpu`,
or `outcome.gpu_refuses()` containing the message's stable part). Then remove `"GpuFilter"`,
`"GpuProject"`, `"GpuSort"` from `PENDING` and run once more: the guard must be green.

- [ ] **Step 5: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/ llm-wiki/tickets.md
git commit -m "filter, project and sort through the harness"
```

---

### Task 2: `GpuAggregate` and `GpuAggregateBatches`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`
- Modify: `mod.rs`, `coverage.rs`

Matrix rows: each `PlanAgg` grouped and global; the single-node shortcut with a finalize; grouping
sets; a decimal sum's scale; a global aggregate over zero rows (#199); merge with and without
finalize; arrivals crossing 1 MiB; `merge_m2`; count merging by sum; an average's digits (#163).

- [ ] **Step 1: The init cases**

`GpuAggregate::new(input, body, intermediate, schema)`: `intermediate` is `[group keys…, state…]`,
`schema` is the output — the same as `intermediate` when nothing finalizes. `AggCall { func, args,
outputs }`; the state field names follow the planner's, `<agg>` or `<agg>$<part>`.

```rust
use datafusion::arrow::datatypes::Field;
use crate::plan::{AggCall, AggregateBody, GpuAggregate, GpuAggregateBatches, PlanAgg};

fn call(func: PlanAgg, arg: Expr, out: &str, ty: DataType) -> AggCall {
    AggCall { func, args: vec![arg], outputs: vec![Field::new(out, ty, true)] }
}

/// `aggs` over `synthetic`, grouped by `key` where `grouped`, nothing finalized.
fn init(grouped: bool, aggs: Vec<AggCall>) -> GpuAggregate {
    let keys: Vec<(&str, DataType)> = if grouped { vec![("key", DataType::Int32)] } else { vec![] };
    let state: Vec<(&str, DataType)> = aggs.iter().flat_map(|a| a.outputs.iter().map(|f| (f.name().as_str(), f.data_type().clone()))).collect::<Vec<_>>();
    let intermediate = columns(&[keys.clone(), state].concat());
    let body = AggregateBody {
        group_by: if grouped { vec![Expr::column(1, "key")] } else { vec![] },
        grouping_sets: vec![], null_exprs: vec![], aggs, finalize: None,
    };
    GpuAggregate::new(given(), body, intermediate.clone(), intermediate)
}

operator_case! { GpuAggregate, fn a_grouped_sum_min_max_count_agree() {
    let node = init(true, vec![
        call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
        call(PlanAgg::Min, Expr::column(2, "i32"), "min(i32)", DataType::Int32),
        call(PlanAgg::Max, Expr::column(5, "s"), "max(s)", DataType::Utf8),
        call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
    ]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

operator_case! { GpuAggregate, fn a_global_sum_min_max_count_agree() {
    let node = init(false, vec![
        call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
        call(PlanAgg::Min, Expr::column(6, "d"), "min(d)", DataType::Date32),
        call(PlanAgg::Max, Expr::column(4, "f64"), "max(f64)", DataType::Float64),
        call(PlanAgg::Count, Expr::column(5, "s"), "count(s)", DataType::Int64),
    ]);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

/// The Welford triple over `f64`: count, mean, m2 — as `stddev` decomposes. `M2` and
/// `MergeM2` are refused by both backends unless the state `Schema` names its owner in
/// `agg_state` (`plan/aggregate.rs`, `welford_owners`), so the state is not `columns(...)` but
/// the annotated schema `welford_state()` builds — copy that fixture from the device suite's
/// `accumulate.rs` (post-move: `executor/gpu_backend/gpu_tests/accumulate.rs`), keys included.
operator_case! { GpuAggregate, fn a_welford_init_agrees() {
    let state = welford_state();
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key")], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![
            call(PlanAgg::Count, Expr::column(4, "f64"), "stddev(f64)$count", DataType::UInt64),
            call(PlanAgg::Mean, Expr::column(4, "f64"), "stddev(f64)$mean", DataType::Float64),
            call(PlanAgg::M2, Expr::column(4, "f64"), "stddev(f64)$m2", DataType::Float64),
        ],
        finalize: None,
    };
    let node = GpuAggregate::new(given(), body, state.clone(), state);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

// #187 — over `decimals`. DataFusion's sum of Decimal(18,2) is Decimal(28,2) — precision plus
// ten, capped at 38 — and the cpu holds the state to that declaration, so (38,2) here would be
// a cpu refusal. Grouped by `id % 7` is not available on this fixture; a global sum is enough.
operator_case! { GpuAggregate, fn a_decimal_sum_lands_on_the_declared_scale() {
    let dec = decimals(64, 1);
    let intermediate = columns(&[("sum(dec)", DataType::Decimal128(28, 2))]);
    let body = AggregateBody {
        group_by: vec![], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![call(PlanAgg::Sum, Expr::column(1, "dec"), "sum(dec)", DataType::Decimal128(28, 2))],
        finalize: None,
    };
    let node = GpuAggregate::new(Given::of(Schema::new(dec.schema()), BatchLayout::MultipleBatches), body, intermediate.clone(), intermediate);
    run_both(&node, Script::Exec(vec![dec])).same(Order::Any);
}}

/// The single-node shortcut: init aggregators and finalize expressions on one node. The
/// finalize indexes `[keys…, state…]`; `avg` is `sum / count` over the state.
operator_case! { GpuAggregate, fn the_single_node_shortcut_finalizes_an_average() {
    let intermediate = columns(&[("key", DataType::Int32), ("avg(i64)$sum", DataType::Int64), ("avg(i64)$count", DataType::Int64)]);
    let output = columns(&[("key", DataType::Int32), ("avg", DataType::Float64)]);
    let divide = Expr::binary(
        Expr::Cast { expr: Box::new(Expr::column(1, "avg(i64)$sum")), target: DataType::Float64 },
        BinaryOp::Divide,
        Expr::Cast { expr: Box::new(Expr::column(2, "avg(i64)$count")), target: DataType::Float64 },
        DataType::Float64,
    );
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key")], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![
            call(PlanAgg::Sum, Expr::column(3, "i64"), "avg(i64)$sum", DataType::Int64),
            call(PlanAgg::Count, Expr::column(3, "i64"), "avg(i64)$count", DataType::Int64),
        ],
        finalize: Some(vec![NamedExpr::new(divide, "avg")]),
    };
    let node = GpuAggregate::new(given(), body, intermediate, output);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

/// Two keys, three sets: both, `key` alone, neither. The id is the bitmask of the masked
/// positions — 0, 2, 3 — and `null_exprs` is the typed NULL each masked key takes.
operator_case! { GpuAggregate, fn grouping_sets_emit_the_same_rows_and_ids() {
    let intermediate = columns(&[("key", DataType::Int32), ("b", DataType::Boolean), ("__grouping_id", DataType::UInt8), ("sum(i64)", DataType::Int64)]);
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key"), Expr::column(7, "b")],
        grouping_sets: vec![vec![false, false], vec![false, true], vec![true, true]],
        null_exprs: vec![Expr::Literal(ScalarValue::Int32(None)), Expr::Literal(ScalarValue::Boolean(None))],
        aggs: vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)],
        finalize: None,
    };
    let node = GpuAggregate::new(given(), body, intermediate.clone(), intermediate);
    run_both(&node, Script::Exec(vec![input()])).same(Order::Any);
}}

// Empty inputs, each its own case.
operator_case! { GpuAggregate, fn a_grouped_aggregate_over_zero_rows_is_zero_rows_on_both() {
    let node = init(true, vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)]);
    run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
}}
// #199 — a global aggregate on an empty lane drops its identity row on the device. The cpu
// answers one row: count 0, sum NULL. Green form first; the bug_ form asserts the device's
// zero rows against that.
operator_case! { GpuAggregate, fn a_global_aggregate_over_zero_rows_keeps_its_identity_row() {
    let node = init(false, vec![
        call(PlanAgg::Count, Expr::column(2, "i32"), "count(i32)", DataType::Int64),
        call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64),
    ]);
    run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
}}
operator_case! { GpuAggregate, fn grouping_sets_over_zero_rows_are_zero_rows_on_both() {
    let intermediate = columns(&[("key", DataType::Int32), ("b", DataType::Boolean), ("__grouping_id", DataType::UInt8), ("sum(i64)", DataType::Int64)]);
    let body = AggregateBody {
        group_by: vec![Expr::column(1, "key"), Expr::column(7, "b")],
        grouping_sets: vec![vec![false, false], vec![true, true]],
        null_exprs: vec![Expr::Literal(ScalarValue::Int32(None)), Expr::Literal(ScalarValue::Boolean(None))],
        aggs: vec![call(PlanAgg::Sum, Expr::column(3, "i64"), "sum(i64)", DataType::Int64)],
        finalize: None,
    };
    let node = GpuAggregate::new(given(), body, intermediate.clone(), intermediate);
    run_both(&node, Script::Exec(vec![synthetic(0, 1)])).same(Order::Any);
}}
```

The `__grouping_id` type and the `$count` type (`UInt64` vs `Int64`, #163) are read off what the
planner declares — `grep -n '__grouping_id' peacockdb-core/src/planner/translator/*.rs` and
`plan/aggregates.rs` — before the first run, not guessed.

- [ ] **Step 2: The merge cases, over state batches the file builds**

A merge consumes state, so its input is not `synthetic` but a batch of `[key, sum]` this fixture
writes — deterministic, from `synthetic`'s own columns so nothing new is invented:

```rust
/// `[key, sum(i64)]` rows: the state a partial sum would have emitted, one per row of
/// `synthetic(rows, seed)`, so a merge over several of them has duplicate keys to fold.
fn sum_state(rows: usize, seed: u64) -> RecordBatch {
    let source = synthetic(rows, seed);
    RecordBatch::try_new(
        columns(&[("key", DataType::Int32), ("sum(i64)", DataType::Int64)]).fields.clone(),
        vec![source.column(1).clone(), source.column(3).clone()],
    ).unwrap()
}

fn merge_sum(finalize: bool) -> GpuAggregateBatches {
    let state = columns(&[("key", DataType::Int32), ("sum(i64)", DataType::Int64)]);
    let output = if finalize { columns(&[("key", DataType::Int32), ("total", DataType::Int64)]) } else { state.clone() };
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "key")], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![call(PlanAgg::Sum, Expr::column(1, "sum(i64)"), "sum(i64)", DataType::Int64)],
        finalize: finalize.then(|| vec![NamedExpr::new(Expr::column(1, "sum(i64)"), "total")]),
    };
    GpuAggregateBatches::new(Given::of(state.clone(), BatchLayout::MultipleBatches), body, state, output)
}

operator_case! { GpuAggregateBatches, fn a_merge_emits_state_at_done() {
    let arrivals = vec![sum_state(32, 1), sum_state(32, 2), sum_state(0, 3), sum_state(32, 4)];
    run_both(&merge_sum(false), Script::Accumulate(arrivals)).same(Order::Any);
}}

operator_case! { GpuAggregateBatches, fn a_merge_with_a_finalize_emits_the_projected_columns() {
    let arrivals = vec![sum_state(32, 1), sum_state(32, 2)];
    run_both(&merge_sum(true), Script::Accumulate(arrivals)).same(Order::Any);
}}

/// COMPACT_BYTES is 1 MiB on the cpu and 64 MiB on the device (`{cpu,gpu}_backend/backend.rs`),
/// so the device's is the one to size for: `sum_state(1_500_000, …)` is about 18 MiB by the
/// shared formula, five of them cross 64 MiB at the fourth, and the fold must not change the
/// answer. `synthetic` at 1.5M rows is a few seconds; the upload is one C-data import.
operator_case! { GpuAggregateBatches, fn arrivals_crossing_the_compaction_threshold_fold_the_same() {
    let arrivals = (1..=5).map(|seed| sum_state(1_500_000, seed)).collect();
    run_both(&merge_sum(false), Script::Accumulate(arrivals)).same(Order::Any);
}}

/// A count merges by sum: the merge aggregator is `Sum` over a column named `count(...)`.
operator_case! { GpuAggregateBatches, fn a_count_merges_by_sum() {
    let state = columns(&[("key", DataType::Int32), ("count(i32)", DataType::Int64)]);
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "key")], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![call(PlanAgg::Sum, Expr::column(1, "count(i32)"), "count(i32)", DataType::Int64)],
        finalize: None,
    };
    let node = GpuAggregateBatches::new(Given::of(state.clone(), BatchLayout::MultipleBatches), body, state.clone(), state);
    let counts = |seed| {
        let s = sum_state(32, seed);
        let ones = datafusion::arrow::array::Int64Array::from(vec![1i64; s.num_rows()]);
        RecordBatch::try_new(s.schema(), vec![s.column(0).clone(), std::sync::Arc::new(ones)]).unwrap()
    };
    run_both(&node, Script::Accumulate(vec![counts(1), counts(2)])).same(Order::Any);
}}

/// `merge_m2` is not per column: the three state columns go in as one call's arguments, and
/// the state `Schema` must carry its `agg_state` owner or both backends refuse (`welford_owners`).
/// `welford_state()` is the annotated fixture copied above.
operator_case! { GpuAggregateBatches, fn a_welford_merge_agrees() {
    let state = welford_state();
    let body = AggregateBody {
        group_by: vec![Expr::column(0, "key")], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![AggCall {
            func: PlanAgg::MergeM2,
            args: vec![Expr::column(1, "stddev(f64)$count"), Expr::column(2, "stddev(f64)$mean"), Expr::column(3, "stddev(f64)$m2")],
            outputs: state.fields.fields()[1..].iter().map(|f| f.as_ref().clone()).collect(),
        }],
        finalize: None,
    };
    let node = GpuAggregateBatches::new(Given::of(state.clone(), BatchLayout::MultipleBatches), body, state.clone(), state.clone());
    // One Welford partial per key: count 1, mean = f64, m2 = 0 — what an init over one row emits.
    let partial = |seed| {
        let s = synthetic(32, seed);
        let ones = datafusion::arrow::array::UInt64Array::from(vec![1u64; 32]);
        let zeros = datafusion::arrow::array::Float64Array::from(vec![0.0; 32]);
        RecordBatch::try_new(state.fields.clone(), vec![s.column(1).clone(), std::sync::Arc::new(ones), s.column(4).clone(), std::sync::Arc::new(zeros)]).unwrap()
    };
    run_both(&node, Script::Accumulate(vec![partial(1), partial(2), partial(3)])).same(Order::Any);
}}

// #163, #187 — the declared type is never checked against the expression that produces it,
// and the recipe walk asserts an average's digits because cuDF takes a divide's scale from its
// operands. Every type here is the planner's own: plan `SELECT avg(dec) FROM t` over
// `dec DECIMAL(18,2)`, read the merge node's state types and its finalize expression off the
// plan text, and build exactly those — the sum state is (28,2), the count Int64, the finalize
// a divide with the casts the planner emits, and the output whatever it declares. Over
// `decimals`, grouped by nothing, so the state batches are `[sum, count]`.
operator_case! { GpuAggregateBatches, fn a_decimal_average_finalizes_to_the_same_digits() {
    let (state, output, finalize) = planned_decimal_average(); // copied off the planner, as above
    let body = AggregateBody {
        group_by: vec![], grouping_sets: vec![], null_exprs: vec![],
        aggs: vec![
            call(PlanAgg::Sum, Expr::column(0, "avg(dec)$sum"), "avg(dec)$sum", state.fields.field(0).data_type().clone()),
            call(PlanAgg::Sum, Expr::column(1, "avg(dec)$count"), "avg(dec)$count", DataType::Int64),
        ],
        finalize: Some(finalize),
    };
    let node = GpuAggregateBatches::new(Given::of(state.clone(), BatchLayout::MultipleBatches), body, state.clone(), output);
    let fields = state.fields.clone();
    let partial = |seed| {
        let d = decimals(32, seed);
        let sums = datafusion::arrow::compute::cast(d.column(1), fields.field(0).data_type()).unwrap();
        let ones = datafusion::arrow::array::Int64Array::from(vec![1i64; 32]);
        RecordBatch::try_new(fields.clone(), vec![sums, std::sync::Arc::new(ones)]).unwrap()
    };
    run_both(&node, Script::Accumulate(vec![partial(1), partial(2)])).same(Order::Any);
}}

// Empty inputs, each its own case.
operator_case! { GpuAggregateBatches, fn a_merge_over_no_arrival_answers_nothing_on_both() {
    run_both(&merge_sum(false), Script::Accumulate(vec![])).same(Order::Any);
}}
operator_case! { GpuAggregateBatches, fn a_merge_over_one_zero_row_arrival_is_zero_rows_on_both() {
    run_both(&merge_sum(false), Script::Accumulate(vec![sum_state(0, 1)])).same(Order::Any);
}}
operator_case! { GpuAggregateBatches, fn a_zero_row_arrival_among_others_changes_nothing() {
    run_both(&merge_sum(false), Script::Accumulate(vec![sum_state(32, 1), sum_state(0, 2), sum_state(32, 3)])).same(Order::Any);
}}
operator_case! { GpuAggregateBatches, fn a_finalize_over_no_arrival_answers_nothing_on_both() {
    run_both(&merge_sum(true), Script::Accumulate(vec![])).same(Order::Any);
}}
```

`planned_decimal_average()` and `welford_state()` are the file's two fixtures: constants written
down after a planning run, with the SQL in the comment, and the annotated Welford schema copied
from the device suite.

- [ ] **Step 3: Run, `bug_`, shrink `PENDING` by `GpuAggregate` and `GpuAggregateBatches`, commit**

```bash
git commit -m "both aggregates through the harness"
```

---

### Task 3: `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs`
- Modify: `mod.rs`, `coverage.rs`

Matrix rows: coalesce several/one/none (#173); sorted accumulate several/one/none, fetch (#173);
merge-sorted N lanes, fetch, one lane empty, `Done` before any batch (#173).

`synthetic`'s `id` is `0..rows` in order, so every synthetic batch is already sorted by `id`
ascending — the sorted family's batches need no pre-sort.

- [ ] **Step 1: The cases**

```rust
use crate::plan::{ColumnOrder, GpuAccumulateBatchesAndSort, GpuCoalesceAllBatches, GpuMergeSortedPartitions, SortOrder};

fn by_id() -> Vec<ColumnOrder> {
    vec![ColumnOrder { column: 0, ascending: true, nulls_first: false }]
}

fn coalesce() -> GpuCoalesceAllBatches {
    GpuCoalesceAllBatches::new(given())
}

operator_case! { GpuCoalesceAllBatches, fn several_batches_coalesce_to_one() {
    run_both(&coalesce(), Script::Accumulate(vec![synthetic(16, 1), synthetic(16, 2), synthetic(0, 3), synthetic(8, 4)])).same(Order::AsEmitted);
}}

operator_case! { GpuCoalesceAllBatches, fn one_batch_coalesces_to_itself() {
    run_both(&coalesce(), Script::Accumulate(vec![synthetic(16, 1)])).same(Order::AsEmitted);
}}

// #173 — the frozen surface cannot build a table out of nothing; `accumulate.rs` returns
// `Ok(empty)` under a doc comment claiming the device refuses. Green form first.
operator_case! { GpuCoalesceAllBatches, fn no_batch_coalesces_to_nothing_on_both() {
    run_both(&coalesce(), Script::Accumulate(vec![])).same(Order::AsEmitted);
}}

fn sorted(fetch: Option<usize>) -> GpuAccumulateBatchesAndSort {
    let sorted_input = Given::with_layout(
        Schema::new(input().schema()),
        PartitionLayout { sort_order: SortOrder::BatchSorted { columns: by_id() }, ..PartitionLayout::new(1) },
    );
    GpuAccumulateBatchesAndSort::new(sorted_input, by_id(), fetch)
}

operator_case! { GpuAccumulateBatchesAndSort, fn sorted_batches_merge_into_one_sorted_stream() {
    run_both(&sorted(None), Script::Accumulate(vec![synthetic(16, 1), synthetic(24, 2), synthetic(8, 3)])).same(Order::AsEmitted);
}}

operator_case! { GpuAccumulateBatchesAndSort, fn a_fetch_cuts_the_merged_stream() {
    run_both(&sorted(Some(10)), Script::Accumulate(vec![synthetic(16, 1), synthetic(24, 2)])).same(Order::AsEmitted);
}}

operator_case! { GpuAccumulateBatchesAndSort, fn one_sorted_batch_is_itself() {
    run_both(&sorted(None), Script::Accumulate(vec![synthetic(16, 1)])).same(Order::AsEmitted);
}}

// #173 again, the merge of no runs.
operator_case! { GpuAccumulateBatchesAndSort, fn no_sorted_batch_is_nothing_on_both() {
    run_both(&sorted(None), Script::Accumulate(vec![])).same(Order::AsEmitted);
}}

fn merged(lanes: usize, fetch: Option<usize>) -> GpuMergeSortedPartitions {
    let sorted_lanes = Given::with_layout(
        Schema::new(input().schema()),
        PartitionLayout {
            sort_order: SortOrder::BatchSorted { columns: by_id() },
            batch_layout: BatchLayout::SingleBatch,
            ..PartitionLayout::new(lanes)
        },
    );
    GpuMergeSortedPartitions::new(sorted_lanes, by_id(), fetch)
}

operator_case! { GpuMergeSortedPartitions, fn three_sorted_lanes_merge_into_one() {
    run_both(&merged(3, None), Script::Lanes(vec![vec![synthetic(16, 1)], vec![synthetic(24, 2)], vec![synthetic(8, 3)]])).same(Order::AsEmitted);
}}

operator_case! { GpuMergeSortedPartitions, fn a_fetch_cuts_the_merged_lanes() {
    run_both(&merged(2, Some(7)), Script::Lanes(vec![vec![synthetic(16, 1)], vec![synthetic(16, 2)]])).same(Order::AsEmitted);
}}

operator_case! { GpuMergeSortedPartitions, fn an_empty_lane_is_skipped_by_both() {
    run_both(&merged(3, None), Script::Lanes(vec![vec![synthetic(16, 1)], vec![], vec![synthetic(8, 3)]])).same(Order::AsEmitted);
}}

// #173 — every lane done before any batch: a merge of no runs.
operator_case! { GpuMergeSortedPartitions, fn every_lane_empty_is_nothing_on_both() {
    run_both(&merged(2, None), Script::Lanes(vec![vec![], vec![]])).same(Order::AsEmitted);
}}

// More empty inputs, each its own case.
operator_case! { GpuCoalesceAllBatches, fn one_zero_row_batch_coalesces_to_zero_rows() {
    run_both(&coalesce(), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuCoalesceAllBatches, fn a_zero_row_batch_among_others_adds_nothing() {
    run_both(&coalesce(), Script::Accumulate(vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)])).same(Order::AsEmitted);
}}
operator_case! { GpuAccumulateBatchesAndSort, fn one_zero_row_batch_sorts_to_zero_rows() {
    run_both(&sorted(None), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuAccumulateBatchesAndSort, fn a_zero_row_batch_among_sorted_others_adds_nothing() {
    run_both(&sorted(None), Script::Accumulate(vec![synthetic(16, 1), synthetic(0, 2), synthetic(16, 3)])).same(Order::AsEmitted);
}}
operator_case! { GpuAccumulateBatchesAndSort, fn a_fetch_over_zero_rows_is_zero_rows() {
    run_both(&sorted(Some(5)), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
}}
operator_case! { GpuMergeSortedPartitions, fn a_zero_row_lane_beside_lanes_with_rows_is_skipped() {
    run_both(&merged(3, None), Script::Lanes(vec![vec![synthetic(16, 1)], vec![synthetic(0, 2)], vec![synthetic(8, 3)]])).same(Order::AsEmitted);
}}
operator_case! { GpuMergeSortedPartitions, fn every_lane_a_zero_row_batch_is_zero_rows_or_nothing_the_same_way() {
    run_both(&merged(2, None), Script::Lanes(vec![vec![synthetic(0, 1)], vec![synthetic(0, 2)]])).same(Order::AsEmitted);
}}
operator_case! { GpuMergeSortedPartitions, fn lane_zero_done_before_lane_one_arrives() {
    run_both(&merged(2, None), Script::Lanes(vec![vec![], vec![synthetic(16, 2)]])).same(Order::AsEmitted);
}}
```

- [ ] **Step 2: Run, `bug_`, shrink `PENDING` by the three, commit**

```bash
git commit -m "the three accumulators through the harness"
```

---

### Task 4: `GpuEmitPartitions`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/emit_cases.rs`
- Modify: `mod.rs`, `coverage.rs`

Matrix rows: 4 lanes and 64, each lane a slot; null keys; two keys; a string key; a decimal key
(#187, #95); one lane in and four out (#184); and the empty shapes — a zero-row batch, one key
everywhere, all-null keys, a zero-row / rows / zero-row stream.

- [ ] **Step 1: The cases**

`Script::Emit` produces one slot per lane per call, so the comparison is per lane by construction:
a row in the wrong lane is a wrong slot. Every case here is one lane in and N out — the shape #184
names — so the whole family is that ticket's witness.

```rust
use crate::plan::GpuEmitPartitions;

fn emit(keys: Vec<u32>, lanes: usize) -> GpuEmitPartitions {
    GpuEmitPartitions::new(given(), keys, lanes)
}

operator_case! { GpuEmitPartitions, fn four_lanes_on_an_int_key_place_every_row_the_same() {
    run_both(&emit(vec![1], 4), Script::Emit(vec![input()])).same(Order::Any);
}}

operator_case! { GpuEmitPartitions, fn sixty_four_lanes_leave_most_empty_and_agree_on_all() {
    run_both(&emit(vec![1], 64), Script::Emit(vec![synthetic(8, 1)])).same(Order::Any);
}}

/// `key` is null on every eleventh row; a null key hashes to the seed's lane on both.
operator_case! { GpuEmitPartitions, fn null_keys_land_in_the_same_lane() {
    run_both(&emit(vec![1], 4), Script::Emit(vec![input()])).same(Order::Any);
}}

operator_case! { GpuEmitPartitions, fn two_keys_combine_the_same_way() {
    run_both(&emit(vec![1, 5], 4), Script::Emit(vec![input()])).same(Order::Any);
}}

operator_case! { GpuEmitPartitions, fn a_string_key_places_every_row_the_same() {
    run_both(&emit(vec![5], 4), Script::Emit(vec![input()])).same(Order::Any);
}}

operator_case! { GpuEmitPartitions, fn each_batch_is_scattered_on_its_own() {
    run_both(&emit(vec![1], 4), Script::Emit(vec![synthetic(16, 1), synthetic(16, 3)])).same(Order::Any);
}}

// #187 — a decimal key, over `decimals`; #95 is the murmur3 side of the same column.
operator_case! { GpuEmitPartitions, fn a_decimal_key_places_every_row_the_same() {
    let dec = decimals(64, 1);
    let node = GpuEmitPartitions::new(Given::of(Schema::new(dec.schema()), BatchLayout::MultipleBatches), vec![1], 8);
    run_both(&node, Script::Emit(vec![dec])).same(Order::Any);
}}

// Empty inputs, each its own case. `emit` must answer N lanes whatever it was handed — the
// driver checks the count — so a zero-row batch in is N slots out, and how each side fills
// them (a zero-row batch each) is the assertion.
operator_case! { GpuEmitPartitions, fn a_zero_row_batch_scatters_into_n_zero_row_lanes() {
    run_both(&emit(vec![1], 4), Script::Emit(vec![synthetic(0, 1)])).same(Order::Any);
}}
/// One key value everywhere: every row lands in one lane and three lanes get nothing.
operator_case! { GpuEmitPartitions, fn a_batch_of_one_key_leaves_n_minus_one_lanes_empty() {
    let batch = synthetic(32, 1);
    let one_key = std::sync::Arc::new(datafusion::arrow::array::Int32Array::from(vec![Some(3); 32])) as datafusion::arrow::array::ArrayRef;
    let mut cols = batch.columns().to_vec();
    cols[1] = one_key;
    let batch = RecordBatch::try_new(batch.schema(), cols).unwrap();
    run_both(&emit(vec![1], 4), Script::Emit(vec![batch])).same(Order::Any);
}}
/// Every key null: one lane, `pmod(seed, N)`, and the other lanes empty.
operator_case! { GpuEmitPartitions, fn a_batch_of_all_null_keys_lands_in_one_lane() {
    let batch = synthetic(32, 1);
    let nulls = datafusion::arrow::array::new_null_array(&DataType::Int32, 32);
    let mut cols = batch.columns().to_vec();
    cols[1] = nulls;
    let batch = RecordBatch::try_new(batch.schema(), cols).unwrap();
    run_both(&emit(vec![1], 4), Script::Emit(vec![batch])).same(Order::Any);
}}
operator_case! { GpuEmitPartitions, fn a_stream_of_zero_row_rows_zero_row_is_scattered_per_batch() {
    run_both(&emit(vec![1], 4), Script::Emit(vec![synthetic(0, 1), synthetic(16, 2), synthetic(0, 3)])).same(Order::Any);
}}
```

Note what the null-key case can and cannot see: `same` compares lanes, so it proves both engines
put the null rows in one lane and the same lane — not which lane. That is the `inc2` gate's job.

- [ ] **Step 2: Run, `bug_`, shrink `PENDING` by `GpuEmitPartitions`, commit**

If `four_lanes…` refuses on the device with `spark_hash_partition.cu`, that is #184 reached from
the smallest possible input — say so on the ticket, with the type of the key that reached it.

```bash
git commit -m "the scatter through the harness, lane by lane"
```

---

### Task 5: `GpuHashJoin`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`
- Modify: `mod.rs`, `coverage.rs`

Matrix row: nine types × one probe batch and two × `null_equals_null` both ways × a residual
filter where the matrix allows; with a projection. Tickets: #152, #159. The empty shapes are
Step 4, one case per (type, shape).

- [ ] **Step 1: The fixture**

The build side is `prefixed(synthetic, "b_")`, the probe `prefixed(synthetic, "p_")`, joined on
`key` (ordinal 1 on both). Output columns per type follow the capability matrix: both sides for
Inner/Left/Right/Full; the build side for LeftSemi/LeftAnti; the probe side for RightSemi/RightAnti;
the build side plus a `mark: Boolean` for LeftMark.

```rust
use datafusion::common::JoinType;
use crate::plan::{GpuHashJoin, JoinFilterColumn, JoinSide};

fn build_batch(rows: usize) -> RecordBatch { prefixed(&synthetic(rows, 11), "b_") }
fn probe_batch(rows: usize, seed: u64) -> RecordBatch { prefixed(&synthetic(rows, seed), "p_") }

fn output_of(join_type: JoinType) -> Schema {
    let build = Schema::new(build_batch(0).schema());
    let probe = Schema::new(probe_batch(0, 0).schema());
    let fields = |s: &Schema| s.fields.fields().iter().map(|f| f.as_ref().clone()).collect::<Vec<_>>();
    let all = match join_type {
        JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => [fields(&build), fields(&probe)].concat(),
        JoinType::LeftSemi | JoinType::LeftAnti => fields(&build),
        JoinType::RightSemi | JoinType::RightAnti => fields(&probe),
        JoinType::LeftMark => [fields(&build), vec![datafusion::arrow::datatypes::Field::new("mark", DataType::Boolean, false)]].concat(),
    };
    Schema::new(std::sync::Arc::new(datafusion::arrow::datatypes::Schema::new(all)))
}

/// `b_i64 < p_i64` over the filter schema, which is `[left columns…, right columns…]` as
/// `filter_columns` lists them — so 0 is the build's `i64` and 1 the probe's.
fn residual() -> (Option<Expr>, Vec<JoinFilterColumn>) {
    (
        Some(Expr::binary(Expr::column(0, "b_i64"), BinaryOp::Lt, Expr::column(1, "p_i64"), DataType::Boolean)),
        vec![JoinFilterColumn { side: JoinSide::Build, index: 3 }, JoinFilterColumn { side: JoinSide::Probe, index: 3 }],
    )
}

fn hash_join(join_type: JoinType, null_equals_null: bool, filtered: bool, projection: Option<Vec<u32>>) -> GpuHashJoin {
    let (filter, filter_columns) = if filtered { residual() } else { (None, vec![]) };
    let output = match &projection {
        None => output_of(join_type),
        Some(keep) => {
            let all = output_of(join_type);
            Schema::new(std::sync::Arc::new(all.fields.project(&keep.iter().map(|i| *i as usize).collect::<Vec<_>>()).unwrap()))
        }
    };
    GpuHashJoin::new(
        Given::of(Schema::new(build_batch(0).schema()), BatchLayout::SingleBatch),
        Given::of(Schema::new(probe_batch(0, 0).schema()), BatchLayout::MultipleBatches),
        join_type, vec![(1, 1)], filter, filter_columns, null_equals_null, projection, output,
    )
}

fn one_probe() -> Script {
    Script::Join { build: Some(build_batch(32)), probe: vec![probe_batch(48, 21)] }
}

fn two_probes() -> Script {
    Script::Join { build: Some(build_batch(32)), probe: vec![probe_batch(24, 21), probe_batch(24, 22)] }
}

const EVERY_TYPE: [JoinType; 9] = [
    JoinType::Inner, JoinType::Left, JoinType::Right, JoinType::Full,
    JoinType::LeftSemi, JoinType::RightSemi, JoinType::LeftAnti, JoinType::RightAnti, JoinType::LeftMark,
];
```

Check the field the `JoinSide` enum actually spells (`Build`/`Probe` or `Left`/`Right`) and the
`Schema` fields accessor (`fields` as `SchemaRef` or a method) against `plan/mod.rs` before the
first build.

- [ ] **Step 2: One case per type, one probe batch**

Nine cases, written out rather than looped, so each has a name a run reports:

```rust
operator_case! { GpuHashJoin, fn an_inner_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::Inner, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::Left, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::Right, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_full_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::Full, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_semi_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::LeftSemi, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_semi_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::RightSemi, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_anti_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::LeftAnti, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_anti_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::RightAnti, false, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_mark_join_over_one_probe_batch_agrees() {
    run_both(&hash_join(JoinType::LeftMark, false, false, None), one_probe()).same(Order::Any);
}}
```

Expected divergences, from `architecture.md`'s "What a streamed probe costs": `Left` and `Full`
may refuse outright on the device (#152 — the probe batch is read twice). #181's `unreachable!`
is guarded in the code (`gpu_backend/backend.rs` asks `answers_in_one_call` first), so an Inner
join is not expected to panic here; if it does, that is a new finding. The ticket itself stays
open either way — it closes when its corpus cells run, which is not this task. Read each failure
before deciding; the survey and the walk may have moved these.

- [ ] **Step 3: The streamed, null-equality, filtered, projected and empty cases**

```rust
// #152 — the build handle does not survive a streamed probe: the second probe batch of a
// build-local type is refused. One case per type that streams; green form first.
operator_case! { GpuHashJoin, fn an_inner_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::Inner, false, false, None), two_probes()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::Right, false, false, None), two_probes()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_semi_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::RightSemi, false, false, None), two_probes()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_right_anti_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::RightAnti, false, false, None), two_probes()).same(Order::Any);
}}
/// The build-side semi family streams by design: a probe call is only the key project.
operator_case! { GpuHashJoin, fn a_left_semi_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::LeftSemi, false, false, None), two_probes()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_anti_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::LeftAnti, false, false, None), two_probes()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_mark_join_over_two_probe_batches_agrees() {
    run_both(&hash_join(JoinType::LeftMark, false, false, None), two_probes()).same(Order::Any);
}}

/// `key` is null on every eleventh row of both sides; `true` matches them to each other.
operator_case! { GpuHashJoin, fn null_equals_null_matches_null_keys_on_an_inner_join() {
    run_both(&hash_join(JoinType::Inner, true, false, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn null_equals_null_is_honoured_by_a_semi_join() {
    run_both(&hash_join(JoinType::LeftSemi, true, false, None), one_probe()).same(Order::Any);
}}
/// Anti and mark hardcode EQUAL whatever the flag says (`architecture.md`, join types and
/// NULL key equality) — on both engines, which is what this asserts.
operator_case! { GpuHashJoin, fn an_anti_join_treats_null_keys_as_equal_on_both() {
    run_both(&hash_join(JoinType::LeftAnti, false, false, None), one_probe()).same(Order::Any);
}}

/// The matrix allows a residual on Inner and the build-side semi family. Left/Right/Full with
/// one is refused by the recipe writer (#153) and RightSemi/RightAnti by the planner (#159),
/// so neither has a case.
operator_case! { GpuHashJoin, fn an_inner_join_with_a_residual_filter_agrees() {
    run_both(&hash_join(JoinType::Inner, false, true, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_semi_join_with_a_residual_filter_agrees() {
    run_both(&hash_join(JoinType::LeftSemi, false, true, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_anti_join_with_a_residual_filter_agrees() {
    run_both(&hash_join(JoinType::LeftAnti, false, true, None), one_probe()).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn a_left_mark_join_with_a_residual_filter_agrees() {
    run_both(&hash_join(JoinType::LeftMark, false, true, None), one_probe()).same(Order::Any);
}}

operator_case! { GpuHashJoin, fn a_projection_keeps_the_same_columns() {
    run_both(&hash_join(JoinType::Inner, false, false, Some(vec![0, 5, 8, 13])), one_probe()).same(Order::Any);
}}

```

- [ ] **Step 4: The empty shapes, one case per (type, shape)**

Six shapes across nine types, written out — fifty-four cases, each named so a red run says
which combination reached the limit. A helper builds the script; the names carry the type and
the shape. The block below shows the six for `Inner`; repeat it for `Left`, `Right`, `Full`,
`LeftSemi`, `RightSemi`, `LeftAnti`, `RightAnti`, `LeftMark`, changing only the type and the
name's first word. Tickets to expect: #175 on Right, Full and RightAnti with an empty build
(the cpu pads, the device may not); #173 on the finish-pass types — Left, Full, LeftSemi,
LeftAnti, LeftMark — after only zero-row probes, the one site (`gpu_backend/join.rs`) that still
refuses by name; #152 wherever a second probe batch, even a zero-row one, follows a first.

```rust
fn empty_build(join_type: JoinType) -> Script { Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(16, 21)] } }
fn empty_probe(join_type: JoinType) -> Script { Script::Join { build: Some(build_batch(32)), probe: vec![probe_batch(0, 21)] } }
fn both_empty(join_type: JoinType) -> Script { Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(0, 21)] } }
fn no_build(join_type: JoinType) -> Script { Script::Join { build: None, probe: vec![] } }
fn empty_between(join_type: JoinType) -> Script { Script::Join { build: Some(build_batch(32)), probe: vec![probe_batch(16, 21), probe_batch(0, 22), probe_batch(16, 23)] } }
fn only_empty_probes(join_type: JoinType) -> Script { Script::Join { build: Some(build_batch(32)), probe: vec![probe_batch(0, 21), probe_batch(0, 22)] } }
```

(`join_type` is unused in the helpers and exists so a reader of a case sees the type twice —
drop it if clippy objects and name the type in the case alone.)

```rust
operator_case! { GpuHashJoin, fn inner_over_a_zero_row_build_answers_nothing() {
    run_both(&hash_join(JoinType::Inner, false, false, None), empty_build(JoinType::Inner)).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn inner_over_a_zero_row_probe_answers_nothing() {
    run_both(&hash_join(JoinType::Inner, false, false, None), empty_probe(JoinType::Inner)).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn inner_over_both_sides_empty_answers_nothing() {
    run_both(&hash_join(JoinType::Inner, false, false, None), both_empty(JoinType::Inner)).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn inner_with_no_build_batch_is_never_probed() {
    run_both(&hash_join(JoinType::Inner, false, false, None), no_build(JoinType::Inner)).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn inner_with_a_zero_row_probe_between_two_with_rows() {
    run_both(&hash_join(JoinType::Inner, false, false, None), empty_between(JoinType::Inner)).same(Order::Any);
}}
operator_case! { GpuHashJoin, fn inner_finishing_after_only_zero_row_probes() {
    run_both(&hash_join(JoinType::Inner, false, false, None), only_empty_probes(JoinType::Inner)).same(Order::Any);
}}
```

For the three types that owe rows the names say what the cpu does — `right_over_a_zero_row_build_pads_every_probe_row`,
`full_over_a_zero_row_build_pads_every_probe_row`, `right_anti_over_a_zero_row_build_keeps_every_probe_row`
— and for the finish-pass types the last shape is `…finishing_after_only_zero_row_probes_answers_from_the_build_alone`
(LeftAnti: every build row; LeftSemi: none; LeftMark: every build row marked false; Left, Full:
every build row padded).

- [ ] **Step 5: Run, `bug_`, shrink `PENDING` by `GpuHashJoin`, commit**

Each `bug_` names its ticket; a device refusal pinned with `outcome.gpu_refuses()` on the stable
part of the message (`unknown input handle` for #152's second probe). Where the device answers a
`Left` join that #152 says it cannot, the case is green and the ticket stays open: a ticket
closes when its corpus cells run, not when a hand-built case passes. Note it in the detail file
so the human knows the corpus cells are worth trying.

```bash
git commit -m "nine hash-join types through the harness"
```

---

### Task 6: `GpuCrossJoin` and `GpuNestedLoopJoin`

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_cases.rs`, `coverage.rs`

Matrix rows: cross — two batches, with a projection; nested-loop — Inner and Left with a
predicate, with a projection (#190, #160); and the empty shapes for both.

- [ ] **Step 1: The cases**

```rust
use crate::plan::{GpuCrossJoin, GpuNestedLoopJoin, NestedLoopJoinType};

fn cross(projection: Option<Vec<u32>>) -> GpuCrossJoin {
    let output = output_of(JoinType::Inner);
    let output = match &projection {
        None => output,
        Some(keep) => Schema::new(std::sync::Arc::new(output.fields.project(&keep.iter().map(|i| *i as usize).collect::<Vec<_>>()).unwrap())),
    };
    GpuCrossJoin::new(
        Given::of(Schema::new(build_batch(0).schema()), BatchLayout::SingleBatch),
        Given::of(Schema::new(probe_batch(0, 0).schema()), BatchLayout::SingleBatch),
        projection, output,
    )
}

operator_case! { GpuCrossJoin, fn a_cross_join_is_the_product_on_both() {
    run_both(&cross(None), Script::Join { build: Some(build_batch(8)), probe: vec![probe_batch(6, 21)] }).same(Order::Any);
}}
operator_case! { GpuCrossJoin, fn a_cross_join_projects_the_same_columns() {
    run_both(&cross(Some(vec![0, 8])), Script::Join { build: Some(build_batch(8)), probe: vec![probe_batch(6, 21)] }).same(Order::Any);
}}

fn nested(join_type: NestedLoopJoinType, projection: Option<Vec<u32>>) -> GpuNestedLoopJoin {
    let (filter, filter_columns) = residual();
    let output = output_of(match join_type { NestedLoopJoinType::Inner => JoinType::Inner, NestedLoopJoinType::Left => JoinType::Left });
    let output = match &projection {
        None => output,
        Some(keep) => Schema::new(std::sync::Arc::new(output.fields.project(&keep.iter().map(|i| *i as usize).collect::<Vec<_>>()).unwrap())),
    };
    GpuNestedLoopJoin::new(
        Given::of(Schema::new(build_batch(0).schema()), BatchLayout::SingleBatch),
        Given::of(Schema::new(probe_batch(0, 0).schema()), BatchLayout::SingleBatch),
        join_type, filter.expect("a residual"), filter_columns, projection, output,
    )
}

operator_case! { GpuNestedLoopJoin, fn an_inner_nested_loop_join_agrees() {
    run_both(&nested(NestedLoopJoinType::Inner, None), Script::Join { build: Some(build_batch(16)), probe: vec![probe_batch(16, 21)] }).same(Order::Any);
}}
operator_case! { GpuNestedLoopJoin, fn a_left_nested_loop_join_pads_the_unmatched() {
    run_both(&nested(NestedLoopJoinType::Left, None), Script::Join { build: Some(build_batch(16)), probe: vec![probe_batch(16, 21)] }).same(Order::Any);
}}
// #190 — the cpu backend drops a nested-loop join's projection. Green form first; the bug_
// form asserts the cpu's wider slot.
operator_case! { GpuNestedLoopJoin, fn a_nested_loop_join_projects_the_same_columns() {
    run_both(&nested(NestedLoopJoinType::Inner, Some(vec![0, 8])), Script::Join { build: Some(build_batch(16)), probe: vec![probe_batch(16, 21)] }).same(Order::Any);
}}

// Empty inputs, each its own case.
operator_case! { GpuCrossJoin, fn a_cross_join_over_a_zero_row_build_is_zero_rows() {
    run_both(&cross(None), Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(6, 21)] }).same(Order::Any);
}}
operator_case! { GpuCrossJoin, fn a_cross_join_over_a_zero_row_probe_is_zero_rows() {
    run_both(&cross(None), Script::Join { build: Some(build_batch(8)), probe: vec![probe_batch(0, 21)] }).same(Order::Any);
}}
operator_case! { GpuCrossJoin, fn a_cross_join_over_both_sides_empty_is_zero_rows() {
    run_both(&cross(None), Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(0, 21)] }).same(Order::Any);
}}
operator_case! { GpuNestedLoopJoin, fn an_inner_nested_loop_join_over_a_zero_row_build_is_zero_rows() {
    run_both(&nested(NestedLoopJoinType::Inner, None), Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(16, 21)] }).same(Order::Any);
}}
operator_case! { GpuNestedLoopJoin, fn an_inner_nested_loop_join_over_a_zero_row_probe_is_zero_rows() {
    run_both(&nested(NestedLoopJoinType::Inner, None), Script::Join { build: Some(build_batch(16)), probe: vec![probe_batch(0, 21)] }).same(Order::Any);
}}
operator_case! { GpuNestedLoopJoin, fn a_left_nested_loop_join_over_a_zero_row_build_is_zero_rows() {
    run_both(&nested(NestedLoopJoinType::Left, None), Script::Join { build: Some(build_batch(0)), probe: vec![probe_batch(16, 21)] }).same(Order::Any);
}}
operator_case! { GpuNestedLoopJoin, fn a_left_nested_loop_join_over_a_zero_row_probe_pads_every_build_row() {
    run_both(&nested(NestedLoopJoinType::Left, None), Script::Join { build: Some(build_batch(16)), probe: vec![probe_batch(0, 21)] }).same(Order::Any);
}}
```

- [ ] **Step 2: Run, `bug_`, shrink `PENDING` by the two, commit**

```bash
git commit -m "cross and nested-loop joins through the harness"
```

---

### Task 7: `GpuLoadParquet`

**Files:**
- Create: `peacockdb-core/src/tests/gpu_tests/source_cases.rs`
- Modify: `mod.rs`, `coverage.rs`

Matrix row: both backends read one parquet the test wrote — one batch per row group; a limit; row
groups and a limit together (#186, #188); a parquet of zero rows. The one helper this task adds, `write_parquet`, lives
here and nowhere else.

- [ ] **Step 1: The writer and the node**

```rust
use datafusion::parquet::arrow::ArrowWriter;
use datafusion::parquet::file::properties::WriterProperties;
use datafusion::parquet::file::reader::{FileReader, SerializedFileReader};
use crate::plan::{GpuLoadParquet, RowGroupMeta, ScanMetadata};

/// `synthetic(rows, seed)` as a parquet file of `rows_per_group`-row row groups, under the
/// temp dir, named by the test so two cases never share one.
fn write_parquet(name: &str, rows: usize, seed: u64, rows_per_group: usize) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("operator-cases-{name}-{}.parquet", std::process::id()));
    let batch = synthetic(rows, seed);
    let props = WriterProperties::builder().set_max_row_group_size(rows_per_group).build();
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = ArrowWriter::try_new(file, batch.schema(), Some(props)).unwrap();
    writer.write(&batch).unwrap();
    writer.close().unwrap();
    path
}

/// A one-lane scan of `path`, one batch per row group, every column projected.
fn scan(path: &std::path::Path, limit: Option<usize>) -> GpuLoadParquet {
    let reader = SerializedFileReader::new(std::fs::File::open(path).unwrap()).unwrap();
    let groups: Vec<RowGroupMeta> = (0..reader.metadata().num_row_groups())
        .map(|i| {
            let g = reader.metadata().row_group(i);
            RowGroupMeta { index: i as u32, rows: g.num_rows() as u64, bytes: g.total_byte_size() as u64 }
        })
        .collect();
    let per_group: Vec<Vec<u32>> = groups.iter().map(|g| vec![g.index]).collect();
    let scan = ScanMetadata { file: path.to_string_lossy().into_owned(), groups, can_be_null: vec![true; 8] };
    GpuLoadParquet::new("t".into(), (0..8).collect(), vec![per_group], &scan, limit, Schema::new(synthetic(0, 0).schema()))
}

operator_case! { GpuLoadParquet, fn both_backends_read_the_same_batches_per_row_group() {
    let path = write_parquet("per-group", 64, 1, 16);
    let outcome = run_both(&scan(&path, None), Script::Source { lane: 0 });
    std::fs::remove_file(&path).unwrap();
    outcome.same(Order::AsEmitted);
}}

// #186 — the cpu backend ignores a limit pushed into the scan; #188 — the device refuses a
// read with row groups and a limit together. Green form first; whichever side is wrong is
// the bug_ form's subject, and if both are, two bug_ tests.
operator_case! { GpuLoadParquet, fn a_pushed_down_limit_cuts_the_read_the_same_way() {
    let path = write_parquet("limit", 64, 1, 64);
    let outcome = run_both(&scan(&path, Some(10)), Script::Source { lane: 0 });
    std::fs::remove_file(&path).unwrap();
    outcome.same(Order::AsEmitted);
}}

operator_case! { GpuLoadParquet, fn row_groups_and_a_limit_together() {
    let path = write_parquet("groups-and-limit", 64, 1, 16);
    let outcome = run_both(&scan(&path, Some(10)), Script::Source { lane: 0 });
    std::fs::remove_file(&path).unwrap();
    outcome.same(Order::AsEmitted);
}}

// Empty input: a parquet of zero rows has no row group, so the mapping is one lane of no
// batches and both sources should be exhausted at the first call. If the writer emits one
// empty row group instead, the mapping has one batch of zero rows — either way one case.
operator_case! { GpuLoadParquet, fn a_parquet_of_zero_rows_reads_as_nothing_on_both() {
    let path = write_parquet("zero-rows", 0, 1, 16);
    let outcome = run_both(&scan(&path, None), Script::Source { lane: 0 });
    std::fs::remove_file(&path).unwrap();
    outcome.same(Order::AsEmitted);
}}
```

`ScanMetadata` may carry more fields than the three above; fill them as `test_gpu_executors`'
`mapped()` (now under `executor/gpu_backend/gpu_tests/`) does. The decimal column is where #187's
shape can reappear — cuDF's reader picks the narrowest fixed-point width — and a `Decimal128(18,
2)` that comes back as anything else is a schema difference `same` names; #187's task is on the
other chain, so a new ticket or a dated line on #187 is the call to make from the message.

- [ ] **Step 2: Run, `bug_`, shrink `PENDING` by `GpuLoadParquet`, commit**

```bash
git commit -m "the scan through the harness, over a parquet the test wrote"
```

`scan(&path, …)` over a file with no row group builds `partition_groups = vec![vec![]]`; if
`GpuLoadParquet::new` refuses an empty mapping, that refusal on a hand-built node is the case's
answer and goes in the detail file — the planner never emits one.

---

### Task 8: `PENDING` goes, the pages, the handoff

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/coverage.rs`
- Modify: `llm-wiki/build-test.md`, `llm-wiki/tickets.md`, `llm-wiki/tasks/operator-cases-detail.md`

- [ ] **Step 1: Delete `PENDING`**

It is empty. Remove the constant and its two uses in the guard, leaving `EXCLUDED` and the four
assertions over it. Run the guard once more with the whole module:

```bash
PCK_TEST_FILTER='tests::gpu_tests' timeout 7200 scripts/build-test-shadgpu.sh --all
```

Expected: every case green, `bug_` cases included; the guard green with the three forwarders as
the only exclusions.

- [ ] **Step 2: The row and the tickets**

`build-test.md`'s "Operator harness" row: the description gains "and every seq-bearing operator —
the exec three, both aggregates, the three accumulators, the scatter, nine hash-join types, cross,
nested-loop and the scan — each green or a `bug_` case naming its ticket"; the count is the
module's `--list` total; the grand total moves with it. Every new ticket is in `tickets.md`,
fifteen lines at most, and every `bug_` test's comment names one that exists — check with

```bash
grep -rn 'bug_' peacockdb-core/src/tests/gpu_tests/*.rs -B 3 | grep -o '#[0-9]\+' | sort -u | while read t; do grep -q "id=\"t${t#\#}\"" llm-wiki/tickets.md llm-wiki/tasks/active-tickets.md || echo "no ticket $t"; done
```

Expected: no output.

- [ ] **Step 3: The detail file**

Per family: which cases landed as `bug_`, under which ticket, and what the engine actually did —
the message or the wrong slot. Which tickets a case could not reproduce — the device answering
what its ticket says it cannot — so the human can try their corpus cells, which is what closes a
ticket. Which declared types had to be read off the planner rather than guessed. The human reads
this to decide what the next chain fixes.

- [ ] **Step 4: Commit**

```bash
git add peacockdb-core/src/tests/gpu_tests/coverage.rs llm-wiki/
git commit -m "every kind has a case; what the engine does wrong is a bug_ test each"
```
