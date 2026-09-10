# The crate's API becomes the CLI's — implementation plan

> **For agentic workers:** the coordinator dispatches one task per developer. Steps use
> checkbox (`- [ ]`) syntax. A task ends by appending its state to
> `llm-wiki/tasks/visibility-detail.md` and handing back — it does **not** commit.

**Goal:** take `peacockdb-core` from the 174 bare `pub` items tasks 4 and 5 leave to eight, move the
corpus harness behind a feature, delete the exemption registers, and write the rules that keep it
that way.

**Architecture:** components stay `pub mod` and their items become `pub(crate)` — a sibling
component needs nothing more, and only the CLI's entry points need bare `pub`. `#![warn(unreachable_pub)]`
turns the rule into a compiler check, and its warning count is the work list. Task 5 put the corpus harness behind the feature, so
nothing outside the crate needs an engine type before this task starts.

**Tech Stack:** Rust 2024, `unreachable_pub`,
`scripts/visibility-dump.py`, `test_module_layout.rs`.

**Spec:** [`visibility.md`](visibility.md) — read it before Task 1.

## Global Constraints

- **You do not mutate git state.** No commits, no branch switches, no stash. Leave work in the
  tree; the coordinator commits.
- **No test case moves and no golden is touched.** Both are invariants here, not outcomes: check
  them after every task.
- **A demotion is `pub` → `pub(crate)`, never a deletion.** An item removed and an item demoted
  look identical to a count and different to the dump.
- **Never run cudf-feature cargo builds in `./target`.** `rust-only` is plain `cargo`; the other
  two shapes go through `scripts/cargo-cudf.sh` with `CUDF_ROOT`.
- **rustfmt only the files you touched.** Comment caps: four lines in a body, ten above a
  declaration.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` on this host.

---

### Task 1: Baselines, the tooling's permanent home, and the lint that does the work

**Files:**
- Move: `llm-wiki/tasks/module-layout-baselines/{visibility-dump.py,case-inventory.sh,compare-inventory.sh}`
  → `scripts/`
- Create: `llm-wiki/tasks/visibility-baselines/{visibility-items.txt,inv-*.txt,goldens.sha256}`
- Modify: `peacockdb-core/src/lib.rs` (the lint)

**Interfaces:**
- Consumes: task 3's end state.
- Produces: the bare-`pub` count from `scripts/visibility-dump.py` — the full form, since `--items`
  drops the visibility column — and a warning count that every later task drives toward zero.

- [ ] **Step 1: Confirm the tooling is where task 3 left it**

```bash
ls scripts/{visibility-dump.py,case-inventory.sh,compare-inventory.sh}
```

Task 4 of the chain moved these three out of `module-layout-baselines/` because they are checks in
several tasks.
If they are not here, task 3 did not finish its first task; say so rather than copying them.

- [ ] **Step 2: Take the baselines**

```bash
scripts/visibility-dump.py > llm-wiki/tasks/visibility-baselines/visibility.txt
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l      # ~174 entering; 8 leaving
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  > llm-wiki/tasks/visibility-baselines/goldens.sha256
scripts/case-inventory.sh rust-only > llm-wiki/tasks/visibility-baselines/inv-rust-only.txt
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/case-inventory.sh cudf \
  > llm-wiki/tasks/visibility-baselines/inv-cudf.txt
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/case-inventory.sh gpu \
  > llm-wiki/tasks/visibility-baselines/inv-gpu.txt
