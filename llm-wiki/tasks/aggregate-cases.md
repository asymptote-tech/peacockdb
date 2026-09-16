# Aggregates, expressions and sorted merges along the dimensions the corpus varies

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Fifteenth in the chain, after [`join-cases.md`](join-cases.md), whose mechanism — a hand-built
node over `Given` leaves, a `Script`, `run_both`, `assert_same` — it uses unchanged. It adds
cases and nothing else. Like join-cases it declares no view type: strings are `Utf8`, and the
corpus's `Utf8View` leaves with the `schema_force_view_types` flip, a task of its own.

## Why these dimensions

Task 9's aggregate cases group on one `Int32` key over a bare column. The corpus groups on
strings and dates (tpch q1 on two string keys is its commonest shape), counts with
`count(*)` 632 times, sums a CASE expression 48 times ([#56](../tickets.md#t56)), and keeps
thirteen queries out on the stddev finalize ([#163](../tickets.md#t163)). Its project cases
cover arithmetic, a cast and CASE; the corpus's `date_part` ([#191](active-tickets.md#t191)),
`substr` (77 plans), `coalesce` (40), a cast to decimal ([#55](../tickets.md#t55)) and the
unary forms have no case. Its sorted merges sort on `id`, ascending and never null; the corpus
has 24 `desc nulls_first` merges, and [#202](../tickets.md#t202)'s second site — the merge in
`node_session.cpp` — has no pin. None of these is a mechanism gap: each is a node and a script
the harness already drives, over a shape nobody wrote down.

## The matrix

The right column names the tickets a row may land on; a row with none may still find one.

| Row | Cases | May land on |
|---|---|---|
| Group keys | `GpuAggregate` grouped on `Utf8`, on `Date32`, on `Int64`, and on two columns (`Int32` and `Utf8`); each state carried through `GpuAggregateBatches`' merge | new |
| Count and expression arguments | `count(*)`, `count(1)`, `count(col)` over nulls; `sum(a * b)`; `sum(CASE … END)` grouped | [#180](active-tickets.md#t180), [#56](../tickets.md#t56) |
| Merge arms with rows | keyless `Sum`, `Count`, `Min`, `Max` and `MergeM2` over several arrivals; `Min` and `Max` merged grouped; a merge-level finalize that computes — `avg`'s divide at done; a merge over grouping-set state, `[keys, gid, state]` | [#199](../tickets.md#t199), pinned; new |
| Welford and its finalize | a global init; the stddev and var finalize — CASE, `Sqrt` and a typed NULL over the count — grouped and global | [#163](../tickets.md#t163) |
| Project expressions | `Minus`, `Modulo`; a comparison and an `AND`/`OR` as a projected column; `IsNull`, `Not`, `Negative`; casts `→ Decimal128`, `Decimal128 → Float64`, `Date32 → Utf8`, `Utf8 → Date32`; `date_part`; `substr`, `coalesce`, `concat`, `lower`, `round`; `NOT LIKE`, `ILIKE`; CASE with a typed-NULL branch; a string literal as a column | [#191](active-tickets.md#t191), [#55](../tickets.md#t55), [#198](../archive/archived-tickets.md#t198), [#203](../tickets.md#t203) |
| Sorts and merges on real keys | `GpuSort`: `fetch` with `desc`; `fetch` at and past the row count; `fetch 0`; a string key; a date key. `GpuAccumulateBatchesAndSort` and `GpuMergeSortedPartitions`, each: a `desc` key; a nullable key, nulls first and nulls last; two keys; 4 lanes | [#202](../tickets.md#t202) |

### Empty inputs

Only where a new path is reached, each its own named case, never a loop:

| Shape | Cases |
|---|---|
| a keyless merge whose every arrival has zero rows | `Sum`, once |
| a `desc` merge with one lane empty beside lanes with rows | `GpuMergeSortedPartitions`, once |
| `count(*)` over a zero-row batch | grouped and global |

Roughly 40 cases. The count is not the deliverable; a case that answers a question another case
already answered is one too many.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `exec_cases.rs`,
  `accumulate_cases.rs`: the new cases. The state fixtures an aggregate needs — `welford_state`,
  `columns`, the finalize builders — are already in `aggregate_cases.rs`; the new group-key
  states are declared beside them. `emit_cases.rs` only if the grouping-set merge needs a lane
  fixture that file already has.
- `llm-wiki/tickets.md`: a ticket per new defect; `llm-wiki/build-test.md`: the counts and the
  known-wrong table.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/`;
  `synthetic.rs` untouched.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of task 9, unchanged: every divergence gets a ticket before it gets a `bug_` test; a
`bug_` test asserts the wrong behaviour precisely — the wrong value, the refusal's message —
not merely that the two sides differ; nothing here repairs, works around or casts away what a
case finds. And:

- The Welford helper's `WELFORD_RELATIVE` is the one place a float is compared inexactly, and
  it stays the one place. A case that needs another tolerance is a finding against the harness,
  recorded in the detail file, not a second constant.
- Every case is named for its shape. No loop over functions, key types or sort options: a red
  case says which combination it is by its name.
- The kind guard does not move: no kind gains or loses a case here, only rows.

## Verification bar

- Every row above and every empty shape present as a case, green or `bug_` with its ticket in
  the comment above it.
- New tickets in `tickets.md` within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s harness row carries the new count; the known-wrong table carries every new
  `bug_`; the grand total moves with them.
- Green on shad-gpu through `build-test-shadgpu.sh`, one device cycle; the rust-only lib
  unchanged.

## Completeness signoff — 2026-09-16, rewritten after the reopening

Solved under its constraints, as amended: 65 cases against `ENS-join-cases` (29 in
`aggregate_dimension_cases.rs`, 23 in `exec_cases.rs`, 13 in `accumulate_cases.rs`), every
matrix row and empty shape named but one (`GpuAccumulateBatchesAndSort` at four lanes, a
one-lane node by definition — the detail file says why), no production code, no harness
addition, 13 `bug_` cases each with a ticket, #216–#219 new; strings plain `Utf8` and every
group-key cell a `.same()` on init and through the merge; `gpu_tests::` rung 385/385 on
shad-gpu, rust-only lib unchanged. Shortcuts, recorded in the detail file: the known-wrong table
does not exist on any base now that `declared-schemas` is rejected, so the `bug_` prefix, the
ticket comment above each case and the detail file's register are the record; the keyless
`Count` merge is folded into `Sum` and `count(1)` into `count(*)`, both plan facts.
