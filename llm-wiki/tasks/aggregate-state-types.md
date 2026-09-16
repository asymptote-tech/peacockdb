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
`:764-771`), the CPU's `avg` count comes from DataFusion's `count` UDAF. The declaration names one
producer and the plan runs another. On the device nothing checks between calls, so the state
divides fine and the mismatch reached no sink in the survey; on the CPU `declared_as` refuses it,
which is why 23 rows have every `cpu_*` and `gpu_*` cell disabled.

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
   true)`; `state_fields()` supplies the name suffix and the arity check only. `declared_state`
   stops returning a type.
3. **The one producer that moves.** The CPU's Welford init runs DataFusion's `stddev`/`var`
   UDAF, whose count is `UInt64` (`variance.rs:115`); `cpu_backend/mod.rs:449-466` casts that
   column to `Int64` after the accumulator. `merge_m2.rs:44-76`'s declared layout follows the
   derived one. The device changes nowhere: its casts already say `Int64`.
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
   exists, not assumed. The four
   `bug_` pins flip and retire with the ticket:
   `bug_a_welford_init_exports_its_count_as_int64`,
   `bug_a_welford_merge_exports_its_count_as_int64` (`gpu_tests/aggregate_cases.rs:263, 570`),
   `bug_a_decimal_average_is_refused_on_the_cpu`, and `aggregate_cases.rs:294`'s.
6. **Goldens.** Every `avg`/`stddev`/`var` plan golden changes its count state column
   `UInt64 → Int64`, and the `avg` finalize gains a cast in `recipe-payloads.txt`. Regenerate
   rust-only; the diff is those two things and nothing else, or the extra is a finding.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `state_type`; the finalize cast |
| `peacockdb-core/src/planner/translator/aggregate.rs` | `decompose` derives; `declared_state` shrinks |
| `peacockdb-core/src/executor/cpu_backend/mod.rs`, `merge_m2.rs` | the Welford count cast; layout follows |
| `peacockdb-core/src/plan/aggregates/tests.rs` (new), `tests/gpu_tests/aggregate_cases.rs` | the producer tests; four pins retired |
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
and the device's values match; a ticket where they do not. Then the other four modes for those
that pass.

## Verification bar

- rust-only: `--lib` (the producer tests, the validation tests, plan goldens), `test_cpu_corpus`
  over the 23 rows' cpu cells.
- device: the harness with the four pins gone; the rollout.
- `git diff --stat testdata/goldens` shows only `avg`/`stddev`/`var` queries.

## Device workflow

`build-test-shadgpu.sh`, one cycle for the harness, one for the rollout.
