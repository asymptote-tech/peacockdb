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

Schema divergence, end to end: four fixes at the sites the sink survey named, then the harness and
the driver hook that keep the boundary measured. In order; the fixes first so the measurements
after them are exact. Starts after chain E merges: task 1 assumes E's Task 8 retirements and task
5 reuses E's join builders. The wire and header move once, in task 2.

### 1. [`utf8-everywhere.md`](utf8-everywhere.md) — closes [#183](active-tickets.md#t183) — state: new

`schema_force_view_types = false` in `build_session_state`: the parquet scan is the only producer of
`Utf8View` in DataFusion 45, so every plan declares `Utf8` from the leaf and the export agrees with no
cast. A validation rule refuses any view type that returns; every handling site is deleted; goldens
regenerate; the survey's 76 string queries roll out at `tp1_single`.

### 2. [`decimal-precision-at-export.md`](decimal-precision-at-export.md) — closes [#187](active-tickets.md#t187) — state: new

cuDF's type is `{type_id, scale}`; precision is a label the export must be told. `peacock_result_from_handle`
takes a per-column declared precision, set on the imported Arrow schema (25.02 has no
`column_metadata::precision`); the dead widening becomes two hard failures; a rule refuses any decimal
but `Decimal128`. The chain's one wire/header rebuild: `output_schema` and the view values leave,
`peacock_handle_schema` arrives. 53 decimal queries roll out.

### 3. [`aggregate-state-types.md`](aggregate-state-types.md) — closes [#163](../tickets.md#t163) — state: new

`PlanAgg::state_type` types every state column by the aggregator that produces it, and `decompose` uses
it instead of copying DataFusion's accumulator layout; the CPU's Welford count gets the one cast; the
decimal `avg` finalize gains its explicit cast. Four pins retire; the 23 rows roll out.

### 4. [`date-part-return-type.md`](date-part-return-type.md) — closes [#191](active-tickets.md#t191) — state: new

`expr.cpp`'s `date_part` arm casts cuDF's INT16 to the `return_type` the wire already names. Three
plan-executor cases, one harness case written first as the pin, `tpch/q7`, `q8`, `q9`.

### 5. [`device-schema-harness.md`](device-schema-harness.md) — state: new

Testing only. `test_support::device_schema` reads a handle's schema through `peacock_handle_schema`,
projects an arrow schema onto `{type_id, scale}` and compares; a new schema case per node kind in
every harness family, and eleven hand-written spot-checks inside the recipe walk. Existing cases
untouched; a red case is a `bug_` with a ticket.

### 6. [`driver-output-hook.md`](driver-output-hook.md) — state: new

The driver takes an optional hook called on every emitted batch — the one production change. Under
the `test-support` feature: a validator holding each device batch to its node's schema, four
mock-driver unit tests, and a `schema_validation_enabled|disabled` argument on `corpus_query!`, on for
every enabled cell.

## Chain D (base: master)

The two fixes, rebased onto master after reaching `done` without a test run. The coordinator finishes
the rebase onto master's tip, runs the tiers, and writes `done` if all pass or `building` if not.

### 1. [`typed-nulls.md`](typed-nulls.md) — closes [#198](../tickets.md#t198) — state: completing — PR #152; rebased untested, see the spec's last section

`build_expr` builds ten literal scalars a second time and assumes validity, so a typed NULL inside an
AST expression is a typed zero — a wrong value in arithmetic and a wrong row count in a comparison.
One scalar builder, not a corrected copy. C++ only. Carries a test-helper repair first:
`CreateScalarValue` gained `is_null` as field 2 and the gtest call sites kept their old positions, so
every `make_int64_literal` builds the literal 0. Claims no cells.


### 2. [`empty-build.md`](empty-build.md) — closes [#175](../tickets.md#t175) — state: completing — PR #153; rebased untested, see the spec's last section

A lane whose build side got no rows keeps its typed zero-row table where the join above owes rows, so
`Right`, `Full` and `RightAnti` answer instead of refusing. No new marker — the driver routes
`NoBuild` and `without_build` already asks `empty_build_answers_nothing`; only the `false` branch was
never written, and the table it needs is one the scatter builds and the driver drops. One derived
index field, one conditional drop. Replaces the parked `empty-answers.md`.

## Chain E (base: master)

Cases only, in `src/tests/gpu_tests/`: independent of every other chain's files. Both tasks
reopened: the specs no longer declare `Utf8View`, and each plan's Task 8 retires the view cases.

### 1. [`join-cases.md`](join-cases.md) — state: completing — PR #155; Task 8 landed as `9b2350fc`, round 3 clean

Cases only, along the three dimensions operator-cases held constant and the corpus does not: a
projection on every reachable join type, composite and `Int64`/`Utf8`/`Date32` keys on the
three join code paths, and the non-AST nested-loop path. Roughly 27 cases, one device cycle, no production code.


### 2. [`aggregate-cases.md`](aggregate-cases.md) — state: rebase needed(building) — PR #156; Task 8 (retire the view cases) outstanding; its base `ENS-join-cases` moved onto master `fafaaaaf`

Cases only, after join-cases: group keys the corpus uses (`Utf8`, `Date32`, `Int64`, two
columns), `count(*)` and expression arguments, every merge arm with rows, the stddev finalize,
the project expressions and casts with no case, and sorted merges on `desc`, nullable and
composite keys — #202's second site. No fixture change, no production code. Roughly 40 cases.

### 3. [`case-expectations.md`](case-expectations.md) — state: approved to build

Cases only, on the branch that carries both: `same_within_welford` compares names and types before
its one tolerance; the composite-key form of #59 is pinned instead of sidestepped; `welford_partial`
counts a null value as 0 so the two backends stop agreeing on a phantom zero. A case that goes red
under the tightened expectation is a ticket and a `bug_`, never a fixture restored.