```

- [ ] **Step 3: Record the three registers' entry counts**

```bash
grep -c 'PubModule {\|CrossComponentReach {' peacockdb-core/tests/test_module_layout.rs
```

`PUB_MODULES` should be empty — task 4 emptied it as it moved the files that forced each entry —
`CROSS_COMPONENT_REACHES` empty, `PUB_OUTSIDE_A_MOD_RS` two entries. If `PUB_MODULES` is not empty,
stop and say so: task 4 did not finish and this task cannot start.

- [ ] **Step 4: Turn the lint on**

At the top of `peacockdb-core/src/lib.rs`:

```rust
#![warn(unreachable_pub)]
```

- [ ] **Step 5: Count the warnings — this is the work list**

```bash
cargo build --features rust-only -p peacockdb-core 2>&1 | grep -c 'unreachable_pub\|item is not reachable'
```

Record the number. Every later task lowers it; the last one leaves zero.

- [ ] **Step 6: Append the baselines and the warning count, hand back**

---

### Task 2: Fix the three guards before anything relies on them

241 items are about to become private, which makes these guards matter more than they ever have.
Two under-report today and one double-reports.

**Files:**
- Modify: `peacockdb-core/tests/test_module_layout.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: guards the demotion slices trust.

- [ ] **Step 1: Make the private-type guard see a bare type**

`no_public_signature_names_a_type_from_a_private_module` matches only `alias::`/`module::`
prefixes, so `use some_private::Foo;` followed by `pub fn f() -> Foo` passes. Resolve imports in
the file first, then match the resolved type. Prove it: add that exact shape to a component
`mod.rs`, watch it go red, revert.

- [ ] **Step 2: Make `names_the_module`'s reverse half see a plain module import**

`use peacockdb_core::executor::cpu_backend;` has no `::` after the path and is missed. Match a
module import as well as an item path. Prove it red the same way, then revert.

- [ ] **Step 3: Stop the super-climb reader reporting one line twice**

It reports a single `super::super::x` at depth 0 twice. Fix the double count; the assertion's
meaning does not change, only its message.

- [ ] **Step 4: Run the guard target and the unit tier**

```bash
cargo test --features rust-only -p peacockdb-core --test test_module_layout
cargo test --features rust-only -p peacockdb-core --lib
```

- [ ] **Step 5: Append and hand back**

---

