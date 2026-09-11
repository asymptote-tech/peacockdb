# peacockdb coding style

- **Create useful abstractions.** New code should introduce (or reuse) abstractions that
  make later reuse easy — a shared driver, a trait, a helper — rather than copies of
  similar logic.
- **Small files:** under 1000 lines. Split by responsibility (`plan/`, one file per node
  family, and `executor/cpu_backend/` are the pattern). A component's `mod.rs` is exempt: it
  is a facade of declarations, and the limit it wants is set once the layout has settled.
- **Interfaces/traits in separate files** from their implementations
  (`executor/mod.rs` declares the seven category traits and `Backend`; the batch types are
  implemented in `executor/{cpu_batch,gpu_batch}.rs`).
- **Short functions:** under 150 lines in most cases.
- **Comments say *why*, briefly.** Only non-obvious constraints, invariants, and gotchas —
  never what the next line does, never process history. If a comment documents an
  important historical decision, a distilled version may go to
  `llm-wiki/archive/historical-comments.md` instead of living in the code.
- **Comment length is capped.** Four lines for a comment inside a function body, ten for
  one above a declaration or at the top of a file. A comment past its cap has stopped
  annotating and started explaining, and an explanation nobody can find unless they are
  already reading this function is one nobody reads — move it to `llm-wiki/` and leave
  the line that points there.
- **Match surrounding idiom** (naming, comment density, error handling). Trust rustfmt;
  don't hand-format — and run it over the files you touched, never the crate, which
  predates the installed rustfmt and reformats 49 of them. A `mod.rs` is not one file for
  this purpose: rustfmt follows `mod` declarations, so formatting one reformats every file
  below it. Name the leaves instead.
- **C++ formatting** is defined by `.clang-format` at the repo root. Apply it to the
  lines you changed — `git clang-format` — never to whole files: the tree was never
  machine-formatted, so reformatting one file to fix one function buries a three-line
  change in three hundred.
- **Python:** plain module filenames — no leading underscores.
- **Bash: the flag set is an interface, and failure is fatal.** No flag that another
  flag already implies, and none whose only effect is the default. Contradictory
  combinations are rejected with a message naming the contradiction — never resolved by
  argument order, which makes the same two flags mean different things depending on how
  they were typed. Validate every argument, and that the run will actually do something,
  *before* the first side effect: a typo should fail before it ships files, not halfway
  through. Prefer `set -euo pipefail`, and remember what it does not cover: an `exit`
  inside `$(…)` ends the subshell only; an empty list makes a `for` body vanish
  silently, so a derived-but-empty work set must be an explicit error rather than a
  green no-op; and a failing `&&` list is ignored at statement level but becomes the
  return value when it is a function's *last* command, so `[ -f x ] && do_thing` written
  at the end of a function silently fails the caller. Where execution genuinely must continue past a failure (running every
  test binary before reporting), say why at the site and accumulate the status so the
  script still exits non-zero.
- **No defensive code for impossible scenarios**; trust internal invariants and framework
  guarantees. No fallbacks or feature flags the task didn't ask for.
- **Test code is never in a production file.** A module's unit tests are a child module in a
  file of their own — `validate.rs` beside `validate/tests.rs` — never an inline
  `#[cfg(test)] mod tests { … }`. Every test-only path in `src/` carries `test` in its name
  (`src/tests/`, `plan/tests/`, `driver/tests/mock.rs`), so a reader tells test code from
  production code by the path alone. A `#[cfg(test)]` sits on a test-module declaration and
  nowhere else, bar the register below. `test_module_layout` checks all three.
- **A test module declares the lowest build rung it needs, and its name says which.** The three
  shapes nest — `rust-only` ⊂ default ⊂ `gpu` — and a module says what it needs, never what it
  excludes: `#[cfg(test)] mod tests;` for pure Rust, `#[cfg(all(test, not(feature = "rust-only")))]
  mod ffi_tests;` for the FFI linked, `#[cfg(all(test, feature = "gpu"))] mod gpu_tests;` for a
  device. Above the floor the name and the gate imply each other, in both directions, and the
  layout test holds them to it: that is what lets one CI line select one rung by path filter,
  `-- ffi_tests::` or `-- gpu_tests::`, without re-running the rungs beneath it. The two ends of
  the ladder are exclusive by `compile_error!`.
