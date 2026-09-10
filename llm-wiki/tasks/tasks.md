# Task board

One `##` section per chain, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain ENS-casts (base: master)

### 1. [`casts.md`](casts.md) — closes [#183](active-tickets.md#t183) — state: blocked(done)

Rust only, no FFI, no C++. Predict the export type at plan time in `attach_recipes`, render it as
`exports=`, and cast the one divergence that is inherent — cuDF has a single string type. The
decimal is predicted here and deliberately not cast. Moves ten `.plans.txt`; 59 queries carry it.

### 2. [`wire-schema.md`](wire-schema.md) — closes [#187](active-tickets.md#t187) — state: blocked(done)

Crosses the FFI but small: `Writer::push` is the one funnel that must fill `PlanNode.output_schema`,
and the C++ carries a per-column precision into `column_metadata`, which cuDF currently defaults to
38 because nobody passes it. Removes the divergence rather than casting it. Payload golden bytes
move, so `schema_text` must render precision in the same change or the diff shows nothing.

### 3. [`empty-answers.md`](empty-answers.md) — closes [#173](../tickets.md#t173), [#175](../tickets.md#t175) — state: blocked(done)

Needs task 2 first, and that is what makes it small: with `output_schema` on the wire, `execute_node`
answers with an empty table instead of throwing, and no ABI symbol is needed. `RightAnti` routes its
probe side rather than calling; `Right` and `Full` want the mirror of a pad that already exists. The
global aggregate keeps its identity row — the natural implementation deletes it.

### 4. [`refcounted-tables.md`](refcounted-tables.md) — closes [#145](../tickets.md#t145), [#152](../tickets.md#t152) — state: new

The largest and the one that frees the most: 39 `.table` sites across 11 files, plus
`peacock_handle_retain`, the first new ABI symbol — needed because `execute_one` takes its inputs by
value, so the registry cannot keep a handle and let an operator own its input unless the owner is
shared. 75 queries carry #152. Memory accounting is deliberately out of scope and will diverge.

## Chain ENS-drop-mode-name (base: master)

The layout refactor. It may not run beside the ENS-casts chain: both rewrite the same tree, so
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

### 3. [`rmm-pool-budget.md`](rmm-pool-budget.md) — closes [#178](../tickets.md#t178) — state: approved to build

Six gtest binaries reserve 85% of free VRAM from `main()`, and two of them are not sf40 tests at
all, so an ordinary CI run puts four such processes on a shared card. Each declares an explicit
byte budget measured from the pool's own statistics adaptor instead. Touches `cpp/` and one wiki
page, so it is independent of the three layout tasks around it.

### 4. [`test-layout.md`](test-layout.md) — state: blocked(approved to build)

Of 170 public items in `peacockdb-core/src`, 108 are public only because `tests/*.rs` are separate
crates that see the library the way crates.io would. Moving the eleven targets that force them, plus
the murmur gate, down into `src/` takes that surface from 108 to eight. The move, the visibility
sweep and the separation of test code from production code happen together, because none is worth
its own pass over the same files.

### 5. [`test-support.md`](test-support.md) — state: blocked(approved to build)

The corpus harness — 698 lines — joins the helpers task 4 put in `src/test_support/`, and the two
corpus binaries reach it through three functions whose signatures carry no engine type. That is
what stops the last eight items needing `pub`. Small and semantic on purpose: it is a diff a
reviewer reads line by line, so it does not share a branch with task 6's three hundred one-word
demotions.

### 6. [`visibility.md`](visibility.md) — state: blocked(approved to build)

174 bare `pub` items become eight, `unreachable_pub` goes on to keep them there, both exemption
registers are deleted and `coding-style.md`'s Visibility section stops carrying an exemption at
all. It is also the sweep: the formatting hunks, comment wording and guard fixes that tasks 1-3
deferred without naming a task.
