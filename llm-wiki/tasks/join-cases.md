# The joins along the dimensions the corpus varies

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Fourteenth in the chain, after [`operator-cases.md`](operator-cases.md), whose mechanism it
uses unchanged: a hand-built node over `Given` leaves, a `Script`, `run_both`, `assert_same`.
It adds cases along the dimensions task 9 did not vary, and nothing else.

## Why a second cases task, and why joins first

Task 9 proved every kind has a case. Read against the corpus, its join cases hold three things
constant that the corpus does not: the projection (only `Inner` projects; 634 of the 693 corpus
joins do), the key (one `Int32` everywhere; the corpus has 57 composite keys and `Int64` on
nearly every join), and the nested-loop predicate (always AST-able; six of the ten corpus nested
loops go down the cross-then-mask path). A wrong ordinal in a projection, a swapped index after
cuDF swaps a `Right` join's sides, a mask column read in the wrong order — each is a silently
wrong answer, and nothing on a device has ever asked. That is why joins come before the
aggregate, expression and merge dimensions, which are the next task.

The one thing the fixture lacks that every corpus string carries is `Utf8View`. It is a
declaration, not data: the corpus's strings enter through cuDF's parquet reader as plain
strings, and `Utf8View` is what the plan declares for them. cuDF's `from_arrow` has no
conversion for a `Utf8View` array, so the harness cannot upload one — a `Utf8View` case is a
`Given` leaf *declaring* `Utf8View` over an ordinary `Utf8` batch, which is exactly the
corpus's situation. The device exports it as `Utf8` at the sink ([#183](active-tickets.md#t183)),
whatever the operator did, so a case whose output carries the column is red for the export and
says nothing about the join: the new cases project the column away, and #183 gets a small,
deliberate set of pins rather than a hundred accidental ones. The first of them,
`bug_a_column_declared_utf8view_is_exported_as_utf8_and_the_sink_names_it`, is already in
`harness_cases.rs`.

## The matrix

The right column names the tickets a row may land on; a row with none may still find one.

| Row | Cases | May land on |
|---|---|---|
| Projection on every reachable type | `Right`, `RightSemi`, `RightAnti`, `LeftSemi`, `LeftAnti`, `LeftMark` (`Inner` exists): a projection that reorders, drops the keys, and takes columns from both sides where the type keeps both. `Left` and `Full` are not rows — [#152](../tickets.md#t152) refuses their first probe batch, pinned in task 9 | a wrong ordinal is a new ticket; [#153](../tickets.md#t153), [#159](../tickets.md#t159) are plan-time and have no row |
| Key type × three code paths | keys: two `Int32` (composite), `Int64`, `Utf8View` declared over `Utf8` data, `Date32`; each on `Inner` (the per-batch join), `Right` (cuDF swaps the sides and the indices are swapped back), `LeftAnti` (the accumulated-keys finish). Every `Utf8View`-keyed case projects the key away except one per path, which pins #183 at the join | [#183](active-tickets.md#t183), [#45](../tickets.md#t45) |
| Residual combinations on `Inner` | residual with a projection; residual with `null_equals_null = true`; residual over two probe batches; a residual on a string column and one on a decimal column, the column-path predicate | #152 for the second batch, pinned; otherwise new |
| Nested loop, non-AST predicate | `CAST(x AS Decimal128) > y`, the cross-then-mask path (`join.cpp`, mask columns in `filter_columns` order): `Inner` and `Left`, each with and without a projection. `Left` is admitted by the planner (`plan/join.rs` checks the batch layout only) and thrown by the C++ ("only supported for Inner joins"): a ticket first, then its `bug_` | [#190](active-tickets.md#t190), [#160](../tickets.md#t160), new |
| `Utf8View` through the operators | one case per family that passes the column through unchanged — project, filter, coalesce-all, emit on a string key; the unload's exists — asserting the export's `Utf8` exactly. The deliberate #183 set: a fix deletes these and nothing else | [#183](active-tickets.md#t183) |

### Empty inputs

Only where a new dimension changes the code path, each its own named case, never a loop:

| Shape | Cases |
|---|---|
| a zero-row build with a projection | `Right`, `LeftAnti` |
| a `Utf8View`-declared key over zero rows | `Inner`, once |
| the non-AST nested loop | an empty build; an empty probe |

Roughly 37 cases. The count is not the deliverable; a case that answers a question another case
already answered is one too many.

## The fixture

`join_cases.rs` gets a builder local to it, `keyed(rows, seed, key)`, for the composite,
`Int64`, `Utf8View`-declared and `Date32` keys; build and probe draw from one key domain so
every case has matches and misses. A `Utf8View` declaration is a schema transform over an
ordinary batch (`declaring_view_strings` in `harness_cases.rs` is the shape), never a
retyped batch: `synthetic.rs` is untouched. No other helper: a case that needs more is a
finding against the harness, recorded in the detail file, not a helper added here.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/join_cases.rs` and `nested_cases.rs`: the new cases and
  the `keyed` builder. `exec_cases.rs`, `accumulate_cases.rs`, `emit_cases.rs`: one #183 pin
  each; `harness_cases.rs` has the unload's.
- `llm-wiki/tickets.md`: a ticket per new defect — the `Left` nested-loop refusal at least;
  `llm-wiki/build-test.md`: the counts and the known-wrong table.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/`.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of task 9, unchanged: every divergence gets a ticket before it gets a `bug_` test; a
`bug_` test asserts the wrong behaviour precisely — the wrong value, the refusal's message —
not merely that the two sides differ; nothing here repairs, works around or casts away what a
case finds; the join scripts follow the capability matrix, build side one batch and always
first, probe streamed. And:

- Every `Utf8View` case but the pins projects the column out of its output, so the join is
  what the comparison reads.
- Every case is named for its shape. No loop over key types or join types: a red case says
  which combination it is by its name.
- The kind guard does not move: no kind gains or loses a case here, only rows.

## Verification bar

- Every row above and every empty shape present as a case, green or `bug_` with its ticket in
  the comment above it.
- New tickets in `tickets.md` within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s harness row carries the new count; the known-wrong table carries every new
  `bug_`; the grand total moves with them.
- Green on shad-gpu through `build-test-shadgpu.sh`, one device cycle; the rust-only lib
  unchanged.
