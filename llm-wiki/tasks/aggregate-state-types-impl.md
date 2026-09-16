# Aggregate state types implementation plan

**Goal:** Every aggregate state column is typed by `PlanAgg::state_type`, the CPU's Welford count
is `Int64` like everything else's, the decimal `avg` finalize asks for its narrowing — closing
#163 on both arms.

**Architecture:** One function on the enum owns the table; `decompose` reads it instead of
DataFusion's accumulator layout; a `ProjectionExec` over the CPU's Welford init and one cast in
`finalize` bring the two arms to the declaration. The device changes nowhere.

**Tech stack:** Rust, rust-only for everything but the harness pins and the rollout.

**Spec:** [`aggregate-state-types.md`](aggregate-state-types.md) — frozen.

## Global constraints

- Output types are DataFusion's; only *state* types are derived. `Sum`'s decimal rule is quoted
  from `datafusion-functions-aggregate/src/sum.rs:153-168`, not reinvented.
- No change under `cpp/`. #94 stays open.
- The golden diff is exactly three classes: count state columns `UInt64 → Int64`; decimal
  `avg` `$sum` states from the input type to `(p + 10, s)`; the `avg` finalize's cast in
  `recipe-payloads.txt`. Anything else is a finding.
- No wildcard arm in `state_type`.
- `rustfmt`; commits at most 10 lines; device cycles foreground.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs` | `state_type`; the finalize cast |
| `peacockdb-core/src/plan/aggregates/tests.rs` (new; `mod tests` under `#[cfg(test)]` in `aggregates.rs`) | the producer tests, the decomposition walk |
| `peacockdb-core/src/planner/translator/aggregate.rs` | `decompose` derives; `declared_state` returns the name only |
| `peacockdb-core/src/executor/cpu_backend/mod.rs` | the init wrapped in a `ProjectionExec` casting `$count` when the accumulator's type differs from the declaration |
| `peacockdb-core/src/executor/cpu_backend/tests/` | the producer test: every decomposition's init schema equals the derived state |
| `peacockdb-core/src/executor/cpu_backend/merge_m2.rs` | `state_fields`/signature on `Int64` count |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs` | three pins retired |

---

### Task 1: `state_type`

**Files:**
- Modify: `peacockdb-core/src/plan/aggregates.rs` (after `decomposition`, `:36-74`)
- Create: `peacockdb-core/src/plan/aggregates/tests.rs`

**Interfaces:**
- Produces: `impl PlanAgg { pub(crate) fn state_type(self, input: &DataType) -> Result<DataType, PlanError> }`.

- [ ] **Step 1: The failing tests.** `aggregates/tests.rs`:

```rust
use super::*;
use datafusion::arrow::datatypes::DataType::*;

