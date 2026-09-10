# 6 — the crate's API becomes the CLI's, and the walls go up

Kind: production

Last of six, after [`test-support.md`](test-support.md). That task put the corpus harness behind
the feature, so nothing outside the crate needs an engine type any more. This one takes the
surface down to what the CLI calls, deletes the registers task 2 had to build, and writes the
rules that keep it there. It is also the sweep: three tasks deferred work to "later" without
naming a task, and this is that task.

The two halves are the same thing from two ends. The **surface**: 174 bare `pub` items become
eight. The **walls**: `pub mod` survives only in `lib.rs`, and `coding-style.md`'s Visibility
section stops carrying an exemption at all.

## Where the surface stands entering this task

Measured after task 2, to be re-measured after task 3:

| | count |
|---|--:|
| bare `pub` items in `src/` | 249 |
| of those, in a component or subcomponent `mod.rs` or `lib.rs` | 174 |
| behind the nine exempt `pub mod` paths, which task 4 demotes | 75 (60 excluding the two subcomponent facades) |
| `pub mod` declarations | 15 — six components in `lib.rs`, nine exemptions |

The two rows are disjoint and sum to 249. Task 4 takes the 75, so this task starts from **174** and
ends at **eight** bare `pub` items in three files, six `pub mod`, and no register — 166 demotions,
every one checked by the compiler. Every other item becomes `pub(crate)`, which is all a sibling
component ever needed: components live in one crate, so a component API is `pub(crate)` and only
the CLI's entry points are `pub`. That is the whole of "the crate's API becomes the CLI's".

## What tasks 1-3 leave here

Each of these is stated somewhere as deferred, parked or open, and none has a task. Closing them
is this task's work, not an appendix to it.

**Formatting and wording residues.** The `parquet_meta.rs` rustfmt hunk task 1 deferred to task 2
was never applied and still reports. Three files were left unformatted in task 2's second review
round — `test_cpu_end_to_end.rs` (which task 4 moves to `src/tests/`), `cpu_backend/expr_physical.rs`, and `corpus_gpu.rs` (which task 5 moves to `src/test_support/`).
About twenty comments still use "mode" as a common noun for the thing task 1 retired.

**Guards that under-report.** `no_public_signature_names_a_type_from_a_private_module` matches only
`alias::`/`module::` prefixes, so a bare type imported out of a private module and named in a `pub`
signature passes; that becomes load-bearing here, where the private set grows by 166 items.
`names_the_module`'s reverse half misses `use peacockdb_core::executor::cpu_backend;` because there
is no `::` after the path. The super-climb reader reports one `super::super::x` at depth 0 twice.

**Two tickets to file rather than fix**, because each is a different subject: the murmur gate
re-derives `pmod` and the seed-42 pre-fill locally instead of calling `rows_per_lane`, so one rule
has two copies; and the repo is not rustfmt-clean, has no `rustfmt.toml`, and `pipeline.yml` runs
neither a fmt nor a clippy step. Only the first earns a ticket: `prompts.md` files tickets for
production behaviour and names cosmetics as the case never filed, so the formatting gap is recorded
in `build-test.md`, where CI shape lives, and not in `tickets.md`.

**One contradiction to correct in writing.** `module-layout.md` and
`peacockdb-core/tests/common/memory_limit.rs` both say `test-layout.md` creates `src/test_support/`.
It does not — it hands the feature and the module here. Fix both call sites in the commit that
moves the file, or the next reader trusts the comment over the spec.

**Baseline tooling outlives its task.** `module-layout-baselines/` is described as scaffolding
deleted when that task is archived, but `visibility-dump.py`, `case-inventory.sh` and
`compare-inventory.sh` are checks in tasks 3 and 4. They already live in `scripts/`, moved there by task 2's
completeness commit; `doc-attr-check.py`, `narrow.py` and `external-names.py` are task
2's own and die with it. This task inherits them there and adds nothing.

## The demotions

Task 3 raises the nine subcomponent walls as it moves the tests that forced them, and demotes the
75 items behind them. What is left here is the other 174: everything a component `mod.rs` declares
for its siblings, which needs `pub(crate)` and has been spelled `pub` because components are
`pub mod`.

**`#![warn(unreachable_pub)]` goes on in the first slice, but it is a backstop, not the work
list.** The lint fires on a `pub` item that is not reachable from outside the crate — and every one
of the 174 sits in a component `lib.rs` declares `pub mod`, so it reports **zero today** and would
report zero after a task that demoted nothing. It cannot measure this work.

`scripts/visibility-dump.py` is the work list: `awk '$2=="pub" && $3!="mod"' | wc -l`, 174 falling
to eight, one slice at a time. What the lint buys is the future — once a component's items are
`pub(crate)`, a `pub` written inside a private module is unreachable and the lint says so, which is
the rule enforcing itself after this task rather than during it. It stays at `warn`: the crate's
warning count is already a checked baseline, so a new one fails that check without a second
mechanism.

