# Tests down the source tree — implementation plan

> **For agentic workers:** the coordinator dispatches one task per developer. Steps use
> checkbox (`- [ ]`) syntax. A task ends by appending its state to
> `llm-wiki/tasks/test-layout-detail.md` and handing back — it does **not** commit.

**Goal:** move eleven integration targets plus the murmur gate into `peacockdb-core/src`, taking
the test-driven public surface from 108 items to eight, and separate test code from production
code by path.

**Architecture:** three cumulative build shapes form a ladder — `rust-only` ⊂ default ⊂ `gpu`.
Each test module declares the lowest rung it needs and is named for it (`tests`, `ffi_tests`,
`gpu_tests`), so one CI line per rung selects exactly that rung by path filter. Moves happen one
target per task, each proved by leaf-name set equality against a baseline taken before the first
move.

**Tech Stack:** Rust 2024, cargo features, `cargo test -- --list`, the module-layout baseline
tooling (bash + python), GitHub Actions, `scripts/build-test.sh` and
`scripts/build-test-shadgpu.sh`.

**Spec:** [`test-layout.md`](test-layout.md) — read it before Task 1. This plan argues from it.

## Global Constraints

- **You do not mutate git state.** No commits, no branch switches, no stash
  (`llm-wiki/prompts.md`, Developer). Leave work in the tree; the coordinator commits.
- **No golden may move at all in this task.** `sha256sum` over `testdata/goldens/` is identical at
  the end of every task.
- **This task moves tests; it deletes none.** Leaf-name set equality against the baseline is the
  check, not the count — a case deleted and another duplicated must not cancel out.
- **Never run cudf-feature cargo builds in `./target`** (`build-test.md`). The `rust-only` shape is
  plain `cargo`; the default and `gpu` shapes go through `scripts/cargo-cudf.sh`, which derives
  `CARGO_TARGET_DIR` and `CC`/`CXX` from `CUDF_ROOT`.
- **Naming, both directions:** `ffi_tests` ⇔ `#[cfg(all(test, not(feature = "rust-only")))]`;
  `gpu_tests` ⇔ `#[cfg(all(test, feature = "gpu"))]`; a module named `tests` carries neither.
- **No test code in a production file.** No `#[cfg(test)]` anywhere but on a test-module
  declaration; every test-only path in `src/` carries `test` in its name.
- **rustfmt only the files you touched** — never the crate, and name leaves rather than a `mod.rs`.
- **Comment caps:** four lines inside a function body, ten above a declaration.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` on this host.

---

### Task 1: Baselines, and tooling that knows about three rungs

Task 2's tooling enumerates two shapes and one test-module name. This task moves tests and adds
two more module names, so the tooling is extended first — with no source change, so a wrong
extension is visible as a diff in the baseline and nothing else.

**Files:**
- Create: `llm-wiki/tasks/test-layout-baselines/` (copies of `case-inventory.sh`,
  `compare-inventory.sh`, `visibility-dump.py` from `llm-wiki/tasks/module-layout-baselines/`)
- Create: `llm-wiki/tasks/test-layout-baselines/{inv-rust-only,inv-cudf,inv-gpu,visibility-items}.txt`
- Create: `llm-wiki/tasks/test-layout-baselines/goldens.sha256`

**Interfaces:**
- Consumes: nothing.
- Produces: `case-inventory.sh <rust-only|cudf|gpu>` writing one `<target>\t<case>` line per case;
  `compare-inventory.sh <shape> <baseline> <fresh>` exiting non-zero on any difference —
  it takes the baseline path, it does not derive it;
  `visibility-dump.py` writing `<scope> <vis> <kind> <name>\t<file>` — its `--items` form drops the
  visibility column, so anything counting bare `pub` must use the full form. Every later task calls
  these three.

- [ ] **Step 1: Read the three scripts you are about to copy**

Run: `head -30 llm-wiki/tasks/module-layout-baselines/{case-inventory.sh,compare-inventory.sh,visibility-dump.py}`
Their header comments state their usage exactly. `case-inventory.sh` takes a shape and enumerates
`--lib` plus every `peacockdb-core/tests/test_*.rs`; `compare-inventory.sh` takes shape, baseline and fresh
as three arguments, matching lib cases on the suffix from the last `::tests::`.

- [ ] **Step 2: Confirm the tooling is already in `scripts/`**

```bash
ls scripts/{case-inventory.sh,compare-inventory.sh,visibility-dump.py}
```

Task 2's completeness commit moved them out of `module-layout-baselines/`, so there is nothing to
move here. If they are missing, say so rather than copying them back — something reverted.

- [ ] **Step 3: Teach `compare-inventory.sh` the two new module names**

Its lib-case suffix rule matches `::tests::`. Change it to match `::tests::`, `::ffi_tests::` or
`::gpu_tests::`, taking the suffix from the last such segment. Without this every moved lib case
reads as renamed and every comparison is noise.

- [ ] **Step 4: Add the `gpu` shape to `case-inventory.sh`**

The existing `case` statement has `rust-only` and `cudf` arms. Add a `gpu` arm: same
`LD_LIBRARY_PATH` handling as `cudf`, features `--features gpu`, and `--lib` only — under the
ladder there is no separate `--test` target left on that rung after Task 9.

- [ ] **Step 5: Take the four baselines**

```bash
scripts/case-inventory.sh rust-only \
  > llm-wiki/tasks/test-layout-baselines/inv-rust-only.txt
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 \
  scripts/case-inventory.sh cudf \
  > llm-wiki/tasks/test-layout-baselines/inv-cudf.txt
