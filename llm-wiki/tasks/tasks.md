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

### 1. [`drop-mode-name.md`](drop-mode-name.md) — state: done — PR #141

Names only. "Batch partitioned" and its `bp` abbreviation qualify against an alternative that no
longer exists, so every occurrence goes — identifiers, mode labels, golden filenames, one Python
module, one ticket page. No Rust file moves and no behaviour changes, which makes the bar absolute:
every derived artifact reproduces byte for byte. `src/batch_partitioned/` is the one name left
standing, for task 2 to move once rather than twice.

### 2. [`module-layout.md`](module-layout.md) — state: done — PR #143

`peacockdb-core/src/batch_partitioned/**` moves up to `src/`, laid out as components whose whole API
is declared in `mod.rs` with the implementation behind private modules. `plan_batch_partitioned`
becomes `planner::plan` and `batch_partitioned_driver` becomes `executor::run` as their modules
acquire those names; `peacockdb/src/main.rs` moves with them. The 170 public items stay public here
— what changes is where they are declared and what may reach past them.

### 3. [`rmm-pool-budget.md`](rmm-pool-budget.md) — closes [#178](../tickets.md#t178) — state: done — PR #144

Six gtest binaries reserve 85% of free VRAM from `main()`, and two of them are not sf40 tests at
all, so an ordinary CI run puts four such processes on a shared card. Each declares an explicit
byte budget measured from the pool's own statistics adaptor instead. Touches `cpp/` and one wiki
page, so it is independent of the three layout tasks around it.

### 4. [`test-layout.md`](test-layout.md) — state: building

Items in `peacockdb-core/src` are public partly because `tests/*.rs` are separate crates that see
the library the way crates.io would. Moving the eleven targets that force them, plus the murmur
gate, down into `src/` ends that reason for all but eight. The move, the visibility
sweep and the separation of test code from production code happen together, because none is worth
its own pass over the same files.

### 5. [`test-support.md`](test-support.md) — state: approved to build

The corpus harness — 698 lines — joins the helpers task 4 put in `src/test_support/`, and the two
corpus binaries reach it through three functions whose signatures carry no engine type. That is
what stops the last eight items needing `pub`. Small and semantic on purpose: it is a diff a
reviewer reads line by line, so it does not share a branch with task 6's three hundred one-word
demotions.

### 6. [`visibility.md`](visibility.md) — state: approved to build

174 bare `pub` items become eight, `unreachable_pub` goes on to keep them there, both exemption
registers are deleted and `coding-style.md`'s Visibility section stops carrying an exemption at
all. It is also the sweep: the formatting hunks, comment wording and guard fixes that tasks 1-3
deferred without naming a task.

### 7. [`sink-divergence-survey.md`](sink-divergence-survey.md) — state: approved to build

**Prototype — no PR, branch never merged; the product is
[`reports/sink-divergence.md`](../reports/sink-divergence.md).** Sixty-odd disabled cells already fail
at the sink with a message naming neither the column nor either type, so a rollout over sixty queries
produces sixty identical lines. One error site changes to name column, declared and exported type;
the corpus rollout that was happening anyway then reports which divergence classes actually reach the
boundary and how often. Fixes nothing and enables no cell. It is what tells `declared-schemas` which
classes are worth a harness.

### 8. [`operator-harness.md`](operator-harness.md) — state: approved to build

One operator, one script of batches the test wrote, both backends, the outputs compared exactly.
A test-only upload symbol puts an Arrow batch on the device; `Given` leaves plus one arm in
`attach.rs` give a hand-built node its recipe with no plan; a driver per category, generic over
`Backend`, runs the same calls on each side. Proven here on the seqless three — unload, limit and
the round trip — and a guard that names every kind without a case.

### 9. [`operator-cases.md`](operator-cases.md) — state: approved to build

Cases only, no mechanism: every seq-bearing operator through the harness — filter, project, the
sorts, the accumulators, both aggregates, the scatter, nine hash-join types, cross, nested-loop
and the scan. Each row is green or a `bug_` test with a ticket, and only a device run says which;
#198 and #175 are expected to land as `bug_` tests that `typed-nulls` and `empty-build` then delete.

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
