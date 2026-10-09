# Operators hand their output over instead of copying it

Kind: production

**This task closes [#154](../tickets/corpus-coverage.md#t154)** (every operator exit path
deep-copies its output into a fresh table) for the 11 sites outside `join.cpp`; join-session-cpp
rewrites `join.cpp` and takes its 10. Sixth of the join-rewrite chain, after refcounted-scatter,
whose `TableResult` (one owner per column, final shape) it uses without changing it.

## Why it happens

`std::make_unique<cudf::column>(view)` copies the device buffer, and the operators use it to
build a table out of one they just produced, or out of their input. #154 lists the sites by kind.
Outside `join.cpp`:
- **ordinal subset of a fresh table:** `filter.cpp` 41 — a `select` over the fresh table (a repeated ordinal is two entries sharing one owner);
- **an input column kept in the output:** `project.cpp` 44, `window.cpp` 46;
- **a temporary that only needed a view:** `expr.cpp` 834, `build_column`'s `ColumnRef` arm, which
  copies a whole column the caller views one line later — about 46 of the 107 GB q19's lineitem
  filter moves at sf40, on every predicate the AST refuses;
- **unresolved:** `aggregate.cpp` 413, 642, 644, 678, 680, 759, 771, each read and classed by this
  task.

## The work

1. `expr.cpp`: the `ColumnRef` arm returns a view of the input column (`table.column(idx)`); its
   callers take `column_view` already. No ownership change.
2. `filter.cpp`: the kept columns are `select`ed from the fresh table's owners, so a repeated
   ordinal is two entries sharing one owner and answers correctly — the "moved twice, a hole" trap
   of a release cannot arise, and nothing is refused.
3. `project.cpp`, `window.cpp`: an output mixing input columns with computed ones is built with
   refcounted-scatter's `select` and `with`, so it views the input columns it keeps and owns the
   ones it computes, and the input's other columns are freed as today. A repeated
   `ColumnRef` is two entries sharing one owner, so the "moved twice, a hole" trap cannot arise;
   tpcds q84 and q85 (a column projected twice, device cells on at tp1-single) are its regression.
   If a site proves costlier to share than to copy, the task keeps that copy with a measured note
   — the PR says which.
4. `aggregate.cpp`: each of the seven read; a fresh-table site selected as in 2, an input-column
   site as in 3, a site that must copy kept with a one-line reason. Expected outcome, from reading
   them: 6 of the 11 sites outside `join.cpp` lose their copy; 5 keep it — the four Welford struct
   members and one merged-state child, whose copies are small and whose shared form would cost
   more than it saves. The PR lists each with its reason.
5. Traps: a view taken before a release dangles (`ftv` near `join.cpp` ~L372 is the known one;
   none may survive here).

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp`, `cpp/src/peacock/expr.h`, `operators/filter.cpp`, `project.cpp`, `window.cpp`, `aggregate.cpp`, `sort.cpp` | the sites; `evaluate_column`'s borrowed view |
| `cpp/tests/gpu/test_plan_executor.cpp` | the allocation cases |
| `llm-wiki/tickets/corpus-coverage.md`, `build-test.md` | #154 reworded to `join.cpp`'s sites; counts |

Component-level API: none; `TableResult` is unchanged (refcounted-scatter set it). No ABI, wire,
Rust or golden change.

## Restriction

The 11 sites; `join.cpp` is join-session-cpp's. No operator's output changes.

## Tests

One gtest per family (aggregate, filter, project, window, a non-AST predicate's `ColumnRef`)
under an RMM statistics adaptor: the call allocates the output's bytes, not twice them. A filter
with a repeated projection ordinal answers both columns correctly. Every existing test green unchanged.

## Verification bar

- device: the gtests; the full gpu tier and the corpus unchanged; the sf40 q6 and q19 benchmark
  before and after, recorded.

## Device workflow

`build-test-shadgpu.sh`, two cycles.

## Completeness signoff (2026-10-09)

Solved under its constraints. **Six of the eleven sites lose their copy, five keep it** — §4's
prediction — and `join.cpp`'s ten are untouched for the next task. Item 3 landed by a different
mechanism than the spec named: `project.cpp` assembles its handle through the public fields rather
than `select`/`with`, because no constructor can rename a kept column, with `register_handle` as
the shape check. The outcome is identical, repeated ordinals included.

**The nine gtests are the whole of the evidence, and two of them exist because the other seven
could not fail.** Seven bound an allocation, each against the same cuDF work on the same input
rather than a formula, and all seven came out exactly equal; all nine were watched red first. The
two added by the completeness pass bound a *release*, and the run that proved they were needed is
the argument for them: with both operators deliberately over-retaining, 75 of 77 cases passed —
every byte bound green at its exact figure — and only these two went red.

**Deferred, by the human host override and not by this task:** the Verification bar's sf40 q6 and
q19 benchmark before and after. sf40 lives only on shad-gpu, down throughout. So the saving at
`expr.cpp`'s `ColumnRef` arm — about 46 of the 107 GB q19's lineitem filter moves at sf40 — is
**never quantified at that scale**, and the gtest table at 122,880 rows stands in for it. That is
the one gap in this task's evidence. No bandaid applied.