scripts/visibility-dump.py > llm-wiki/tasks/test-layout-baselines/visibility.txt
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  > llm-wiki/tasks/test-layout-baselines/goldens.sha256
```

- [ ] **Step 6: Record the warning counts for both shapes that exist today**

```bash
cargo clean -p peacockdb-core && cargo build --features rust-only -p peacockdb-core 2>&1 | grep -c '^warning'
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core 2>&1 | grep -c '^warning'
```

- [ ] **Step 7: Write the numbers into the detail file and hand back**

Append to `llm-wiki/tasks/test-layout-detail.md`: the case count per shape, the item count from
`visibility-items.txt`, the two warning counts, and the bare-`pub` count that the spec's
108 → 8 ladder will be measured against —
`scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l`. The `--items` form cannot do
it: it drops the visibility column, so an `$2=="pub"` filter over it matches nothing and reports a
confident zero.
Every later task compares against these, so a wrong number here is a wrong task.

---

### Task 2: The `gpu` feature and the mutually-exclusive guard

**Files:**
- Modify: `peacockdb-core/Cargo.toml` (`[features]`)
- Modify: `peacockdb-core/src/lib.rs` (the `compile_error!`)

**Interfaces:**
- Consumes: nothing.
- Produces: `feature = "gpu"`, on which every device test module is gated from Task 9 onward.

- [ ] **Step 1: Add the feature**

```toml
[features]
rust-only = ["peacockdb-ffi/rust-only"]
gpu = []                # device tests; not propagated to peacockdb-ffi, which has no device path
```

- [ ] **Step 2: Add the guard at the top of `lib.rs`**

```rust
#[cfg(all(feature = "gpu", feature = "rust-only"))]
compile_error!("gpu needs the FFI linked; rust-only removes it. Pass one or neither.");
```

- [ ] **Step 3: Prove the guard fires**

Run: `cargo build --features "gpu rust-only" -p peacockdb-core 2>&1 | head -5`
Expected: the `compile_error!` message, not a link failure and not a success.

- [ ] **Step 4: Prove the three shapes still build**

```bash
cargo build --features rust-only -p peacockdb-core
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core --features gpu
```

Expected: all three succeed, and the first two at the warning counts Task 1 recorded.

- [ ] **Step 5: Prove no case moved**

```bash
scripts/case-inventory.sh rust-only > /tmp/inv.txt
scripts/compare-inventory.sh rust-only llm-wiki/tasks/test-layout-baselines/inv-rust-only.txt /tmp/inv.txt
```

Expected: exit 0, no output. A feature nothing uses must move nothing.

- [ ] **Step 6: Append to the detail file and hand back**

---

### Task 3: `src/test_support/` — the feature, the shared harness, and the one testdata root

Nine of the eleven moving targets read `tests/common/`, and the seven binaries that stay read the
same files. A helper with two audiences goes behind the feature; duplicating it guarantees drift.

**Files:**
- Modify: `peacockdb-core/Cargo.toml` (`[features]`, `[dev-dependencies]`),
  `peacockdb-core/src/lib.rs` (declare the module)
- Create: `peacockdb-core/src/test_support/{mod.rs,testdata.rs}`
- Move: `peacockdb-core/tests/common/{golden_text.rs,registry.rs,mode.rs,memory_limit.rs}` and the
  shared helpers out of `tests/common/mod.rs`
- Modify: `peacockdb-core/tests/common/mod.rs` (delegate, do not reimplement), and the seven
  `CARGO_MANIFEST_DIR` sites — find them with
  `git grep -ln 'CARGO_MANIFEST_DIR' -- peacockdb-core/src`

**Interfaces:**
- Consumes: nothing.
- Produces: `crate::test_support::testdata::root() -> PathBuf`, plus `Mode`, `MODES`,
  `MemoryLimit`, the golden-text reader and the registry loader. In-crate code says
  `crate::test_support::…`; the binaries that stay say `peacockdb_core::test_support::…`.

- [ ] **Step 1: Declare the feature and the self dev-dependency**

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

In `lib.rs`: `#[cfg(feature = "test-support")] pub mod test_support;`

