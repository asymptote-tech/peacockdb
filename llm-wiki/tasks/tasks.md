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
after them are exact. Chain E's Task 8 retirements are on its branches and merge first; task 5
reuses E's join builders. The wire and header move once, in task 2.

### 1. [`utf8-everywhere.md`](utf8-everywhere.md) — closes [#183](active-tickets.md#t183) — state: done — PR #158

`schema_force_view_types = false` in `build_session_state`: the parquet scan is the only producer of
`Utf8View` in DataFusion 45, so every plan declares `Utf8` from the leaf and the export agrees with no
cast. A validation rule refuses any view type that returns; every handling site is deleted; goldens
regenerate; the survey's 76 string queries roll out at `tp1_single`.

### 2. [`decimal-precision-at-export.md`](decimal-precision-at-export.md) — closes [#187](active-tickets.md#t187) — state: done — PR #159

cuDF's type is `{type_id, scale}`; precision is a label the export must be told. `peacock_result_from_handle`
takes a per-column declared precision, set on the imported Arrow schema (25.02 has no
`column_metadata::precision`); the dead widening becomes two hard failures; a rule refuses any decimal
but `Decimal128`. The chain's one wire/header rebuild: `output_schema` and the view values leave,
`peacock_handle_schema` arrives. 53 decimal queries roll out.

### 3. [`aggregate-state-types.md`](aggregate-state-types.md) — closes [#163](../tickets.md#t163) — state: done — PR #160

`PlanAgg::state_type` types every state column by the aggregator that produces it, and `decompose` uses
it instead of copying DataFusion's accumulator layout; the CPU's Welford count gets the one cast; the
decimal `avg` finalize gains its explicit cast. Four pins retire; the 23 rows roll out.

### 4. [`date-part-return-type.md`](date-part-return-type.md) — closes [#191](active-tickets.md#t191) — state: done — PR #161

`expr.cpp`'s `date_part` arm casts cuDF's INT16 to the `return_type` the wire already names. Three
plan-executor cases, one harness case written first as the pin, `tpch/q7`, `q8`, `q9`.

### 5. [`device-schema-harness.md`](device-schema-harness.md) — state: completeness approved — PR #162

Testing only. `test_support::device_schema` reads a handle's schema through `peacock_handle_schema`,
projects an arrow schema onto `{type_id, scale}` and compares; a new schema case per node kind in
every harness family, and eleven hand-written spot-checks inside the recipe walk. Existing cases
untouched; a red case is a `bug_` with a ticket.

### 6. [`driver-output-hook.md`](driver-output-hook.md) — state: approved to build

The driver takes an optional hook called on every emitted batch — the one production change. Under
the `test-support` feature: a validator holding each device batch to its node's schema, four
mock-driver unit tests, and a `schema_validation_enabled|disabled` argument on `corpus_query!`, on for
every enabled cell.
