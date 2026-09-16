# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain A (base: master)

### 1. [`refcounted-tables.md`](refcounted-tables.md) — closes [#145](../tickets.md#t145), [#152](../tickets.md#t152) — state: new

The largest and the one that frees the most: 39 `.table` sites across 11 files, plus
`peacock_handle_retain`, a new ABI symbol — needed because `execute_one` takes its inputs by
value, so the registry cannot keep a handle and let an operator own its input unless the owner is
shared. 75 queries carry #152. Memory accounting is deliberately out of scope and will diverge.

## Chain C (base: master)

walk-drives-every-plan alone. declared-schemas is rejected and archived (2026-09-16); this task's
branch is built on `ENS-declared-schemas` and needs a decision — carry its `wire/gpu_tests/walk.rs`
onto master without the declared catalogue, or drop it with its base.

### 1. [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — state: blocked(completing) — PR #154 → `ENS-declared-schemas`, a rejected base; unverified there

**Conditional: its first task reads the survey's report, argues what is still worth teaching, and
stops for the human.** The walk is the only instrument that can ask what one call produced, and it
panics on ten shapes — eight never taught, two the engine truly cannot run. Teaches the eight, makes
a device refusal an outcome rather than an abort, and records the real limits as `bug_` tests naming
#136, #152 and #175. No production code. Deliverable: a checked table of what it can drive.


## Chain D (base: master)

The two fixes that do not need declared-schemas, rebased beside it.

### 1. [`typed-nulls.md`](typed-nulls.md) — closes [#198](../tickets.md#t198) — state: done — PR #152

`build_expr` builds ten literal scalars a second time and assumes validity, so a typed NULL inside an
AST expression is a typed zero — a wrong value in arithmetic and a wrong row count in a comparison.
One scalar builder, not a corrected copy. C++ only. Carries a test-helper repair first:
`CreateScalarValue` gained `is_null` as field 2 and the gtest call sites kept their old positions, so
every `make_int64_literal` builds the literal 0. Claims no cells.


### 2. [`empty-build.md`](empty-build.md) — closes [#175](../tickets.md#t175) — state: done — PR #153

A lane whose build side got no rows keeps its typed zero-row table where the join above owes rows, so
`Right`, `Full` and `RightAnti` answer instead of refusing. No new marker — the driver routes
`NoBuild` and `without_build` already asks `empty_build_answers_nothing`; only the `false` branch was
never written, and the table it needs is one the scatter builds and the driver drops. One derived
index field, one conditional drop. Replaces the parked `empty-answers.md`.

## Chain E (base: master)

Cases only, in `src/tests/gpu_tests/`: independent of every other chain's files. Both tasks
reopened: the specs no longer declare `Utf8View`, and each plan's Task 8 retires the view cases.

### 1. [`join-cases.md`](join-cases.md) — state: building — PR #155; Task 8 (retire the view cases) outstanding

Cases only, along the three dimensions operator-cases held constant and the corpus does not: a
projection on every reachable join type, composite and `Int64`/`Utf8`/`Date32` keys on the
three join code paths, and the non-AST nested-loop path. Roughly 27 cases, one device cycle, no production code.


### 2. [`aggregate-cases.md`](aggregate-cases.md) — state: building — PR #156; Task 8 (retire the view cases) outstanding

Cases only, after join-cases: group keys the corpus uses (`Utf8`, `Date32`, `Int64`, two
columns), `count(*)` and expression arguments, every merge arm with rows, the stddev finalize,
the project expressions and casts with no case, and sorted merges on `desc`, nullable and
composite keys — #202's second site. No fixture change, no production code. Roughly 40 cases.