- [ ] **Step 2: Write the testdata root, and make it the only one**

```rust
//! The testdata root. A test binary is built on one host and run on another, so the
//! compile-time path is a fallback and the environment wins (#49).
use std::path::PathBuf;

pub fn root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PEACOCK_TESTDATA_DIR") {
        return PathBuf::from(dir);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../testdata")
}
```

Read `tests/common/mod.rs:43` first and match its fallback exactly, including whether it resolves
a `canonical_root()` symlink. Then make `testdata_root()` there a one-line call to this, not a
second implementation — that is the whole of #49.

- [ ] **Step 3: Move the four helper files and the shared `mod.rs` helpers**

`golden_text.rs`, `registry.rs`, `mode.rs`, `memory_limit.rs`, and from `common/mod.rs` the ones
the moving targets name: `data_dir_for`, `golden_dir_for`, `queries_dir_for`, `GPU_BUDGET`,
`assert_results_match`, `total_rows`, `testdata_minimal_dir`. `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` stay: only binaries that stay read them.

**Bare `pub` only in `test_support/mod.rs`.** Everything below it is `pub(crate)`. The layout test
already fails on a bare `pub` outside a `mod.rs`, and `test-support.md` turns on
`unreachable_pub`, which would fire on every one of them.

- [ ] **Step 4: Convert the seven `CARGO_MANIFEST_DIR` sites**

Each becomes `crate::test_support::testdata::root().join("tpch.minimal")`.
Run: `git grep -n 'CARGO_MANIFEST_DIR' -- peacockdb-core/src`
Expected: no hits.

- [ ] **Step 5: Prove the feature is off in a plain build**

Reference `crate::test_support` from a non-test path in `lib.rs`, run
`cargo build --features rust-only -p peacockdb-core`, confirm `E0433`, revert. A passing build
proves nothing — the module simply is not there.

- [ ] **Step 6: Prove no CI step passes the feature**

Run: `grep -rn 'test-support' .github/workflows/`
Expected: no hits. If one appears, the self dev-dependency is not doing its job.

- [ ] **Step 7: Everything still compiles, on both sides of the boundary**

```bash
cargo test --features rust-only -p peacockdb-core --lib
cargo test --features rust-only -p peacockdb-core --no-run
```

The second is the one that matters: all eighteen integration targets must still build against the
helpers in their new home.

- [ ] **Step 8: Prove the override works, which is the point of the exercise**

Run: `PEACOCK_TESTDATA_DIR=/tmp/nonexistent cargo test --features rust-only -p peacockdb-core --lib estimator 2>&1 | tail -5`
Expected: failures naming `/tmp/nonexistent`. If it passes, the variable is not being read and the
device tier will silently read the wrong tree on shad-gpu.

- [ ] **Step 9: Compare inventories, then hand back**

No case moves in this task. State in the detail file that #49's residual is closed and the ticket
can be retired by the completeness pass.

### Task 4: Teach `test_module_layout` the layout rules, and make the tree obey them