- **`#[cfg(test)]` marks test code, and test code is what exists to serve tests** — whether or
  not it contains an assertion. A one-line wrapper that hands a test an object it could not
  otherwise reach is test code; being compiled only in a test build is exactly right for it,
  since a test build is the only build that can matter to it.
- **A test-only item may keep its `#[cfg(test)]` outside a test path in two cases only.** A
  cross-component entry point cannot live in one: it must name what its own module owns while its
  caller is in another component, so the two can never sit together, and it is declared in that
  module's `mod.rs` like anything else crossing a boundary. A reader of a private field cannot
  either: only the module declaring the field and its children can see it. Both are registered in
  `test_module_layout/test_code.rs`'s `TEST_ONLY_ITEMS`, which checks the item, its callers and the doc
  comment naming them — a stale comment is how the set grows without anyone deciding to grow it.
- **What `#[cfg(test)]` is not is a way to quiet `dead_code` on production code.** An item behind
  it is absent from a release build, so it is never type-checked against a change made for
  shipping code. A production item that nothing ships yet stays `pub(crate)` and keeps its
  warning, which is the honest signal — answer that with a caller, with deletion, or with
  `#[allow(dead_code)]` and the reason at the site.
- **A bug the review finds ships with a regression test**, red before the fix — a defect proved
  only by the reader who found it is one the next refactor is free to restore.
- **No scope-creep refactors**: a bug fix doesn't need surrounding cleanup.

## Names

Kernighan's rules, written down after the fact rather than followed from the start.

- **Length is proportional to scope** (K&R §2.1). A loop index is `i`; a name crossing a
  module, a trait or the FFI earns words. Both halves bite: a paragraph-long name in a
  three-line body is noise, and a terse one in an exported signature is a puzzle.
- **A name says what a thing is, never how it came to be.** No `new` in the sense of the
  newer one — `ExecutorNew`, `process_v2` — no `additive`, no ticket number, no task id.
  Rust's `Type::new` is a constructor, not a version marker. The manner of a change is the
  shortest-lived fact about it, git holds it already, and the reader who greps a year later
  is looking for behaviour. The case: T9's gtest suite shipped as `AdditiveAbi` — a suite
  about per-call scan reads and row ranges, named after the fact that it was added without
  breaking anything.
- **Functions get active names, and an inaccurate name is worse than a vague one**
  (Kernighan and Pike, *The Practice of Programming*). A `check_` that also repairs has
  misled every reader who trusted it.
- **A function returning `bool` is named as a claim, not an action.** It answers a question, so
  the call site should read as something that is true or false: `is_`, `has_`, `can_` and
  `needs_` are the usual forms, and `probe_reads_build`, `rows_are_certain` and `satisfied_by`
  are equally good without a prefix. `makes_a_finish_pass` is not — "makes" promises the call
  does something, and a caller who believes it looks for the effect. It becomes
  `has_finish_pass`. A function that genuinely acts *and* reports is not a predicate and keeps
  its active name; the bool is its result, not its subject.
- **The same thing carries the same name everywhere, and one name means one thing** —
  across the FFI most of all, where two names for one value is how the two sides drift
  without either being wrong. The inverse costs as much: `ScanBatch` in the flat buffers
  means partitions while `CudfCoalesceBatches.target_batch_size` in the same buffers means
  Arrow batches, which architecture.md has to carry a naming trap for.

## Length limits

- **A ticket is at most 15 lines**: one header, at most two stating the problem, the rest
  describing it. What runs longer is a design document wearing a ticket's number — put it
  in `llm-wiki/tasks/` and let the ticket point at it. The cap is also what keeps the list
  usable: a reader triaging 75 tickets reads headers and first lines, so a ticket that
  buries its problem statement on line 20 is not being read at all. One exception: a ticket
  carrying a deferred fix, at the detail that stops the next reader re-deriving it, may run to
  thirty. A fix worked out and then thrown away costs more than the lines do.
- **A ticket is about code, never about documentation.** A stale sentence, a dead link, a
  count that no longer adds up, a widget rendering any of them — fix it in the commit that
  found it. Documentation is anything whose product is prose for a reader: `llm-wiki/`, code
  comments, and the rendered cost report. Filing costs a number, a triage pass and a reader's
  attention, and the page stays wrong for as long as it sits in the list. A ticket asking that
  a page say more is the same shape — pages grow when a human asks, not when a ticket does.
