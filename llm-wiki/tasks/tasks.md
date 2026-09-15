# Task board

One `##` section per chain, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain ENS-refcounted-tables (base: master)

### 1. [`refcounted-tables.md`](refcounted-tables.md) — closes [#145](../tickets.md#t145), [#152](../tickets.md#t152) — state: new

The largest and the one that frees the most: 39 `.table` sites across 11 files, plus
`peacock_handle_retain`, a new ABI symbol — needed because `execute_one` takes its inputs by
value, so the registry cannot keep a handle and let an operator own its input unless the owner is
shared. 75 queries carry #152. Memory accounting is deliberately out of scope and will diverge.

## Chain ENS-drop-mode-name (base: master)

The layout refactor. It may not run beside the ENS-refcounted-tables chain: both rewrite the same tree, so
rebasing one across the other is a whole-tree conflict.

### 7. [`sink-divergence-survey.md`](sink-divergence-survey.md) — state: approved to build

**Prototype — no PR, branch never merged; the product is
[`reports/sink-divergence.md`](../reports/sink-divergence.md).** Sixty-odd disabled cells already fail
at the sink with a message naming neither the column nor either type, so a rollout over sixty queries
produces sixty identical lines. One error site changes to name column, declared and exported type;
the corpus rollout that was happening anyway then reports which divergence classes actually reach the
boundary and how often. Fixes nothing and enables no cell. It is what tells `declared-schemas` which
classes are worth a harness.

### 10. [`declared-schemas.md`](declared-schemas.md) — state: approved to build

The engine declares a schema per node and only the CPU backend is held to it — `declared_as` pulls
every stage back to the declaration, and the device path has no equivalent. Declares a schema per
*call* for the six arms that already have one, renders them in a new section of
`recipe-payloads.txt` rendered Rust-side so the wire does not move, and measures them on a device
through an exporter that casts to the declared type instead of relabelling. Fixes nothing: every
disagreement is a `bug_` test. Last in the chain because it needs `wire/gpu_tests/`.

### 11. [`typed-nulls.md`](typed-nulls.md) — closes [#198](../tickets.md#t198) — state: approved to build

`build_expr` builds ten literal scalars a second time and assumes validity, so a typed NULL inside an
AST expression is a typed zero — a wrong value in arithmetic and a wrong row count in a comparison.
One scalar builder, not a corrected copy. C++ only. Carries a test-helper repair first:
`CreateScalarValue` gained `is_null` as field 2 and the gtest call sites kept their old positions, so
every `make_int64_literal` builds the literal 0. Claims no cells.

### 12. [`empty-build.md`](empty-build.md) — closes [#175](../tickets.md#t175) — state: approved to build

A lane whose build side got no rows keeps its typed zero-row table where the join above owes rows, so
`Right`, `Full` and `RightAnti` answer instead of refusing. No new marker — the driver routes
`NoBuild` and `without_build` already asks `empty_build_answers_nothing`; only the `false` branch was
never written, and the table it needs is one the scatter builds and the driver drops. One derived
index field, one conditional drop. Replaces the parked `empty-answers.md`.

### 13. [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — state: approved to build

**Conditional: its first task reads the survey's report, argues what is still worth teaching, and
stops for the human.** The walk is the only instrument that can ask what one call produced, and it
panics on ten shapes — eight never taught, two the engine truly cannot run. Teaches the eight, makes
a device refusal an outcome rather than an abort, and records the real limits as `bug_` tests naming
#136, #152 and #175. No production code. Deliverable: a checked table of what it can drive.

### 14. [`join-cases.md`](join-cases.md) — state: new

Cases only, along the three dimensions task 9 held constant and the corpus does not: a
projection on every reachable join type, composite and `Int64`/`Utf8View`/`Date32` keys on the
three join code paths, and the non-AST nested-loop path. One fixture form, `synthetic_extended`,
and a deliberate handful of #183 pins. Roughly 37 cases, one device cycle, no production code.

### 15. [`aggregate-cases.md`](aggregate-cases.md) — state: new

Cases only, after task 14: group keys the corpus uses (`Utf8View`, `Date32`, `Int64`, two
columns), `count(*)` and expression arguments, every merge arm with rows, the stddev finalize,
the project expressions and casts with no case, and sorted merges on `desc`, nullable and
composite keys — #202's second site. No fixture change, no production code. Roughly 45 cases.