**Files:**
- Modify: `peacockdb-core/tests/test_module_layout.rs`
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs` (four item-level gates)
- Create: `<module>/tests.rs` for each file now holding an inline `mod tests { … }`
- Rename: `driver/mock.rs` → `driver/tests/mock.rs`, `driver/plans.rs` → `driver/tests/plans.rs`

**Interfaces:**
- Consumes: the naming rule from Global Constraints.
- Produces: five assertions every later task leans on. Name them so a failure says which rule
  broke: `a_test_module_is_named_for_its_rung`, `a_rung_gate_implies_its_module_name`,
  `cfg_test_appears_only_on_a_test_module`, `a_test_only_path_carries_test`,
  `a_test_module_lives_in_its_own_file`.

- [ ] **Step 1: Write the four failing assertions**

Read the tree, do not compile it. For every `mod (tests|ffi_tests|gpu_tests);` declaration in
`peacockdb-core/src/**`, capture the `#[cfg(...)]` attribute on the line above:

```rust
// name → required gate, both directions
"tests"      => neither `feature = "gpu"` nor `not(feature = "rust-only")`
"ffi_tests"  => all(test, not(feature = "rust-only"))
"gpu_tests"  => all(test, feature = "gpu")
```

Plus: every `#[cfg(test)]` in `src/` sits on one of those three declarations, and every path
containing a test module contains `test`.

- [ ] **Step 2: Run them and watch three go red**

Run: `cargo test --features rust-only -p peacockdb-core --test test_module_layout`
Expected: `cfg_test_appears_only_on_a_test_module` FAILS naming
`src/executor/driver/partitioned.rs` lines 670, 675, 680, 685 — four item-level `#[cfg(test)]`
attributes. The other three pass vacuously today; there are no `ffi_tests` or `gpu_tests` modules
yet.

- [ ] **Step 3: Fix the four item-level gates**

Move each gated item into the `mod tests` that uses it. Re-run: all four assertions pass.

- [ ] **Step 4: Red-watch each new rule by construction**

One at a time, and revert after each:
- rename an existing `mod tests` to `mod gpu_tests` without changing its gate → rule 2 red
- add `#[cfg(all(test, feature = "gpu"))]` above a `mod tests` → rule 1 red
- add `#[cfg(test)] const X: u8 = 0;` to `src/planner/mod.rs` → rule 3 red
- create `src/planner/helpers/mod.rs` holding a `#[test]` → rule 4 red

A rule that has never been red is a rule you have not tested.

- [ ] **Step 5: Teach `TEST_DIRS` the two new names**

`const TEST_DIRS: &[&str] = &["tests"]` drives the subcomponent, sibling-reach and super-climb
readers. Leave it and the first `executor/ffi_tests/` directory is read as a subcomponent of
`executor`, and those rules fire on it. Extend it to `["tests", "ffi_tests", "gpu_tests"]` here,
before any module moves.

- [ ] **Step 6: Add the fifth rule — a test module lives in its own file**

`#[cfg(test)] mod tests { … }` with a body inline in a production file is test code in a
production file, which is what this task exists to end. The rule: a test-module declaration is
`mod tests;` — a declaration, not a block. Run the layout test and watch it name thirteen files.

- [ ] **Step 7: Split what that finds**

Resolve the paths against the current tree, since task 2 moved them:

```bash
git grep -ln '#\[cfg(test)\]' -- peacockdb-core/src \
  | xargs grep -ln 'mod tests {'
```

The spec deliberately gives no list: task 2 renamed and moved most of them and deleted `config.rs`,
so whatever this finds is the answer. Record the count and the paths. Each becomes `foo.rs`
beside `foo/tests.rs`, which is already the pattern in `expr_physical`, `accounting`, `index`,
`scheduler`, `single_partition` and `expr_writer`. Do not reach for `clippy::mod_module_files` to
enforce this: it cannot be scoped this way and would reject exactly that pairing.

- [ ] **Step 8: Rename the two test-only files that do not say so**

`driver/mock.rs` and `driver/plans.rs` are `#[cfg(test)]` today and carry no `test` in their
names, which rule 4 catches. They become `driver/tests/mock.rs` and `driver/tests/plans.rs`.
`translate/schema_tests.rs` already carries the word and stays.

- [ ] **Step 9: Run the unit tier and compare**

```bash
cargo test --features rust-only -p peacockdb-core --lib
scripts/case-inventory.sh rust-only > /tmp/inv.txt
scripts/compare-inventory.sh rust-only llm-wiki/tasks/test-layout-baselines/inv-rust-only.txt /tmp/inv.txt
```

Expected: green, and no case difference — splitting a module moves its cases' module paths, which
is exactly what Task 1 Step 3 taught the comparison to ignore.

- [ ] **Step 10: Compare inventories and hand back**

`test_module_layout` gains cases; that is the one target whose leaf-name set legitimately grows.
Record the new names in the detail file so Task 12's arithmetic can subtract them.

---

### Task 5: `test_gpu_batch` → `executor/ffi_tests/` — the shape proof and the middle rung

Three cases, two items, and the only target on the middle rung. It proves the whole mechanism
before anything expensive moves.

**Files:**
- Create: `peacockdb-core/src/executor/ffi_tests/mod.rs` (+ the moved case bodies)
- Delete: `peacockdb-core/tests/test_gpu_batch.rs`
- Modify: `peacockdb-core/src/executor/mod.rs` (declare the module)
- Modify: `.github/workflows/pipeline.yml` — **two** lines name this target: the run step, which
  the swap below replaces, and a `cargo test --no-run … --test test_plan_goldens --test
  test_gpu_batch` prebuild that will error once the target is gone

**Interfaces:**
- Consumes: `feature = "gpu"` (Task 2), the layout test (Task 4).
- Produces: the `ffi_tests::` filter contract that Task 11's CI assertion checks.

- [ ] **Step 1: Move the file**

`peacockdb-core/tests/test_gpu_batch.rs` has no `mod common`, so it moves whole. Its file-level
`#![cfg(not(feature = "rust-only"))]` becomes the module's gate:

```rust
#[cfg(all(test, not(feature = "rust-only")))]
mod ffi_tests;
```

Drop the `#![cfg(...)]` inner attribute from the moved file and change
`use peacockdb_core::...` to `use crate::...`.

- [ ] **Step 2: Prove the rung selects exactly three cases**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: --list
```

Expected: exactly 3 cases, all under `executor::ffi_tests::`.

- [ ] **Step 3: Prove the lower rung does not compile them**

```bash
cargo test --features rust-only -p peacockdb-core --lib -- ffi_tests:: --list
```

Expected: 0 cases, and a clean build — not a link error. `GpuBatch` exists only where the FFI is
linked, so the gate is what keeps the rung honest.

- [ ] **Step 4: Swap the CI step**

In `pipeline.yml`, drop `--test test_gpu_batch` from the prebuild line and replace the run line
  with
`cargo test -p peacockdb-core --lib -- ffi_tests::` at default features, in the same job and the
same feature shape. The job already compiles that shape for `peacockdb-ffi --test test_ffi`, so
this is a swap, not a second compile of the DataFusion stack.

- [ ] **Step 5: Compare inventories on both shapes**

The three cases must have left `test_gpu_batch`'s list and appeared under `--lib` in the cudf
shape, and nowhere else.

- [ ] **Step 6: Check the goldens have not moved, then hand back**

```bash
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  | diff - llm-wiki/tasks/test-layout-baselines/goldens.sha256
```

Expected: no output. Run this at the end of every remaining task.

---

### Task 6: The injector trio and `test_layout_injection` → `plan/tests/` and `src/tests/`

The trio forces 73 of the 108 items. This is the task that moves the number.

**Files:**
- Create: `peacockdb-core/src/tests/{injection,rebuild,join_fixture}.rs`
- Modify: `peacockdb-core/src/plan/tests/mod.rs` — it exists, with `aggregate.rs` and `joins.rs`
  beside it; declare the new module, do not overwrite the file
- Delete: `peacockdb-core/tests/common/{injection,rebuild,join_fixture}.rs`,
  `peacockdb-core/tests/test_layout_injection.rs`
- Modify: `peacockdb-core/tests/common/mod.rs` (drop the three `mod` lines)

**Interfaces:**
- Consumes: `crate::test_support::{testdata, …}` (Task 3).
- Produces: `crate::tests::{injection, rebuild, join_fixture}`, reached by Task 7's targets.

- [ ] **Step 1: Find every consumer before moving anything**

Run: `git grep -ln 'injection::\|rebuild::\|join_fixture::' -- peacockdb-core/tests`
Every file listed either moves in this task or breaks. If a target you did not expect appears,
stop and record it in the detail file — the spec's slice order assumed four consumers.

- [ ] **Step 2: Move the trio to `src/tests/`**

They construct plan nodes, wire recipes and backend executors alike, so they sit above every
component. Declare them in `src/tests/mod.rs`. No `#[cfg(test)]` inside — the root gate covers it.

- [ ] **Step 3: Move `test_layout_injection` into `plan/tests/`**

Its cases are pure Rust, so the module is `tests` and carries no rung gate.

- [ ] **Step 4: Run and compare**

```bash
cargo test --features rust-only -p peacockdb-core --lib
scripts/case-inventory.sh rust-only > /tmp/inv.txt
scripts/compare-inventory.sh rust-only llm-wiki/tasks/test-layout-baselines/inv-rust-only.txt /tmp/inv.txt
```

Expected: the four cases moved from `test_layout_injection` into `--lib`, nothing else changed.

- [ ] **Step 5: Close the register entries this move invalidates**

`injection.rs` was the only forcer of `executor/cpu_backend/join` and `executor/cpu_backend/source`,
and one of two for `executor/cpu_backend`, `.../accumulate` and `.../emit`. Drop the two entries
that now have no forcer, and demote those two `pub mod` to `mod`, converting their items to
`pub(crate)` — the compiler names every consumer. Leave the other three entries: they still have
`test_cpu_executors.rs`, which Task 8 moves.

Run: `cargo test --features rust-only -p peacockdb-core --test test_module_layout`
Expected: green. Red on a stale entry means you dropped one half and not the other — the register
is checked both ways.

- [ ] **Step 6: Measure both ladders**

```bash
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l
grep -c 'PubModule {\|CrossComponentReach {' peacockdb-core/tests/test_module_layout.rs
```

Expected: the surface materially below Task 1's figure — this is the slice that removes most of the
demand — and the register at seven entries. A slice that moves neither number moved the wrong
thing; record both either way.

- [ ] **Step 7: Goldens unchanged, append to the detail file, hand back**

---

### Task 7: The planner four → `planner/tests/`

`test_planner_join_capability` (13), `test_planner_join_refusals` (10), `test_null_analysis` (8),
`test_plan_goldens` (19).

**Files:**
- Create: `peacockdb-core/src/planner/tests/{join_capability,join_refusals,null_analysis,plan_goldens}.rs`
  and `peacockdb-core/src/planner/tests/mod.rs`
- Delete: the four files under `peacockdb-core/tests/`
- Modify: `peacockdb-core/src/planner/mod.rs`

**Interfaces:**
- Consumes: `crate::tests::{injection, rebuild, join_fixture}` (Task 6),
  `crate::test_support::{testdata, …}` (Task 3).
- Produces: nothing later tasks name.

- [ ] **Step 1: Move one target, run it, then the next**

`test_null_analysis` has no `mod common` and is the cheapest; take it first as the shape check for
this destination, then the other three. For each, in order: move the file under
`src/planner/tests/`, declare it in `src/planner/tests/mod.rs`, rewrite `use peacockdb_core::` to
`use crate::`, and every `common::…` path to `crate::test_support::…` for a helper Task 3
moved or `crate::tests::…` for the injector trio, then:

```bash
cargo test --features rust-only -p peacockdb-core --lib -- planner::tests
```

- [ ] **Step 2: After each, run the whole unit tier, not just your module**

Run: `cargo test --features rust-only -p peacockdb-core --lib`
A move that compiles in isolation can still shadow a name at crate level.

- [ ] **Step 3: After all four, compare inventories**

Expected: 50 cases moved from four binaries into `--lib`, none lost.

- [ ] **Step 4: `test_plan_goldens` writes goldens — prove it did not**

Run the golden checksum diff. `UPDATE_CANONICAL` is unset, so the target verifies rather than
rewrites; if a byte moved, the move changed a path a golden records.

- [ ] **Step 5: Measure the ladder, append, hand back**

---

### Task 8: `test_cpu_executors` → `executor/cpu_backend/tests/`

One case, and it reads `common/executor_cases.inc` — the table the device side also reads.

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/tests/mod.rs` — it exists, with six files
  beside it; declare the new module, do not overwrite the file
- Delete: `peacockdb-core/tests/test_cpu_executors.rs`
- Modify: `peacockdb-core/src/executor/cpu_backend/mod.rs`

**Interfaces:**
- Consumes: `executor_cases.inc`, which stays in `tests/common/` for now.
- Produces: the `include!` path that Task 9's device half must match.

- [ ] **Step 1: Decide where the `.inc` lives, and say so in the detail file**

Both engines read it, and the device half moves in Task 9. Keep one copy: `include!` it from
`src` by relative path, and record that path — two copies of that table is the drift the file
exists to prevent.

- [ ] **Step 2: Move, run, compare, check goldens**

Run: `cargo test --features rust-only -p peacockdb-core --lib -- cpu_backend::tests`

- [ ] **Step 3: Declare `has_finish_pass` in `executor/mod.rs`, and rename the method it calls**

`wire/tests.rs` names `executor::cpu_backend::join::CpuJoin`, so the next step is an `E0603` on
that line without this. Do not hoist the type — the test wants one answer, not the type:

```rust
pub(crate) fn has_finish_pass(node: &GpuJoin, build: &Fields, probe: &Fields,
    ctx: Arc<TaskContext>) -> Result<bool, PlanError> {
    cpu_backend::join::CpuJoin::hash(node, build, probe, ctx).map(|e| e.has_finish_pass())
}
```

Rename `CpuJoin::makes_a_finish_pass` to `has_finish_pass` in the same step — `coding-style.md`
says a bool-returning function reads as a claim, and "makes" promises an effect. Then rewrite
`wire/tests.rs` to call `crate::executor::has_finish_pass(...)`, keeping both halves of what it
asserts: a refused cell must give `Err`, and an allowed cell's answer must equal whether the
recipe carries an `AtDone` call.

- [ ] **Step 4: Close the `cpu_backend` group**

`test_cpu_executors.rs` was the second forcer of `executor/cpu_backend`, `.../accumulate` and
`.../emit`. Drop all three entries, demote those three `pub mod` to `mod`, convert their items to
`pub(crate)`, and delete the `CROSS_COMPONENT_REACHES` entry that named the reach you just removed.

Run: `cargo test --features rust-only -p peacockdb-core --test test_module_layout`
Expected: green, with four entries left — all four `gpu_backend`, which Task 9 takes.

- [ ] **Step 5: Measure both ladders, append, hand back**

---

### Task 9: The device four → `gpu_tests`, and both scripts

`test_gpu_recipe_walk` (10) → `wire/gpu_tests/`, `test_gpu_executors` (31, already a directory of
five modules) and `test_gpu_abi` (4) → `executor/gpu_backend/gpu_tests/`,
`test_inc2_conformance` → `executor/cpu_backend/gpu_tests/` as `murmur_conformance`.

**Files:**
- Create: the three `gpu_tests/` directories
- Delete: the four files (and `tests/test_gpu_executors/`)
- Modify: `scripts/lib/shadgpu-env.sh` — `stage_cargo_test_binary` is here, not in
  `build-test-shadgpu.sh`
- Modify: `scripts/build-test-shadgpu.sh` (`RUST_TESTS`), `scripts/build-test.sh`
  (`gpu_runtime_targets`, the murmur literal)
- Modify: `.github/workflows/pipeline.yml` — **two** places: the staging loop, which duplicates the
  same `--test "$t"` resolver inline and does not pass `--features gpu`, and the remote run step

**Interfaces:**
- Consumes: `feature = "gpu"` (Task 2), the layout test (Task 4).
- Produces: the staged filename `peacockdb_core_gpu_lib` and the `gpu_tests::` filter, both of
  which Task 11 asserts.

- [ ] **Step 1: Move the four, one at a time, each gated `all(test, feature = "gpu")`**

`test_gpu_executors` keeps its five-module shape under `gpu_tests/`. The murmur gate names nothing
from `peacockdb_core` — it drives comet's `create_murmur3_hashes` and one FFI symbol — and lands
beside `spark_partitioning.rs`, the CPU half of the invariant it protects. Its three ungated cases
move with it and become device-rung cases; say so in the detail file, since that is a coverage
change, not just a move.

- [ ] **Step 2: Prove the filter selects the device set and nothing else**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu -- gpu_tests:: --list
```

Expected: 55 cases, every one under a `gpu_tests` path. A CPU case here means a name and a gate
disagree — Task 4's rule 1 should already have caught it, so a surprise means that rule is wrong.

- [ ] **Step 3: Teach `stage_cargo_test_binary` the lib target**

It matches `target.name` against a `--test` name in cargo's json. The lib test target's entry has
`kind: ["lib"]` and `test: true` and `target.name` is the crate name, so it needs a second form.
Stage it as `peacockdb_core_gpu_lib` — the run loop globs `cpp/install/rust-tests/*` and a bare
crate name reads as ambiguous beside the target binaries.

- [ ] **Step 4: Give the lib binary its filter argument**

The run loop passes `--test-threads=1` to every staged binary alike; the lib binary alone also
takes `gpu_tests::`. Set `RUST_TESTS=(test_gpu_corpus)` — one entry.

- [ ] **Step 5: Shrink `gpu_runtime_targets()` and delete the murmur literal**

In `scripts/build-test.sh`: `gpu_runtime_targets()` becomes `test_gpu_corpus` plus the lib entry.
Line 309's literal `peacockdb-core:test_inc2_conformance` is **deleted, not renamed** — the target
stops existing, and a renamed literal names a `--test` target that is not there, which is #176's
failure: cargo errors late, inside the cuDF leg. The ten lines of comment above it go with it.

- [ ] **Step 6: Prove both derived suites are still non-empty**

`build-test.sh` has no dry-run flag, so read `needs_cmake_targets()` and `gpu_runtime_targets()`
and evaluate them by hand against the tree: each must still name at least one target. The
"derived suite must not be EMPTY" guard is now one move from firing, which is why it stays.

- [ ] **Step 7: Run it on the device**

```bash
scripts/build-test-shadgpu.sh --build --push-binaries --patch --run-detached
scripts/build-test-shadgpu.sh --run-status
```

Run this in the foreground and stay in the call — a backgrounded shad-gpu cycle is killed
mid-build. Expected: the 55 cases pass, and the run takes roughly what the five staged binaries
took. Materially longer means the filter selects more than it should; `PASSED 0 tests` means it
selects nothing.

- [ ] **Step 8: Close the four `gpu_backend` entries**

`test_gpu_executors.rs` and its three child files were the only forcers. They move whole in Step 1,
which is deliberate: task 2 recorded that if the parent file alone stopped naming
`executor/gpu_backend`, the forward half would go red and the children could not re-justify the
entry. Drop all four, demote `executor/gpu_backend` and its three subcomponents to `mod`, convert
their items to `pub(crate)`. `gpu_backend` keeps its `#[cfg(not(feature = "rust-only"))]` — that is
a rung gate, not a visibility one.

Run: `cargo test --features rust-only -p peacockdb-core --test test_module_layout`
Expected: green, `PUB_MODULES` empty. Leave the empty register in place; task 4 deletes it.

- [ ] **Step 9: Compare all three inventories, check goldens, hand back**

Record the final ladders: bare `pub` count, register at zero, `pub mod` down from 15 to 6.

---

### Task 10: `test_cpu_end_to_end` → `src/tests/`

26 cases, two `#[ignore]`d against #182. The end-to-end tier sits at crate level because it needs
every component at once.

**Files:**
- Create: `peacockdb-core/src/tests/end_to_end.rs`
- Delete: `peacockdb-core/tests/test_cpu_end_to_end.rs`
- Modify: `peacockdb-core/src/tests/mod.rs`

- [ ] **Step 1: Move it, keeping both `#[ignore]` attributes and their ticket references**

An ignored case still lists, so leaf-name equality covers them; silently dropping one would not
show up in a pass/fail count.

- [ ] **Step 2: Run, compare, check goldens**

Run: `cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end`
Expected: 24 pass, 2 ignored.

- [ ] **Step 3: Measure both ladders — this is where the test-forced count should reach eight**

```bash
scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l
grep -c 'PubModule {\|CrossComponentReach {' peacockdb-core/tests/test_module_layout.rs
```

The raw bare-`pub` count stays in the hundreds — most of those items are `pub` for no reason and
task 4 demotes them, so do not expect eight here. What must be true is narrower and you check it by
reading the dump rather than counting it: the only items still `pub` **because a test crate names
them** are the eight `corpus.rs` and `corpus_gpu.rs` force. Name any other such item in the detail
file; it is either a lift the spec predicted or a move that did not happen. The register must be at
zero.

- [ ] **Step 4: Append and hand back**

---

### Task 11: `test_ci_coverage` — one assertion per rung, each shown red

**Files:**
- Modify: `peacockdb-core/tests/test_ci_coverage.rs`

**Interfaces:**
- Consumes: the CI lines Tasks 5 and 9 wrote.
- Produces: the guard that keeps them there.

- [ ] **Step 1: Shrink the sweep**

Seven targets, not nineteen. `INTENTIONALLY_NOT_IN_CI` drops from six entries to two. The three
GPU target lists become one staging list of one binary plus the lib build. Its own matcher unit
tests stay whole — they are the reason this guard can go red at all.

- [ ] **Step 2: Assert four lines exist**

`--lib` under `--features rust-only`; `--lib -- ffi_tests::` at default features;
`--lib --features gpu -- --test-threads=1 gpu_tests::` on shad-gpu; and the CLI build. Follow
`line_runs_lib_tests` and `line_builds_the_cli` — they are the pattern for reading a workflow line.

- [ ] **Step 3: Red-watch all four**

Delete each line from `pipeline.yml` in turn, run the guard, confirm it fails naming that line,
restore it. One rung per line, and the assertion is all that stands between a rung and silently
not running.

- [ ] **Step 4: Confirm the three lists agree**

Run: `cargo test --features rust-only -p peacockdb-core --test test_ci_coverage`
Expected: green, including `the_three_gpu_target_lists_agree` and
`both_gpu_runners_pass_test_threads_one`.

- [ ] **Step 5: Append and hand back**

---

### Task 12: The wiki, from measured numbers

Edited last, and from the measurements rather than from the spec.

**Files:**
- Modify: `llm-wiki/build-test.md`, `llm-wiki/coding-style.md`, `llm-wiki/architecture.md`,
  `llm-wiki/tickets.md` (retire #49)

- [ ] **Step 1: Rebuild the test table from the inventories, blocked by rung**

Two tables. The first holds the Rust tests against production code — `peacockdb-core` and
`peacockdb-ffi` — in three blocks opened by a bolded header row: **cpu**, **ffi**, **gpu**. Inside
a block, group by tier (crate integration external, crate integration internal, component,
subcomponent, module unit) and then by category. Keep the Why, Examples and N columns; drop
`Runs`, which the block header now says once. The second table holds the C++ suites, the Python
sets, `cost-report`, and the three repo guards — `test_ci_coverage`, `test_module_layout` and
`test_golden_format` — none of which runs engine code. `test_corpus_goldens` stays in the first,
cpu rung.

Assign each file by where its test module is declared, not by eye. Take every figure from the Task
1 baselines and the final inventories.

- [ ] **Step 2: Check the page's arithmetic still holds**

Task 2 fixed the four-case discrepancy in its completeness commit, so the header and the column
agree entering this task. The two tables you write must still add to the headline figure. A spec
number and a measured number that disagree mean the page follows the measurement.

- [ ] **Step 3: `coding-style.md`**

Add the test-code rules and the rung ladder — a test module declares the lowest build shape it
needs and is named for it, with name and gate implying each other above the floor. Delete the
`test_inc2_conformance` exception paragraph opening the Names section: the rename closes it.
Amend the visibility section with what the rules settled at, counted by `visibility-dump.py`.

- [ ] **Step 4: `architecture.md`**

The Execution section's driver and accountant paths, the wire-format section's writer paths, and
the Rehash section's `spark_partitioning.rs` pointer and its conformance gate, which now sits
beside it.

- [ ] **Step 5: Retire #49**

Task 3 closed it. Move it to `llm-wiki/archive/archived-tickets.md` under Done, with one line
saying the crate now has one testdata root.

- [ ] **Step 6: Final proof, then hand back for the completeness pass**

```bash
cargo test --features rust-only -p peacockdb-core
find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum \
  | diff - llm-wiki/tasks/test-layout-baselines/goldens.sha256
```

Plus the three inventory comparisons and the final ladder measurement. Record all of it.