- **Architecture pages describe the current state, not the route to it.** No "was X, now
  Y", no account of what an earlier attempt did or why it was abandoned. `architecture.md`
  and `build-test.md` answer what is true today; git holds the sequence, and a decision
  worth carrying forward goes to `llm-wiki/archive/`. A page that narrates its own history
  makes the reader work out which sentence is still in force.
- **A commit message, a PR description or a PR comment is at most 10 lines**, subject line
  included. All three are read in a narrow column next to the thing they describe, and the
  diff is the detail — a message that restates the diff is read by nobody, and one that
  argues a design is in the wrong place. Point at the ticket or the task spec instead, which
  is where a later reader will look anyway.

## Visibility

- A component or subcomponent is a directory with `mod.rs`, and its whole API is declared
  there. Implementation modules are declared `mod x;` with `pub(crate)` items: the module's own
  privacy is the boundary, so a path through it is refused whatever the item says and
  `pub(super)` is never needed.
- **A subcomponent is declared `mod`, not `pub mod`** — `mod scan_mapping;`, never
  `pub mod scan_mapping;`, which would make `planner::translator::scan_mapping::Mapping`
  nameable crate-wide and leave the wall on paper. What a sibling component needs is declared in
  the component's own `mod.rs`.
- `lib.rs` declares the components `pub mod`, and those seven — `common`, `executor`, `plan`,
  `plan_text`, `planner`, `wire`, `test_support` — are the only `pub mod` in the crate, counted by
  `scripts/visibility-dump.py`. `PUB_MODULES` and `CROSS_COMPONENT_REACHES` in
  `test_module_layout/{visibility,walls}.rs` are the registers for a `pub mod` or a cross-component reach a test crate
  forces; both are empty, and a `pub mod` outside the register is a violation, not a precedent.