Each component's `mod.rs` then keeps only what the CLI needs as bare `pub` and demotes the rest:
`plan/mod.rs` (92 → 0), `executor/mod.rs` (48 → 2), `wire/mod.rs` (20 → 0), `planner/mod.rs`
(7 → 4), `plan_text/mod.rs` (3 → 0), `planner/translator/mod.rs` (2 → 0), `lib.rs` (2 → 2). A
component staying `pub mod` while every item in it is `pub(crate)` is the intended shape: the
module is nameable, its contents are not.

The eight the corpus harness forced go the same way once the harness is behind the feature — they
are ordinary component items with an unusual reason for having been `pub`, and after the move
their reason is gone.

### The hoist is not here, and this is why

Task 2 could not put the backend types behind a `mod` wall, because `test_cpu_executors` and
`test_gpu_executors` were separate crates and a separate crate cannot reach a private subcomponent.
It listed three answers — open the walls with `pub mod` (taken), declare the 14 types in
`executor/mod.rs`, or the full hoist of 14 types and 55 inherent methods — and deferred the third
as "the one the rules ask for", to be done "later at leisure".

All three answer one question: how does a **separate crate** reach those types. Task 3 answers it a
fourth way by ending the separation, so the question is gone rather than deferred.

Two things confirm it rather than assume it. Measured on the post-task-2 tree, exactly one reach
into the backend child modules comes from outside their own directory — `wire/tests.rs:831`'s
`CpuJoin`, which task 4 replaces with a two-hop `has_finish_pass` delegation rather than a hoist. `executor/driver`, the consumer that would justify
the other thirteen, names none of them: it goes through the `Backend` trait. And the hoist's
destination works against this task — types declared in `executor/mod.rs` are there to be `pub`,
while this task takes that file to two bare `pub` items, so hoisted types would land as
`pub(crate)` and be no more reachable than they were in `accumulate.rs`.

No ticket either. `coding-style.md` files tickets for production behaviour and never for
cosmetics, and a rearrangement with no consumer is the cosmetic case exactly. This paragraph is the
record, so the next reader meeting `pub(crate)` items in `accumulate.rs` does not re-derive it.

## The facades, after

Four kinds of boundary, and after this task each is exactly one thing.

- **The crate** exposes eight items in three files. That is the CLI's API and the whole of it.
- **A component** — `plan`, `planner`, `executor`, `wire`, `plan_text` — is a directory whose
  `mod.rs` declares its whole API, `pub(crate)`, reachable by sibling components and by nothing
  outside the crate. `common.rs` is a file rather than a directory: what the components share,
  declared in one place, and already at zero bare `pub`.
- **A subcomponent** — `executor/cpu_backend`, `executor/driver`, `planner/translator` and the rest
  — is declared `mod`, so it is reachable only from inside its parent component, and its API is its
  own `mod.rs`.
- **`test_support`** is a component-shaped child of the crate root whose API is bare `pub` behind a
  feature, with signatures free of engine types.

No register, no exemption, no `CROSS_COMPONENT_REACHES`. A `pub mod` below `lib.rs` is a violation
with no sanctioned form, which is what makes the rule readable at last.

## coding-style.md's Visibility section is rewritten

Task 2 wrote the rules and, honestly, the exemption beside them: nine sanctioned `pub mod`, 60
items behind them, a register checked both ways, and a paragraph explaining why the register is a
register rather than a habit. All of that was true of a tree with test crates reaching in. None of
it is true after task 3.

What goes: the exemption section entire, the `CROSS_COMPONENT_REACHES` paragraph, and the
"nine more `pub mod` exist, every one forced by a test crate" clause.

What stays, unchanged: the component and subcomponent rules, `mod` not `pub mod`, no `pub use`,
`mod.rs` bodies of one expression, three-deep nesting where the innermost earns it, absolute
`crate::` paths across a boundary, the length exemptions for `mod.rs` and `common.rs`.

What arrives:

- **The crate's API is the CLI's.** Bare `pub` in `src/` means "the binary calls this". Everything a
  component exposes to its siblings is `pub(crate)`. A new bare `pub` is a claim that the CLI needs
  it, and the layout test asks for the receipt.
- **`#![warn(unreachable_pub)]` is what keeps that true afterwards**, once the components' items
  are `pub(crate)` and a `pub` written inside a private module is genuinely unreachable. It cannot
  measure the task itself — while a component is `pub mod`, every item in it is reachable and the
  lint is silent. That is also why the rule is not merely a convention afterwards: Inside a private module `pub` and `pub(crate)` are identical to rustc —
  the module's own privacy is the wall — so the distinction is for the reader, for the blast radius
  when a module is ever opened, and for `private_interfaces`, which passes silently over a `pub`
  type that nothing can name and fires on the `pub(crate)` one. The lint is what makes the first of
  those three self-enforcing.
- **`pub mod` appears in `lib.rs` and nowhere else.** Six components; no exemption, no register.
- **A test-support signature is free of engine types.** The rule that keeps a facade from being a
  rename.
