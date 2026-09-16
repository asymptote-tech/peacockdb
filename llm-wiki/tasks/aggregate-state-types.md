# An aggregate's state is typed by the aggregator that produces it

Kind: production

**This task closes [#163](../tickets.md#t163)** — both arms: `avg` and Welford declare their
count state `UInt64` while both engines produce `Int64`, and the decimal `avg` finalize is typed
by arrow wider than the output it declares. Third of chain B.

## Why it happens

`decompose` (`planner/translator/aggregate.rs:126-157`) builds the partial node's state schema
from DataFusion's `state_fields()` — the layout of DataFusion's *own* accumulator — and
`declared_state` (`:74-92`) picks the field by name tag. But this engine does not run that
accumulator: `decomposition` (`plan/aggregates.rs:36`) rewrites `avg` into `PlanAgg::Count` +
`PlanAgg::Sum` and `stddev`/`var` into `Count` + `Mean` + `M2`, and our `Count` is `Int64` on
both backends — cuDF reduces `COUNT_VALID` to INT64 (`aggregate.cpp:262-265`, the Welford path
`:764-771`), the CPU's `avg` count comes from DataFusion's `count` UDAF (`count.rs:165`, `Int64`)
while its Welford count comes from the `stddev`/`var` UDAF (`variance.rs:115`, `UInt64`). The
declaration names one producer and the plan runs another. On the device nothing checks between
calls, so the state divides fine and the mismatch reached no sink in the survey; on the CPU
`check_state_layout` (`cpu_backend/mod.rs:441-442, 528-552`) refuses it at construction, before
any batch, which is why 23 rows have every `cpu_*` and `gpu_*` cell disabled.

`avg`'s `$sum` state is the same defect on a decimal: DataFusion's `state_fields()` declares it
at the *input* type (`average.rs:154-166`, `avg(x)$sum: Decimal128(7, 2)` in today's goldens),
but the CPU produces it through the `sum` UDAF at `(p + 10, s)`, and only the `widened_decimal`
escape in `check_state_layout` lets that through. Typing the state by its producer moves that
column too.

The finalize arm is the same principle at the next function: `finalize`'s `Avg` case
(`aggregates.rs:80-101`) casts both operands and stamps the divide with `out_type`, but what
arrow computes for `Decimal128(p,s) / Decimal128(p,0)` is wider — `(26,10)` where `(22,6)` is
declared — and the plan never asked for the narrowing.

## The work

1. **The rule.** In `plan/aggregates.rs`, beside `decomposition`:

       impl PlanAgg {
           pub(crate) fn state_type(self, input: &DataType) -> Result<DataType, PlanError>
       }

   matched without a wildcard, so a new variant does not compile until it says its type:
   - `Sum` — DataFusion's `sum::return_type`, quoted: `Decimal128(p, s) → Decimal128(min(38,
     p + 10), s)`; signed integers `→ Int64`; unsigned `→ UInt64`; floats `→ Float64`; anything
     else `PlanError::Unsupported("sum over <type>")`.
   - `Min`, `Max` — the input type.
   - `Count` — `Int64`.
   - `Mean`, `M2` — `Float64`.
   - `MergeM2` — `unreachable!("MergeM2 merges the Welford triple and declares no state")`.
     It is a merge rule, not a state producer: it appears only as `Merge::Combined` and never
     in a `state` slice, and `state_type` is called over `state` slices alone. That is a
     convention today; the test in 5 makes it a check.
2. **Use it.** `decompose` builds each state `Field` as `Field::new(name, state_type(arg)?,
   nullable)`; the name suffix is `rule.state`'s already, and `state_fields()` supplies arity
   and nullability only. `declared_state` stops returning a type.
3. **The one producer that moves.** The CPU's Welford init runs DataFusion's `stddev`/`var`
   UDAF, whose count is `UInt64`. The refusal is `check_state_layout` at construction
   (`cpu_backend/mod.rs:441`), so a cast on a batch cannot help: the init becomes a
   `ProjectionExec` over the `AggregateExec` with `CastExpr(col($count), Int64)` for that
   column and pass-through for the rest, and `check_state_layout` reads the projection's
   schema. `merge_m2.rs:44-76`'s declared layout follows the derived one. The device changes
   nowhere: its casts already say `Int64`. `widened_decimal` stays: the *merge* still produces
   `sum`'s wider type over an already-widened state, which is the escape's remaining reason.
4. **The finalize.** `finalize`'s `Avg` arm wraps the divide: `Expr::Cast { expr: divide,
   target: out_type }`. The device's pre-scaling (`expr.cpp:587-602`) already lands on the
   declared scale; the CPU stops refusing.
5. **Tests.** `plan/aggregates/tests.rs`: for each `PlanAgg` arm, run the CPU accumulator over a
   four-row batch of the arm's input type and assert the produced array's `data_type()` equals
   `state_type(arg)` — the "derived from its producer" half that keeps the table honest; plus
   `Sum` over `Decimal128(15, 2)` declares `(25, 2)` and over `Utf8` is refused. One more
   test walks `decomposition(func)` for every `AggFunc` and calls `state_type` on every entry
   of every `state` slice and every `Merge::PerColumn` list — so the whole table is exercised
   and `MergeM2`'s `unreachable!` is proven unreachable across every decomposition that
   exists, not assumed. And the producer half: for every `AggFunc`, the CPU init built by
   `init_aggregates` over a small batch has the schema `state_type` derives, exactly — the
   test that `check_state_layout` would now pass without its decimal escape for inits. The
   three `bug_` pins flip and retire with the ticket:
   `bug_a_welford_init_exports_its_count_as_int64`,
   `bug_a_welford_merge_exports_its_count_as_int64` (`gpu_tests/aggregate_cases.rs:263, 570`)
   and `bug_a_decimal_average_is_refused_on_the_cpu`. (`aggregate_cases.rs:294` is #187's pin,
   the previous task's.)
6. **Goldens.** Three classes and no other: every `avg`/`stddev`/`var` plan golden changes its
   count state column `UInt64 → Int64`; every decimal `avg`'s `$sum` state moves from the
   input type to `sum`'s `(p + 10, s)`; the `avg` finalize gains a cast in
   `recipe-payloads.txt`. Regenerate rust-only; a diff line outside those three is a finding.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `state_type`; the finalize cast |
| `peacockdb-core/src/planner/translator/aggregate.rs` | `decompose` derives; `declared_state` shrinks |
| `peacockdb-core/src/executor/cpu_backend/mod.rs`, `merge_m2.rs` | the init's `ProjectionExec`; layout follows |
| `peacockdb-core/src/plan/aggregates/tests.rs` (new), `executor/cpu_backend/tests/`, `tests/gpu_tests/aggregate_cases.rs` | the table tests; the producer test; three pins retired |
| `testdata/goldens/**`, `recipe-payloads.txt` | regenerated |
| `testdata/cost-registry.csv`, `tests/common/corpus_cases.inc` | the 23 rows |
| `llm-wiki/tickets.md`, `build-test.md`, `architecture.md` | #163 closed; counts; the aggregate paragraph says states are typed by `state_type` |

Component-level API: `PlanAgg::state_type`, `pub(crate)`. No wire change — the state schema is
already fields on the wire. No ABI change.

## Restriction

Output types stay DataFusion's — `finalize`'s `out_type` is the SQL contract and is not derived.
`Sum`'s decimal rule is quoted from DataFusion, not reinvented. No change to `aggregate.cpp`;
[#94](../tickets.md#t94) stays open and version-gated. Nothing casts a count to `UInt64`
anywhere.

## Registry

The 23 rows naming #163 at `tp1_single` on both backends: enabled where the CPU stops refusing
and the device's values match; a ticket where they do not; `163` struck from a row only when no
disabled cell in it is left without a ticket (`registry.rs:229-240`). Then the other four modes
for those that pass.

## Verification bar

- rust-only: `--lib` (the producer tests, the validation tests, plan goldens), `test_cpu_corpus`
  over the 23 rows' cpu cells.
- device: the harness with the four pins gone; the rollout.
- `git diff --stat testdata/goldens` shows only `avg`/`stddev`/`var` queries, and the diff's
  lines are the three classes above.

## Device workflow

`build-test-shadgpu.sh`, one cycle for the harness, one for the rollout.