- **Eight items are `pub` because a test crate forces them**: `GpuNode` and `validate`
  (`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`, `GpuBackend`
  and `GpuContext` (`executor/mod.rs`), `render_run` (`plan_text/mod.rs`) — all named by
  `tests/common/corpus.rs` and `corpus_gpu.rs`. [`test-support.md`](tasks/test-support.md) unforces
  them and [`visibility.md`](tasks/visibility.md) removes them with the rest of the raw bare-`pub`
  count, which is 200 outside `test_support`.
- **Nesting may go three deep** where the innermost earns it — `planner/translator/scan_mapping/`
  is 720 lines behind three entry points — under the same `mod` rule at each level. A directory
  with a one-item facade and a hundred lines behind it is an implementation module wearing one.
- `pub use` is not allowed: inline the declaration into `mod.rs`, or into `common.rs` for what
  the implementation modules share. A child reaches into its parent; a parent never re-exports a
  child. A body in `mod.rs` is one expression, and a struct keeps its inherent `impl` there.
- An implementation module may implement any trait for a type its component declares and define
  free functions the `mod.rs` delegates to. It may not declare the component's API.
- Absolute `crate::` paths across a component boundary, `super::` only within one. `mod.rs` and
  `common.rs` have no length limit; every other file keeps the 1000-line one.
- **What the compiler enforces**: a component is reachable only through its `mod.rs`, and a
  subcomponent only from inside its parent, both by module privacy. **What
  `test_module_layout` must**: sibling reach between implementation modules, where a `pub`
  appears at all, and a type from a private module in a public signature — `private_interfaces`
  reads nominal visibility, so an unreachable type spelled `pub` passes it silently.

## Antipatterns

Most of these shipped here and cost something, and are recorded with the case that
revealed them, because the general rule is easy to nod along to and hard to recognize in
your own diff. An entry with no case attached is stated generically on purpose; add the
case when one turns up.

### Building around a bug

You find a bug while doing something else, and fixing it is not in this task. You may not
design around it. A branch, a filter, a flag or an enum variant that exists only to avoid the
broken path bakes the bug into the shape of the code, and the shape outlives the fix: the fix
lands, the scaffolding stays, and no later reader can tell it from a requirement.

Three steps instead.

- **Attribute it to a ticket** — an existing one if it is the same bug, a new one otherwise.
- **Write a test named `bug_<what it does wrong>`**, asserting the wrong behaviour, with the
  ticket number in a comment above it. It passes today. It is the record that the behaviour is
  known and unfixed, and it goes red the moment someone fixes the bug — which is the signal to
  delete it, in that same change. The Names rule against ticket numbers still holds: the prefix
  says what the test is, and the number lives in the comment.
- **Leave the production code alone.** No special case whose only reason is the bug.

Every other test in the tree asserts what is right. A `bug_` test is the one place that asserts
what is wrong, so the prefix is what stops a reader taking a scar for a requirement — and it
makes "which known-wrong behaviours does the engine still have" a grep instead of a memory.

The case, on the casts branch: `exports.rs` predicted three ways the device's exported type can
differ from the declared one and carried the reason as an enum — one inherent to cuDF, one a bug
with a fix due in the next task (#187), one assumed unreachable. The cast at the unload then
filtered on the reason name, so the bug had a variant of its own and that variant steered the
dispatch. The third case fell out of the whitelist with nothing failing. Under this rule the bug
is a `bug_` test and not a variant, and the cast is decided by whether the divergence is
inherent.

### Encapsulation violations

Reaching past an interface into what it was meant to hide — reading or writing private
state, depending on a representation its owner is free to change, re-implementing a rule
that lives inside the boundary, or letting a caller assemble something only the owner
should assemble.

It compiles and it passes, which is why it survives review. The cost comes later: the
owner can no longer reason about its own invariants, because correctness now depends on
code it cannot see, and a change that is local by every reasonable reading breaks
something far away. The rule gets duplicated rather than moved, so the two copies drift
and the one that is wrong is whichever the reader did not open.

Fix the interface rather than the caller: add the operation the caller actually needs, and
keep each invariant on the side of the boundary that owns it.

### A large behavior change triggered implicitly by the arguments

A function that switches to fundamentally different behavior based on some combination of
its inputs — a magic value, a string it parses, a pair of flags read together — hides the
most important thing it does. The caller reads one call and cannot tell which behavior it
gets. Worse, the switch has no natural place to fail: feed it an input nobody anticipated
and it picks a branch silently, with no diff to the switching code and nothing going red.

Make the behavior an explicit parameter with a name — an enum, not a bool and not a string
— so the call site states which one it wants. Where an input genuinely must be decoded
back into behavior (a frozen file format, an external contract, anywhere there is no call
site to state it at), keep the mapping **exhaustive**: an unlisted value panics naming its
fix rather than falling through to a default nobody chose.

The case that revealed it, in the test harness: `partition_mode("tp8-standard")` returned
`RealMultiPartition` and every other device label fell through to `SinglePartition`, so
which executor a test ran was a side effect of how its golden file happened to be named.
Adding a device — a memory-constrained genuine-8-way tier (#91) — would have routed it to
the wrong executor with nothing failing. The mode is now a parameter stated at the call
site. One lookup survives, `mode_named` in `src/test_support/mod.rs`, which resolves the
mode ident a `corpus_query!` line writes: it is exhaustive, so an unknown one panics naming
the five rather than planning some default nobody chose.

The same shape appears as a bool that means two unrelated things, a trailing `Option`
whose `None` selects a different algorithm, and an argument order that is not type-checked
because both parameters are the same type.

### A doc comment reassigned by an insertion

A doc block belongs to the declaration below it, so one inserted above takes it and leaves the
original none — nothing reads as missing, which is why review passes it. A split leaves the block
behind a blank line and a guard decides that; an insertion is contiguous and shows only in a diff,
so read each block's first sentence. `tickets.md`'s `<a id>` anchors the same way: #177 took #170's.

### A thread-local as an output or side-channel argument

Found during the executors refactor: `execute_one`/`execute_node` passed node inputs
through an anonymous-namespace `thread_local`, which is per-translation-unit — splitting
the file would have silently forked the variable and re-executed whole subtrees (correct
answers, exponential cost, invisible to correctness tests). Pass inputs and outputs
explicitly through parameters; never smuggle them through thread-locals or globals.

### A reader that stops at the first match

Code that searches text for a marker — a section header, a delimiter, an attribute in a golden
file — and takes the first hit, in a file where a second hit is possible. It is correct for
every input anyone has written so far, which is why it passes review and keeps passing.

Four instances turned up in one branch: a reader that took the first section of a report, one
that took the first segment of a diff line, one that took the first invocation in a script, and
a golden header that was cut short at the first occurrence of an attribute. Each was green
until a second thing appeared beside the first.

Read every match, or write the one line that says why the first is the right one. "There is
only ever one today" is a sentence that stops being true with nobody touching the reader.