### Task 3: Demote `plan/mod.rs` — 92 items, the largest

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs`

- [ ] **Step 1: Demote every bare `pub` to `pub(crate)`**

All 92. None of them is a CLI entry point — `plan`'s consumers are `planner`, `executor`, `wire`
and `plan_text`, all in this crate.

- [ ] **Step 2: Build all three shapes**

```bash
cargo build --features rust-only -p peacockdb-core
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core --features gpu
```

The compiler names every consumer that needed more; there is nothing to search for.

- [ ] **Step 3: Measure both numbers**

```bash
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l
cargo build --features rust-only -p peacockdb-core 2>&1 | grep -c 'unreachable_pub\|item is not reachable'
```

Both must fall. A slice that moves neither moved the wrong thing.

- [ ] **Step 4: Cases and goldens unchanged, append, hand back**

---

### Task 4: Demote `planner/mod.rs` — 7 items, 4 stay

**Files:**
- Modify: `peacockdb-core/src/planner/mod.rs`

- [ ] **Step 1: Keep four, demote three**

`plan`, `PlanKnobs`, `BatchSizing` and `SMALL_TABLE_BYTES` stay bare `pub` — the CLI names them.
Everything else becomes `pub(crate)`.

- [ ] **Step 2: Build all three shapes**

Same three commands as the previous slice, run from the repo root with `CUDF_ROOT` set for the
last two.

- [ ] **Step 3: Confirm the CLI still builds and the four are the reason**

```bash
cargo build -p peacockdb
```

Then demote one of the four, confirm the CLI fails to build, and restore it. That is the receipt
the rule asks for: a bare `pub` is a claim the binary calls it.

- [ ] **Step 4: Measure both numbers, check cases and goldens, append, hand back**

---

### Task 5: Demote `executor/mod.rs` — 48 items, 2 stay

**Files:**
- Modify: `peacockdb-core/src/executor/mod.rs`

- [ ] **Step 1: Keep two, demote the rest**

`run` and `CpuBackend` stay. `CpuJoin`, hoisted here by task 3 to raise the `cpu_backend` wall,
becomes `pub(crate)` like the rest.

- [ ] **Step 2: Build all three shapes, then the CLI**

```bash
cargo build --features rust-only -p peacockdb-core
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core --features gpu
cargo build -p peacockdb
```

- [ ] **Step 3: Measure both numbers, check cases and goldens, append, hand back**

---

### Task 6: Demote `wire/mod.rs` — 20 items

**Files:**
- Modify: `peacockdb-core/src/wire/mod.rs`

- [ ] **Step 1: Demote all 20**

`RecipePlan` and `attach_recipes` are among them; the corpus harness that forced them is behind the
feature as of task 5, so their reason is gone.

- [ ] **Step 2: Build all three shapes**

The `gpu` shape matters here: `wire` is what the device path serialises through, so a demotion that
compiles under `rust-only` and not under `gpu` is the failure to expect.

- [ ] **Step 3: Measure both numbers, check cases and goldens, append, hand back**

---

### Task 7: Demote `plan_text/mod.rs` and `planner/translator/mod.rs` — 5 items

**Files:**
- Modify: `peacockdb-core/src/plan_text/mod.rs`, `peacockdb-core/src/planner/translator/mod.rs`

- [ ] **Step 1: Demote all five**

`render_run` is one of them, and the corpus harness that forced it is behind the feature.

- [ ] **Step 2: Build all three shapes, and the CLI**

The CLI renders plans, so `plan_text` is the component most likely to have a real bare-`pub`
consumer. If `cargo build -p peacockdb` fails, the item it names belongs in the surface table and
the spec is wrong — record that rather than working around it.

- [ ] **Step 3: Measure both numbers, check cases and goldens, append, hand back**

---

### Task 8: `common.rs`, `lib.rs`, and the last of the count

**Files:**
- Modify: `peacockdb-core/src/common.rs`, `peacockdb-core/src/lib.rs`

- [ ] **Step 1: Confirm `common.rs` is already clean**

```bash
grep -c '^pub ' peacockdb-core/src/common.rs
```

Task 2 left it at zero bare `pub` — all four items are `pub(crate)`. If it is not zero, demote.

- [ ] **Step 2: `lib.rs` keeps exactly two**

`build_session_state` and `register_tables_for`. Everything else in that file is `pub(crate)` or a
`pub mod` component declaration.

- [ ] **Step 3: The count must now be eight**

```bash
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"'
```

Expected: eight rows, matching the spec's table by file and by name — not merely eight of
something. Any surplus is either an item the CLI genuinely needs, in which case the table is
wrong and you say so, or a demotion you missed.

- [ ] **Step 4: `unreachable_pub` reports zero**

Then spell one implementation-module item `pub`, confirm it warns, revert. The lint is now known to
be on rather than assumed.

- [ ] **Step 5: Cases and goldens unchanged, append, hand back**

---

### Task 9: Delete the registers

**Files:**
- Modify: `peacockdb-core/tests/test_module_layout.rs`

- [ ] **Step 1: Delete `PUB_MODULES` and its checks**

Task 4 of the chain emptied it. Delete the constant, both directions of its check, and the helper
that parsed
`forced_by` files. An empty register is an invitation.

- [ ] **Step 2: Delete `CROSS_COMPONENT_REACHES` and its check**

Its one entry named the `CpuJoin` reach, which task 3 removed.

- [ ] **Step 3: Keep `PUB_OUTSIDE_A_MOD_RS`, narrowed**

Two entries, `lib.rs` and `common.rs`. `common.rs` now has zero bare `pub`, so drop it and leave
`lib.rs` alone.

- [ ] **Step 4: Add the assertion the registers were standing in for**

`pub mod` appears six times, all in `lib.rs`, plus `test_support` behind its feature. Anything else
is a violation with no sanctioned form.

- [ ] **Step 5: Watch it red**

Add `pub mod` to a subcomponent declaration, run the target, confirm it fails, revert.

- [ ] **Step 6: Run the target and the unit tier, append, hand back**

---

### Task 10: `coding-style.md`, and the rest of the wiki

**Files:**
- Modify: `llm-wiki/coding-style.md`, `llm-wiki/architecture.md`, `llm-wiki/build-test.md`

- [ ] **Step 1: Rewrite the Visibility section**

Delete the exemption section entire, the `CROSS_COMPONENT_REACHES` paragraph, and the "nine more
`pub mod` exist, every one forced by a test crate" clause. Keep the component and subcomponent
rules, `mod` not `pub mod`, no `pub use`, one-expression `mod.rs` bodies, three-deep nesting,
absolute `crate::` paths, and the length exemptions.

- [ ] **Step 2: Add the four rules that arrive**

The crate's API is the CLI's and bare `pub` means the binary calls it; `#![warn(unreachable_pub)]`
is what keeps that true and why the distinction is not merely a convention; `pub mod` appears in
`lib.rs` and nowhere else; a `test_support` signature is free of engine types. Keep task 2's
three-way split of what rustc enforces and what the layout test must.

