# 5 — the crate's API becomes the CLI's, and the walls go up

Kind: production

Last of five, after [`test-layout.md`](test-layout.md). Two things land here and they are the same
thing seen from two ends. The **facade**: `peacockdb-core` exposes exactly what the CLI needs and
nothing else, with the corpus harness moved behind a feature. The **walls**: the registers the exemption
[`module-layout.md`](module-layout.md) had to grant are deleted, `pub mod` survives only in
`lib.rs`, and `coding-style.md`'s Visibility section stops carrying an exemption at all.

Task 3 raises the nine subcomponent walls as the tests that forced them move in-crate; this task
takes the rest of the surface down and writes the rules that keep it there.

It is also the sweep. Three tasks deferred work to "later" without naming a task, and this is that
task; the carry-over list below is part of the spec, not a courtesy.

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
round — `tests/test_cpu_end_to_end.rs`, `cpu_backend/expr_physical.rs`, `tests/common/corpus_gpu.rs`.
About twenty comments still use "mode" as a common noun for the thing task 1 retired.

**Guards that under-report.** `no_public_signature_names_a_type_from_a_private_module` matches only
`alias::`/`module::` prefixes, so a bare type imported out of a private module and named in a `pub`
signature passes; that becomes load-bearing here, where the private set grows by 241 items.
`names_the_module`'s reverse half misses `use peacockdb_core::executor::cpu_backend;` because there
is no `::` after the path. The super-climb reader reports one `super::super::x` at depth 0 twice.

**Two tickets to file rather than fix**, because each is a different subject: the murmur gate
re-derives `pmod` and the seed-42 pre-fill locally instead of calling `rows_per_lane`, so one rule
has two copies; and the repo is not rustfmt-clean, has no `rustfmt.toml`, and `pipeline.yml` runs
neither a fmt nor a clippy step. Both are real, neither is visibility.

**One contradiction to correct in writing.** `module-layout.md` and
`peacockdb-core/tests/common/memory_limit.rs` both say `test-layout.md` creates `src/test_support/`.
It does not — it hands the feature and the module here. Fix both call sites in the commit that
moves the file, or the next reader trusts the comment over the spec.

**Baseline tooling outlives its task.** `module-layout-baselines/` is described as scaffolding
deleted when that task is archived, but `visibility-dump.py`, `case-inventory.sh` and
`compare-inventory.sh` are checks in tasks 3 and 4. Task 3 moves them to `scripts/`, where they
stop being one task's property; `doc-attr-check.py`, `narrow.py` and `external-names.py` are task
2's own and die with it. This task inherits them there and adds nothing.

## The corpus facade

`corpus.rs` (508 lines), `corpus_gpu.rs` (190), `mode.rs` (88) and `memory_limit.rs` (51) — 837
lines — go to `src/test_support/`, behind a feature. Inside the crate they reach `pub(crate)`
items, so the eight stop being `pub`.

The eight are `GpuNode`, `validate`, `RunReport`, `render_run`, `GpuBackend`, `GpuContext`,
`RecipePlan` and `attach_recipes`, and they exist for two integration targets that deliberately
stay external: `test_cpu_corpus` and `test_gpu_corpus`.

Those two stay because they are the genuine end-to-end tier — SQL in, rows out, against committed
goldens, 456 cases — and because keeping them as two binaries keeps `inventory`'s
one-binary-per-engine property resting on two `--test` targets rather than on the `gpu` feature
producing two compilations. The feature route would work and would make the registry guard depend
on something that reads as unrelated to it.

The two binaries name **none** of the eight. They call functions whose signatures are strings and
test-local types: `cpu_case(dataset, sf, query, mode, oracle)`, `authoritative_mode`, `gpu_case`.
That is what makes the facade real rather than a rename. `over_cap` travels with `corpus.rs` and
`test_corpus_goldens` reaches it the same way. `corpus_golden.rs`, `registry.rs`, `golden_text.rs`,
`result_text.rs` and `cost_model.rs` name zero crate items and stay in `tests/common/` untouched.

## The mechanism

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so **no CI step passes a flag** and a plain build cannot see the module. `#[cfg(test)]` cannot do
this job: the library is compiled without `cfg(test)` when cargo builds an integration test, which
is the whole reason these eight exist.

## test_support is shaped like a component

`src/test_support/mod.rs` declares the whole API the integration tests may reach — `cpu_case`,
`gpu_case`, `authoritative_mode`, `over_cap`, `Mode`, `MODES`, `MemoryLimit`, `TIER`, `BUDGET` —
and `mod corpus; mod corpus_gpu; mod mode; mod memory_limit;` are private implementation modules
with `pub(crate)` items. Same rules as a component, for a reason beyond symmetry: the signature
check below is a scan of one file only if the API lives in one file.