#[test]
fn a_count_state_is_int64_whatever_it_counts() {
    for input in [Int32, Utf8, Decimal128(15, 2), Date32] {
        assert_eq!(PlanAgg::Count.state_type(&input).unwrap(), Int64);
    }
}
#[test]
fn a_sum_state_is_datafusions_sum_type() {
    assert_eq!(PlanAgg::Sum.state_type(&Decimal128(15, 2)).unwrap(), Decimal128(25, 2));
    assert_eq!(PlanAgg::Sum.state_type(&Decimal128(30, 6)).unwrap(), Decimal128(38, 6));
    assert_eq!(PlanAgg::Sum.state_type(&Int32).unwrap(), Int64);
    assert_eq!(PlanAgg::Sum.state_type(&UInt16).unwrap(), UInt64);
    assert_eq!(PlanAgg::Sum.state_type(&Float32).unwrap(), Float64);
    assert!(PlanAgg::Sum.state_type(&Utf8).is_err());
}
#[test]
fn min_and_max_keep_the_input_type() { for t in [Utf8, Date32, Decimal128(15, 2), Int32] { … } }
#[test]
fn the_welford_moments_are_float64() { /* Mean, M2 over Int32 and Decimal128 → Float64 */ }
#[test]
fn every_decomposition_types_every_state_it_names() {
    for func in [AggFunc::Sum, AggFunc::Min, AggFunc::Max, AggFunc::Count, AggFunc::Avg, AggFunc::Stddev, AggFunc::Var] {
        let rule = decomposition(func);
        for (_, agg) in rule.state { agg.state_type(&Decimal128(15, 2)).unwrap(); }
        if let Merge::PerColumn(funcs) = rule.merge { for agg in funcs { agg.state_type(&Decimal128(15, 2)).unwrap(); } }
    }
}
```

  The last one is what proves `MergeM2`'s `unreachable!` unreachable: it appears in no
  `state` slice and no `PerColumn` list, so the walk never asks it.

- [ ] **Step 2: Red.** `cargo test --features rust-only -p peacockdb-core --lib --
  plan::aggregates::tests` — no such method.

- [ ] **Step 3: The function.**

```rust
impl PlanAgg {
    /// The type of the state column this aggregator produces, given the argument's type.
    /// Both engines produce it; the plan declares it. No wildcard: a new aggregator says its
    /// type here or does not compile.
    pub(crate) fn state_type(self, input: &DataType) -> Result<DataType, PlanError> {
        use DataType::*;
        Ok(match self {
            Self::Sum => match input {
                // DataFusion's sum::return_type, quoted: Spark's DECIMAL(min(38, p + 10), s).
                Decimal128(p, s) => Decimal128((*p + 10).min(38), *s),
                t if t.is_signed_integer() => Int64,
                t if t.is_unsigned_integer() => UInt64,
                t if t.is_floating() => Float64,
                other => return Err(PlanError::Unsupported(format!("sum over {other}"))),
            },
            Self::Min | Self::Max => input.clone(),
            Self::Count => Int64,
            Self::Mean | Self::M2 => Float64,
            Self::MergeM2 => unreachable!("MergeM2 merges the Welford triple and declares no state"),
        })
    }
}
```

- [ ] **Step 4: Green.** Same command. Add `#[cfg(test)] mod tests;` to `aggregates.rs`.
- [ ] **Step 5: Commit.** `git commit -m "PlanAgg::state_type: a state column is typed by the aggregator that produces it"`.

### Task 2: `decompose` derives

**Files:**
- Modify: `peacockdb-core/src/planner/translator/aggregate.rs:74-92,145-157`

- [ ] **Step 1:** In the state loop (`:149-156`) replace `field.data_type().clone()` with
  `func.state_type(&arg_type)?`, where `arg_type` is the aggregate's first argument's type —
  `aggregate.expressions()[0].data_type(input_schema)?` (a `count(*)` has no argument: use
  `DataType::Null`, which `Count` ignores). `declared_state` keeps returning the field for
  its nullability (the suffix is `rule.state`'s); delete the comment's "the types are
  DataFusion's" sentence and say they are `state_type`'s.
- [ ] **Step 2:** `cargo test --features rust-only -p peacockdb-core --lib --
  planner::tests::plan_goldens` — red on every `avg`/`stddev`/`var` golden, each diff line a
  count column `UInt64 → Int64` or a decimal `avg`'s `$sum` from the input type to `(p+10,
  s)`. Read three of them to confirm. Do not regenerate yet.
- [ ] **Step 3:** `-- planner::translator` and `-- plan::` tests green.
- [ ] **Step 4: Commit.** `git commit -m "decompose types the state from state_type; DataFusion's layout supplies names and arity"`.

### Task 3: The CPU's Welford count

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/mod.rs:430-447` (the init `AggregateExec`), `merge_m2.rs:44-76`
- Test: `peacockdb-core/src/executor/cpu_backend/tests/` (the aggregate module there, or a new `state_types.rs`)

- [ ] **Step 1: Failing tests.** (a) A `stddev` init over a four-row `Float64` batch built
  through the backend's `executors_for`: today it fails at *construction* — `check_state_layout`
  (`mod.rs:441`) refuses "column 0 is Int64 in the declared state and UInt64 in the one
  DataFusion's accumulators produce" — assert instead that construction succeeds and the
  emitted state batch's count column is `Int64`. (b) The producer test the spec asks for: for
  every `AggFunc` (`sum`, `min`, `max`, `count`, `avg`, `stddev`, `var`) over a `Decimal128(15,
  2)` and an `Int32` argument, build the init through the backend and assert its output schema
  equals the state `decompose` derived, column by column — no escape needed for an init.