- [ ] **Step 3: State the facades**

Four boundaries, each now exactly one thing: crate, component, subcomponent, `test_support`. This
is the paragraph a reader meets first, so it goes above the rules, not below them.

- [ ] **Step 4: Record why the hoist is not here**

One paragraph, per the spec: three answers to "how does a separate crate reach these types", task 3
answering it a fourth way, the measurement showing one external reach rather than fourteen, and no
ticket because a rearrangement with no consumer is the cosmetic case.

- [ ] **Step 5: `architecture.md` and `build-test.md`**

Any path or name the move changed. `build-test.md`'s tables were rebuilt in task 3; check they
still describe the tree and fix what moved, without re-deriving the numbers.

- [ ] **Step 6: Append and hand back**

---

### Task 11: The residues, and two tickets

The carry-over list from tasks 1-3, closed item by item.

**Files:**
- Modify: `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs`,
  `peacockdb-core/tests/test_cpu_end_to_end.rs`,
  `peacockdb-core/src/executor/cpu_backend/expr_physical.rs`,
  `peacockdb-core/tests/common/corpus_gpu.rs`, `llm-wiki/tickets.md`

- [ ] **Step 1: The rustfmt hunk task 1 deferred and task 2 never applied**

```bash
rustfmt --edition 2021 --check peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs
```

Expected before: one hunk. Apply it. Then the three files task 2's second review round left:
`test_cpu_end_to_end.rs`, `expr_physical.rs`, `corpus_gpu.rs`. Format the files themselves, never
the crate.

- [ ] **Step 2: The "mode" comments**

About twenty comments use "mode" as a common noun for what task 1 retired. Find them, judge each —
some uses of the word are legitimate — and fix the ones that name the retired concept.

```bash
git grep -n '\bmode\b' -- peacockdb-core/src '*.rs' | grep -i '//'
```

- [ ] **Step 3: File the murmur ticket**

The murmur gate re-derives `pmod` and the seed-42 pre-fill locally instead of calling
`rows_per_lane`, so one rule has two copies and only one is proven against comet. Production
behaviour, so it earns a number. At most fifteen lines.

- [ ] **Step 4: File the formatting ticket**

The repo is not rustfmt-clean, has no `rustfmt.toml`, and `pipeline.yml` runs neither a fmt nor a
clippy step. Say what a fix would need — a config, a one-time sweep, a CI step — and that the sweep
must not ride in a behaviour change.

- [ ] **Step 5: Final proof**

```bash
cargo test --features rust-only -p peacockdb-core
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  | diff - llm-wiki/tasks/visibility-baselines/goldens.sha256
```

Plus the three inventory comparisons and a zero from the lint. Record all of it, then hand back for
the completeness pass.
