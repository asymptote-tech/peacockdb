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

## Chain B (base: master)

Formerly `ENS-drop-mode-name`: drop-mode-name, module-layout, rmm-pool-budget, test-layout, test-support,
visibility, operator-harness and operator-cases merged and archived. What remains is the prototype,
whose product is on master.

### 1. [`sink-divergence-survey.md`](sink-divergence-survey.md) — state: done — prototype, branch `ENS-sink-divergence-survey` at `cebe1ead`, no PR; the report and the sink message are on master

**Prototype — no PR, branch never merged; the product is
[`reports/sink-divergence.md`](../reports/sink-divergence.md).** Sixty-odd disabled cells already fail
at the sink with a message naming neither the column nor either type, so a rollout over sixty queries
produces sixty identical lines. One error site changes to name column, declared and exported type;
the corpus rollout that was happening anyway then reports which divergence classes actually reach the
boundary and how often. Fixes nothing and enables no cell. It is what tells `declared-schemas` which
classes are worth a harness.


## Chain C (base: master)

declared-schemas and the task built on the walk it moved. Held: the human marked declared-schemas for
possible further changes before it merges, and walk-drives-every-plan waits with it.

### 1. [`declared-schemas.md`](declared-schemas.md) — state: blocked(done) — PR #151

The engine declares a schema per node and only the CPU backend is held to it — `declared_as` pulls
every stage back to the declaration, and the device path has no equivalent. Declares a schema per
*call* for the six arms that already have one, renders them in a new section of
`recipe-payloads.txt` rendered Rust-side so the wire does not move, and measures them on a device
through an exporter that casts to the declared type instead of relabelling. Fixes nothing: every
disagreement is a `bug_` test. Last in the chain because it needs `wire/gpu_tests/`.


### 2. [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — state: completing — PR #154 → `ENS-declared-schemas` — rebased off chain D, build and tests not yet re-run on the new base

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

Cases only, in `src/tests/gpu_tests/`: independent of every other chain's files.

### 1. [`join-cases.md`](join-cases.md) — state: approved to build

Cases only, along the three dimensions operator-cases held constant and the corpus does not: a
projection on every reachable join type, composite and `Int64`/`Utf8View`/`Date32` keys on the
three join code paths, and the non-AST nested-loop path, with `Utf8View` as a declaration over
plain strings and a deliberate handful of #183 pins. Roughly 37 cases, one device cycle, no production code.



### 2. [`aggregate-cases.md`](aggregate-cases.md) — state: approved to build

Cases only, after join-cases: group keys the corpus uses (`Utf8View`, `Date32`, `Int64`, two
columns), `count(*)` and expression arguments, every merge arm with rows, the stddev finalize,
the project expressions and casts with no case, and sorted merges on `desc`, nullable and
composite keys — #202's second site. No fixture change, no production code. Roughly 45 cases.
