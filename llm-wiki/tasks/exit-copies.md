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
- **ordinal subset of a fresh table:** `filter.cpp` 41 — a release with a distinct-ordinal assert;
- **an input column kept in the output:** `project.cpp` 44, `window.cpp` 46;
- **a temporary that only needed a view:** `expr.cpp` 834, `build_column`'s `ColumnRef` arm, which
  copies a whole column the caller views one line later — about 46 of the 107 GB q19's lineitem
  filter moves at sf40, on every predicate the AST refuses;
- **unresolved:** `aggregate.cpp` 413, 642, 644, 678, 680, 759, 771, each read and classed by this
  task.

## The work

1. `expr.cpp`: the `ColumnRef` arm returns a view of the input column (`table.column(idx)`); its
   callers take `column_view` already. No ownership change.
2. `filter.cpp`: release the fresh table's columns, asserting the kept ordinals are distinct — a
   repeated ordinal would move one column twice and leave a hole, a wrong answer and not a throw.
3. `project.cpp`, `window.cpp`: an output mixing input columns with computed ones is built with
   refcounted-scatter's `select` and `with`, so it views the input columns it keeps and owns the
   ones it computes, and the input's other columns are freed as today. A repeated
   `ColumnRef` is two entries sharing one owner, so the "moved twice, a hole" trap cannot arise;
   tpcds q84 and q85 (a column projected twice, device cells on at tp1-single) are its regression.
   If a site proves costlier to share than to copy, the task keeps that copy with a measured note
   — the PR says which.
4. `aggregate.cpp`: each of the seven read; a fresh-table site released as in 2, an input-column
   site as in 3, a site that must copy kept with a one-line reason.
5. Traps: a view taken before a release dangles (`ftv` near `join.cpp` ~L372 is the known one;
   none may survive here).

## Scope

| path | change |
|---|---|
| `cpp/src/expr.cpp`, `operators/filter.cpp`, `project.cpp`, `window.cpp`, `aggregate.cpp` | the sites |
| `cpp/tests/gpu/test_plan_executor.cpp` | the allocation cases |
| `llm-wiki/tickets/corpus-coverage.md`, `build-test.md` | #154 reworded to `join.cpp`'s sites; counts |

Component-level API: none; `TableResult` is unchanged (refcounted-scatter set it). No ABI, wire,
Rust or golden change.

## Restriction

The 11 sites; `join.cpp` is join-session-cpp's. No operator's output changes.

## Tests

One gtest per family (aggregate, filter, project, window, a non-AST predicate's `ColumnRef`)
under an RMM statistics adaptor: the call allocates the output's bytes, not twice them. A filter
with a repeated projection ordinal is refused. Every existing test green unchanged.

## Verification bar

- device: the gtests; the full gpu tier and the corpus unchanged; the sf40 q6 and q19 benchmark
  before and after, recorded.

## Device workflow

`build-test-shadgpu.sh`, two cycles.