- [ ] **Step 2: The projection.** In the init builder (`:433-441`): after `AggregateExec::try_new`,
  compare `aggregate.schema()` to the declared state; where a column differs by exactly
  `UInt64 → Int64` (the Welford count), wrap the aggregate in a
  `ProjectionExec::try_new(exprs, Arc::new(aggregate))` whose `exprs` are `CastExpr(Column(i),
  Int64)` for that column and `Column(i)` for every other, and hand `check_state_layout` the
  projection's schema. Comment: DataFusion's variance accumulator counts in `u64`; the plan and
  the device count in `Int64`, and a state read positionally has to be the state declared.
  `widened_decimal` stays for the merge's `sum`-of-`sum` widening.
- [ ] **Step 3:** `merge_m2.rs`: its `state_fields`/`signature` declare the count `Int64`;
  `MergeM2`'s accumulator reads `Int64Array` for the count.
- [ ] **Step 4: Green:** both tests; `-- executor::cpu_backend`; `test_cpu_corpus` over three
  `stddev` queries (`PCK_TEST_FILTER` on tpcds q17, q29, q39 or the file's own choice).
- [ ] **Step 5: Commit.** `git commit -m "the cpu's welford count is Int64 like every other count; inits match their declared state exactly"`.

### Task 4: The finalize cast

**Files:**
- Modify: `peacockdb-core/src/plan/aggregates.rs:80-101`

- [ ] **Step 1: Failing test.** `aggregates/tests.rs`: `finalize(AggSpec{Avg,0}, &state,
  0, &Decimal128(22, 6))` returns `Expr::Cast { target: Decimal128(22, 6), expr: Binary{Divide,..} }`.
- [ ] **Step 2:** Wrap the `Avg` arm's `Expr::binary(...)` in `Expr::Cast { expr: Box::new(…),
  target: out_type.clone() }` for the decimal case only (the non-decimal arm's types already
  agree). Comment: arrow types the divide wider than the declaration; the plan asks for the
  narrowing explicitly, as every cast is.
- [ ] **Step 3: Green**; then the plan goldens — the `avg` finalizes gain a cast in
  `recipe-payloads.txt` and nowhere else.
- [ ] **Step 4: Commit.** `git commit -m "the decimal avg finalize casts to the type it declares"`.

### Task 5: Goldens

- [ ] `UPDATE_CANONICAL=1` over `planner::tests::plan_goldens` and the recipe-payloads test.
  `git diff testdata/goldens | grep '^[-+]' | grep -v '^[-+][-+]' | grep -v 'UInt64\|Int64\|\$sum\|CAST\|cast'`
  is empty; `--stat` names only `avg`/`stddev`/`var` queries; every `$sum` line moves from the
  input precision to `p + 10` at the same scale. Full rust-only tier green.
- [ ] `git commit -m "goldens: count states are Int64; the avg finalize carries its cast"`.

### Task 6: The device — pins, then the rollout

- [ ] **Step 1:** Harness cycle, `PCK_TEST_FILTER='tests::gpu_tests::aggregate'`: the three
  pins (`bug_a_welford_init_exports_its_count_as_int64`, `…_merge_…`,
  `bug_a_decimal_average_is_refused_on_the_cpu`) are red because the bug is gone — rewrite each
  as the green case its name implies, dropping `bug_` and the ticket line; rerun green.
  (`aggregate_cases.rs:294` is #187's pin and was retired by the previous task.)
- [ ] **Step 2:** The 23 rows naming `163` in `cost-registry.csv`: `cpu_tp1_single` and
  `gpu_tp1_single` on; run `test_cpu_corpus` locally and the gpu corpus on the device. Enable
  what is green; ticket what fails on values; leave what fails above the sink on its own ticket
  (`q39` #57, `q9` #63, `q6` #152). Strike `163` from a row only when every cell still disabled
  in it carries another ticket (`registry.rs:229-240`); the registry test green before the push.
- [ ] **Step 3:** Close #163 in `tickets.md` (both arms named); `architecture.md`'s aggregate
  paragraph: "state columns are typed by `PlanAgg::state_type`"; `build-test.md` counts and
  `bug_` table.
- [ ] **Step 4: Commit.** `git commit -m "#163 closed: state typed by its producer, the finalize by its declaration; N cells enabled"`.

### Task 7: The record

- [ ] Detail file: the golden diff summary, the harness lines, the rollout table.
  `git commit -m "aggregate-state-types: the record"`.