Being a child of the crate root it sees component facades and not their internals — the same level
as `src/tests/`, and the reason it reaches `pub(crate)` items without any of them becoming `pub`.

**Every `pub` in `test_support` takes and returns strings, `Mode`, `MemoryLimit` or nothing.** A
signature mentioning `GpuNode` or `RunReport` puts the item straight back on the surface under
another name, and it compiles. This goes in `coding-style.md` beside the visibility rules, and the
layout test enforces it.

## The demotions

Task 3 raises the nine subcomponent walls as it moves the tests that forced them, and demotes the
75 items behind them. What is left here is the other 174: everything a component `mod.rs` declares
for its siblings, which needs `pub(crate)` and has been spelled `pub` because components are
`pub mod`.

**Turn on `#![warn(unreachable_pub)]` in `lib.rs` in the first slice, not the last.** The lint
fires on exactly this — a `pub` item not reachable from outside the crate — and naming
`pub(crate)` as the fix. Switched on first it is the work list: its warning count starts at the
number of items still to demote and reaches zero when the task is done, so every slice has a
number to move and the last one has nothing left to find. It stays at `warn` rather than `deny`:
the crate's warning count is already a checked baseline, so a new one fails the check without a
second mechanism.

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
`CpuJoin`, which task 3 hoists as a single type. `executor/driver`, the consumer that would justify
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
- **`#![warn(unreachable_pub)]` is what keeps that true**, and it is the reason the rule is not
  merely a convention. Inside a private module `pub` and `pub(crate)` are identical to rustc —
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

`test_support/mod.rs` is a fourth file carrying bare `pub` and the only one behind a feature. The
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
- **`unreachable_pub` reports zero.** Run the three build shapes and confirm. Then construct the
  violation — spell one implementation-module item `pub` — and watch it warn, so the lint is known
  to be on rather than assumed.
- **`CROSS_COMPONENT_REACHES` is deleted**, and the reach it named is already gone — task 3 hoisted
  `CpuJoin` into `executor/mod.rs` to raise the `cpu_backend` wall.
- **No engine type in a `test_support` signature.** Scan every `pub` in `test_support/mod.rs` and
  assert its parameter and return types come from the allowed set. Then construct the violation —
  add `pub fn tree() -> Box<dyn GpuNode>` — and watch it go red. This is the guard the facade rests
  on and the one that would otherwise never be exercised.
- **The private-type-in-a-public-signature guard is fixed first, then relied on.** It matches only
  path-prefixed types today; with 241 newly private items it is the guard most likely to be needed
  and most likely to miss. Fix it, prove it red on a bare imported type, then run it.
- **The feature is off in a plain build**, proven by construction: reference `crate::test_support`
  from `lib.rs`, confirm `cargo build` fails with `E0433`, revert. A passing build is not evidence —
  the module simply is not there to break anything.
- **No CI step passes `--features test-support`.** Grep the workflows and assert absence; if one
  does, the self dev-dependency is not doing its job.
- **Case counts and leaf-name sets identical to task 3's**, on all three shapes. Nothing moves
  tiers here; a count that shifts means a test followed the harness by accident.
- **Goldens byte-identical.** Nothing in this task can reach them; a diff means the harness changed
  behaviour while moving 837 lines of test code.
- **`inventory` still sees two binaries.** Both corpus targets keep their own registry assertion and
  both must pass — the property the "keep them external" decision exists to protect.
- **The residues are gone**: `rustfmt --check` is clean on the four files named above, and no
  comment uses "mode" as a common noun for what task 1 retired.

### Slices

Six components, one slice each, every one compiler-checked: turn the lint on first so its warning
count is the work list, then `plan`, `planner`, `executor`, `wire`, `plan_text`, `common`. After
each, the lint's count and the dump's bare-`pub` count both fall; a slice that moves neither moved
the wrong thing. Each slice ends by appending its state to `test-support-detail.md` and handing
back.

The corpus move is one slice of its own and does not interleave with the demotions: it is 837
lines of test code changing compilation unit, and mixing it into a visibility slice makes the diff
unreadable in exactly the place a reviewer needs to read it.

## Done when

`peacockdb-core` exposes exactly the eight items in the table, every other former `pub` demoted to
`pub(crate)` rather than deleted; `pub mod` appears six times unconditionally and only in `lib.rs`, with `test_support` gated
beside them; `#![warn(unreachable_pub)]` is on and reports zero; all three
registers are deleted and the reaches they sanctioned are gone; every `pub` in `test_support` has a
signature free of engine types and that guard has been seen red; a plain `cargo build` cannot name
`test_support`; no workflow passes the feature; case counts, leaf-name sets and goldens are
unchanged; `coding-style.md`'s Visibility section carries rules and no register; the carry-over
list above is closed item by item, with the two tickets filed rather than fixed; and CI is green.
