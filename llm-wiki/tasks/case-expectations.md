# Three expectations the cases let through

Kind: production

**Cases only; closes no ticket and fixes nothing.** An analyst's reading of join-cases and
aggregate-cases for enshrined behaviour found no green expectation written from a run and none
relaxed in review, and three places where the expectation is weaker than the case's name
claims. Third of chain E, after [`aggregate-cases.md`](aggregate-cases.md); the branch carries
both, so it edits their files in place.

## The three

1. **`same_within_welford` compares column 1's values and nothing else about it**
   (`src/tests/gpu_tests/aggregate_cases.rs:156-213`). Column 0 is compared exactly through
   `assert_same`; column 1 is read `as_primitive::<Float64Type>()` and compared within
   `WELFORD_RELATIVE`. Its name and declared type are never checked, so
   `a_grouped_stddev_finalize_agrees_within_welford` and the `var` twin
   (`aggregate_dimension_cases.rs:567-581`) would pass a renamed or retyped finalize. Right
   expectation: `names_and_types` exact over the whole batch — `compare.rs`'s own helper —
   then values approximate on column 1 alone. The tolerance stays the one place a float is
   compared inexactly.
2. **The composite-key form of #59 is sidestepped, not pinned** (`join-cases-detail.md:33-39`).
   `keyed_anti_script` first ran eight rows; row 4's `i32` is null, which under the hardcoded
   `EQUAL` matched on the device (cpu 30 rows, device 27) — [#59](../tickets.md#t59) on the
   *second* column of a composite key. The script was shrunk to four rows and the green case
   is honest, but the nine #59 pins are all single `Int32` keys, and the composite form has
   none. Right expectation: the eight-row probe as `bug_a_left_anti_join_on_a_composite_key_
   matches_a_null_in_the_second_column_on_the_device`, asserting the 27 rows the device answers
   and naming #59 beside the ticket's existing pins.
3. **`welford_partial` gives a null value a count of one** (`aggregate_cases.rs:683-694`:
   `count 1, mean f64, m2 0` for every row of `synthetic(32, seed)`, whose rows 12 and 25 carry a
   null `f64`). An init over one null row emits count 0, mean NULL, m2 0. As written, both
   backends fold a phantom `0.0` for those rows — DataFusion's `VarianceAccumulator::merge_batch`
   reads the mean ignoring validity and cuDF's `merge_m2` reads `d_means[idx]` the same way — and
   agree for the wrong reason. Two finalize cases and three `bug_` pins inherit the fixture.
   Right fixture: count 0 where the value is null. If a case then diverges, the divergence is
   real — a ticket and a `bug_`, not a fixture restored.

## Scope

| file | change |
|---|---|
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs` | `same_within_welford` compares names and types; `welford_partial` counts nulls as 0 |
| `peacockdb-core/src/tests/gpu_tests/join_dimension_cases.rs` | the composite-key #59 pin |
| `llm-wiki/tasks/join-cases-detail.md`, `aggregate-cases-detail.md` | the record; `build-test.md` counts and the `bug_` table |

No production code, no harness mechanism beyond the two helpers named, no other case edited.

## Restriction

No fix. A case that goes red under the tightened expectation is a ticket and a `bug_`. The
tolerance constant does not move.

## Verification bar

- rust-only: `--lib`, `test_module_layout`.
- device: `PCK_TEST_FILTER='_cases'` green with the one new pin; the two finalize cases and
  the three merge pins re-run under the corrected fixture, each still green or re-pinned with
  its reason in the detail file.

## Device workflow

`build-test-shadgpu.sh`, one cycle.