- **What the compiler enforces and what the layout test has to.** Task 2's honest three-way split
  survives the rewrite — module privacy is rustc's, sibling reach and where a `pub` appears at all
  are the layout test's, and a `pub` type that is unreachable but nominally public defeats
  `private_interfaces`, so that is the layout test's too.

## The surface, after

Bare `pub` appears **eight times in `src/` outside the feature gate, in three files**.

| Item | Declared in |
|---|---|
| `build_session_state`, `register_tables_for` | `lib.rs` |
| `plan`, `PlanKnobs`, `BatchSizing`, `SMALL_TABLE_BYTES` | `planner/mod.rs` |
| `run`, `CpuBackend` | `executor/mod.rs` |

**The eight are not closed under their own signatures, and the table has to say so.** `plan`
returns `Box<dyn GpuNode>` and `MemoryModel`; `run` takes `&dyn GpuNode` and a `B: Backend` and
returns `RunReport` and `RunError`. A `pub` item whose signature names a `pub(crate)` type is a
`private_interfaces` warning on the very item this table keeps — against a warning baseline this
task checks. So the surface is these eight **plus the types they name**, and the first slice
enumerates that closure from the signatures rather than guessing it: walk the eight, collect every
type in their parameters and returns, and keep those `pub` too. If the closure comes out large,
that is the honest size of the CLI's API and the table grows; what must not happen is eight `pub`
items sitting on types nothing outside can name.

`test_support/mod.rs` is a further file carrying bare `pub` and the only one behind a feature. The
guard distinguishes them: eight unconditional items checked by file and name, a feature-gated set
checked by signature.

## Validation

No test case moves in this task and no golden is touched, so both are pinned as invariants rather
than checked as outcomes. What moves is visibility, and a visibility regression compiles.

### Baselines

1. The `pub`/`pub(crate)` item dump with declaring files, from the end of task 3, taken with
   `scripts/visibility-dump.py`.
2. `--list` for all three lib shapes and the remaining binaries, and their leaf-name sets.
3. `sha256sum` over `testdata/goldens/`.
4. The three registers in `test_module_layout.rs` — `PUB_MODULES`, `CROSS_COMPONENT_REACHES`,
   `PUB_OUTSIDE_A_MOD_RS` — with their entry counts.

### The checks

- **The surface lands on exactly eight**, asserted as the table above — file and item, not a count.
  A count passes when one item is dropped and another added. Every other item must appear in the
  dump as `pub(crate)`, not merely as absent: an item deleted and an item demoted look the same to
  a count and different to this.
- **`pub mod` appears six times unconditionally, all in `lib.rs`**, plus `test_support` behind its
  feature — the same unconditional-versus-gated split the surface table makes. `PUB_MODULES` is
  deleted, not emptied: an empty register is an invitation.
- **`unreachable_pub` reports zero, and is known to be armed.** Run the three build shapes and
  confirm zero — which is weak evidence on its own, since it also reported zero before the task. So
  construct the violation: spell one item in a now-private implementation module `pub`, watch it
  warn, revert. That is the check; the count is not.
- **`CROSS_COMPONENT_REACHES` is deleted**, and the reach it named is already gone — task 4
  replaced it with the `has_finish_pass` delegation when it raised the `cpu_backend` wall.
- **The private-type-in-a-public-signature guard is fixed first, then relied on.** It matches only
  path-prefixed types today; with 166 newly private items it is the guard most likely to be needed
  and most likely to miss. Fix it, prove it red on a bare imported type, then run it.
- **Case counts and leaf-name sets identical to task 3's**, on all three shapes. Nothing moves
  tiers here; a count that shifts means a test followed the harness by accident.
- **Goldens byte-identical.** Nothing in this task can reach them; a diff means the harness changed
  behaviour while moving 698 lines of test code.
- **The residues are gone**: `rustfmt --check` is clean on the four files named above, and no
  comment uses "mode" as a common noun for what task 1 retired.

### Slices

Six components, one slice each, every one compiler-checked: turn the lint on first so its warning
count is the work list, then `plan`, `planner`, `executor`, `wire`, `plan_text`, `common`. After
each, the lint's count and the dump's bare-`pub` count both fall; a slice that moves neither moved
the wrong thing. Each slice ends by appending its state to `test-support-detail.md` and handing
back.

The corpus move is not here at all — [`test-support.md`](test-support.md) did it. That is the cut:
698 lines of test code changing compilation unit is a diff a reviewer must read, and ~300 one-word
demotions is a diff a reviewer can only skim, so they are judged separately or the first hides
inside the second.

## Done when

`peacockdb-core` exposes exactly the eight items in the table, every other former `pub` demoted to
`pub(crate)` rather than deleted; `pub mod` appears six times unconditionally and only in `lib.rs`,
with `test_support` gated beside them; `#![warn(unreachable_pub)]` is on and reports zero; both
registers are deleted and the reaches they sanctioned are gone; case counts, leaf-name sets and
goldens are unchanged; `coding-style.md`'s Visibility section carries rules and no register; the
carry-over list above is closed item by item, with the two tickets filed rather than fixed; and CI
is green.
