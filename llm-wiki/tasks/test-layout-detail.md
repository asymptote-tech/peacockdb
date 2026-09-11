# test-layout — run detail

Spec: [`test-layout.md`](test-layout.md). Plan: [`test-layout-impl.md`](test-layout-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 4. Branch `ENS-test-layout`, forked at `960add37`, the tip of
task 3's branch. **Its PR targets `ENS-rmm-pool-budget`**, not master: task 3 is `done` but not
merged, so it is the parent that exists. Tasks 1 and 2 are already in master, which is why task 3
aimed there instead.

## How this task is dispatched, and why it is not one dispatch

The spec says it outright: **a slice is a dispatch, not just a commit.** Each of the plan's twelve
tasks ends by appending to this file and handing back, and the next starts in a fresh window from
the baselines and this file. Task 2 of this chain is the lesson — the same shape, run as one
dispatch, spent eight hours and died on a context limit with the work unreported.

So the coordinator commits after each slice and dispatches the next. A slice that reports green
without a ladder number in this file has not finished, whatever it says.

## Hosts at dispatch (2026-09-10)

- **verda** up: `wide-hand-falls-fin-03`, 8 cores. The large CPU batches this task generates belong
  there via `scripts/build-test.sh --host verda`.
- **shad-gpu** up, but a tenant outside this repo has held 53 GiB of the card all day. Only plan
  task 9 (the device four) needs it.

## The two things that make this task silent when it goes wrong

1. **A lost case is invisible.** A target that runs 30 tests instead of 31 goes green. The check is
   **leaf-name set equality** against the baseline union — the set, not the count, so a deletion and
   a duplication cannot cancel out.
2. **Every figure in the spec predates task 2**, which renamed five targets, added
   `test_module_layout` and its 35 cases, and moved the tree. The baselines are the authority. Where
   a measured number and the spec disagree, the measurement is right and the sentence is stale —
   record that here rather than bending the move to fit the page.

## Run log

### 2026-09-10 — slice 1 dispatched: baselines and three-rung tooling

Board set to `building`. Nothing measured yet.

### 2026-09-10 — slice 1 done: baselines and three-rung tooling

Plan task 1, steps 1-7. No file under `peacockdb-core/` was touched. The only source changes are
`scripts/case-inventory.sh` (a `gpu` arm) and a comment in `scripts/compare-inventory.sh`. Not
committed; the work is in the tree.

#### The four baselines

All in `llm-wiki/tasks/test-layout-baselines/`, taken on a clean tree at the tip of
`ENS-test-layout`, with `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2`.

1. `inv-rust-only.txt` — `scripts/case-inventory.sh rust-only`. **1034 cases**: `--lib` 435 plus
   19 integration targets. The five `test_gpu_*` targets list zero here, which is the `rust-only`
   cfg removing them and not the empty-list failure mode `build-test.md` warns about.
2. `inv-cudf.txt` — `CUDF_ROOT=... scripts/case-inventory.sh cudf`. **1097 cases**: the same
   `--lib` 435 plus 63 cases that only the default shape compiles (the five gpu targets, 56, and
   seven more in `test_inc2_conformance`). `--lib` is the same 435 in both shapes, so no lib test
   module is FFI-gated today.
3. `visibility.txt` and `visibility-items.txt` — `scripts/visibility-dump.py` and its `--items`
   form. **578 records** each, 264 bare `pub` and 314 `pub(crate)`. Byte-identical to task 2's
   `visibility-final.txt` and `visibility-items-final.txt`.
4. `goldens.sha256` — `find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0
   sha256sum`. **170 files.** No golden may move for the rest of this task, so this is the file to
   diff against.

Both inventories compare `identical` against task 2's finals
(`scripts/compare-inventory.sh <shape> llm-wiki/tasks/module-layout-baselines/inv-<shape>-final.txt
llm-wiki/tasks/test-layout-baselines/inv-<shape>.txt`). The tree is exactly where task 2 left it,
and the tooling round-trips.

#### Warning counts (step 6)

Both shapes: **0 warnings**, from `cargo clean -p peacockdb-core` followed by the build, in the
right target dir each time (plain `cargo` for `rust-only`, `scripts/cargo-cudf.sh` for the default
shape). The `gpu` shape cannot be measured until plan task 2 adds the feature.

#### The ladder's starting numbers

- Bare `pub`, excluding `mod` — `scripts/visibility-dump.py | awk '$2=="pub" && $3!="mod"' | wc -l`
  — is **249**: 103 `impl fn`, 73 struct, 32 enum, 24 `fn`, 13 trait, 2 type, 2 const. Concentrated
  in `plan/mod.rs` (92), `executor/mod.rs` (48) and `wire/mod.rs` (20).
- `pub mod` is **15**, which is the figure the spec's 15 → 6 expects.
- The `PUB_MODULES` register in `peacockdb-core/tests/test_module_layout.rs` has **9** entries,
  which is the figure the spec's 9 → 0 expects.

#### Where a measurement contradicts the page

- **`test_module_layout` has 11 cases, not 35.** The spec says 35 twice (lines 565 and 584) and
  this file repeated it. Task 2 added 11 cases in total: 1023 → 1034 rust-only and 1086 → 1097
  cudf, measured across its own before and after baselines. The sentence is stale.
- **The ladder starts at 249, not 108.** The spec's 108 → 79 → 50 → 15 → 8 predates task 2, and
  the plan says to re-derive the start from the command above. Only the first number is measured
  here; whether the later rungs still hold is for the slices that reach them.
- **Step 3 was already done.** `compare-inventory.sh` normalises on any segment ending in `tests`,
  so it already handled `ffi_tests` and `gpu_tests`, already took the last such segment, and still
  caught a renamed leaf. Narrowing it to the three rung names, as the step words it, would have
  broken `schema_tests`, which nine lib cases use today. Verified with fixtures rather than by
  reading: same leaf names under different paths compare identical, one renamed leaf exits 1, and
  a test module at the crate root normalises the same as a nested one. Only the header comment
  changed, to name the four module names the rule actually covers.
- **`inv-gpu.txt` cannot exist yet.** The plan's file list asks for it, but `--features gpu` does
  not exist until task 2, and `scripts/case-inventory.sh gpu` fails on exactly that today. Step 5
  is right to omit it. The file list also asks for copies of the three scripts, which step 2 is
  right to refuse — they live in `scripts/`.
- **Step 6's cudf command does not clean**, so on a warm dir it reports 0 warnings in 0.18s
  whatever the code says. Measured that too. Task 2 step 4 compares its warm build against these
  counts, and both are 0, so that check cannot fail; treat it as a smoke test, not a guard.
- **`build-test.md`'s `case-inventory.sh` row still says `rust-only|cudf`.** The `gpu` shape makes
  it stale. Plan task 12 already owns that file.

#### For the next slice

- `cargo clean -p peacockdb-core` ran in both target dirs, so the first build of the next slice
  rebuilds the lib and every test binary. The FFI `OUT_DIR` survived, so `LD_LIBRARY_PATH` still
  resolves for the cudf shape.
- `compare-inventory.sh` still accepts only `rust-only` and `cudf` as its shape argument. Nothing
  in the plan compares a `gpu` inventory, so it was left alone.
- `compare-inventory.sh` keeps the test-module name in what it compares, so a case that changes
  rung (`tests` to `ffi_tests`) reads as drift by design, as does a case leaving an integration
  target for `--lib`. It proves that a slice moved nothing; the leaf-name union across shapes is
  what proves that a slice moved cases without losing one.

### 2026-09-10 — slice 2 done: the `gpu` feature and the mutually-exclusive guard

Plan task 2, steps 1-6. Two source files changed — `peacockdb-core/Cargo.toml` (`gpu = []`) and
`peacockdb-core/src/lib.rs` (the `compile_error!`, three lines above `pub mod common;`) — plus one
new baseline, `llm-wiki/tasks/test-layout-baselines/inv-gpu.txt`. No test moved and no test file
was touched. Not committed.

#### The guard was watched go red, and watched fail to fire first

`cargo build --features "gpu rust-only" -p peacockdb-core` was run three times, and the middle run
is the one that matters:

1. Before the feature existed: `error: the package 'peacockdb-core' does not contain this feature:
   gpu`, exit 101. That is cargo refusing an unknown name, not the guard.
2. After `gpu = []` and **before** the `compile_error!`: `Finished dev profile in 16.95s`, exit 0.
   The contradictory pair builds happily on its own — so the guard is the only thing that rejects
   it, and it had a real failure to catch.
3. After the guard: `error: gpu needs the FFI linked; rust-only removes it. Pass one or neither.`
   at `peacockdb-core/src/lib.rs:10:1`, then `could not compile peacockdb-core (lib) due to 1
   previous error`, exit 101. The message is the guard's own text, not a link failure.

#### The three shapes, all cold for `peacockdb-core`

`cargo clean -p peacockdb-core` first in each target dir, so these are real measurements and not
slice 1's 0.18s warm no-op.

| Shape | Command | Result | `^warning` |
|---|---|---|---|
| `rust-only` | `cargo build --features rust-only -p peacockdb-core` | ok, 16.9s (1599 files cleaned first) | 0 |
| default | `scripts/cargo-cudf.sh build -p peacockdb-core` | ok, 19.1s (541 files cleaned first) | 0 |
| `gpu` | `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu` | ok, 18.9s | 0 |

The first two match slice 1's recorded 0 and 0. The `gpu` build needed no clean: a different
feature set is a different fingerprint, so it recompiled the crate anyway — which also means the
default and `gpu` shapes evict each other's `peacockdb-core` artifacts inside the one
`target-cudf-rapids-cuda-12.2` dir. That is the script's design, not thrash to diagnose, but it is
why a `gpu` inventory costs a crate rebuild every time it follows a default-shape one.

#### Nothing moved, in either shape that already existed

Both fresh inventories are **byte-identical** to slice 1's baselines, not merely `identical` under
the normaliser:

- `scripts/case-inventory.sh rust-only` — 4m06s, 1089 lines, `diff` clean.
  `scripts/compare-inventory.sh rust-only <baseline> <fresh>` → `case inventory (rust-only):
  identical`, exit 0.
- `CUDF_ROOT=... scripts/case-inventory.sh cudf` — 5m31s, 1157 lines, `diff` clean.
  `compare-inventory.sh cudf` → `case inventory (cudf): identical`, exit 0.
- `goldens.sha256` and `visibility.txt` re-taken and `diff`ed against the baselines: both identical.
  The 170 goldens and all 578 visibility records are where slice 1 left them.

#### The `gpu` baseline now exists

`CUDF_ROOT=... scripts/case-inventory.sh gpu` — 53s, exit 0, stored as
`llm-wiki/tasks/test-layout-baselines/inv-gpu.txt` (sha256 `b44f705f…279cfc`). **435 `--lib` cases**,
`--lib` only by the script's design. Its `--lib` section is byte-identical to the `--lib` section of
both `inv-cudf.txt` and `inv-rust-only.txt`, which is the expected answer today: `gpu = []` gates
nothing yet, so all three shapes see the same 435. Task 9 is where that number moves.

This was the first run of the `gpu` arm against a real feature. It behaved: the list came back 435
and not 0, so `LD_LIBRARY_PATH` resolved and the arm did not hit the empty-list failure mode
`build-test.md` warns about.

#### Findings for the slices after this one

- **`compare-inventory.sh` still rejects `gpu`.** `scripts/compare-inventory.sh gpu <base> <fresh>`
  prints `usage: compare-inventory.sh rust-only|cudf …` and exits 1. Verified, not assumed. There is
  now a `gpu` baseline that the comparison tool cannot read, so the slice that first needs to prove
  the `gpu` shape moved nothing — task 9 — has to add the arm or `diff` by hand. Left alone here
  because task 2's six steps do not include it.
- **rustfmt was not run on `lib.rs`.** It is the crate root, and rustfmt follows `mod` declarations
  into every file below it (`coding-style.md`). The two added lines were fed to `rustfmt --edition
  2024` on their own instead and came back unchanged.
- `Cargo.lock` did not change; adding an empty feature to a workspace member does not touch it.
- Both target dirs are now warm and hold every test binary for their shape, so the next slice's
  first build is incremental.

### 2026-09-10 — slice 3 done: `src/test_support/`, the feature, and the one testdata root

Plan task 3, steps 1-9. Not committed. Five files created, five deleted, thirteen modified:

- Created `peacockdb-core/src/test_support/{mod.rs,testdata.rs,golden_text.rs,registry.rs,result_text.rs}`.
- Deleted `peacockdb-core/tests/common/{mode,memory_limit,golden_text,registry,result_text}.rs`.
- Modified `peacockdb-core/Cargo.toml`, `src/lib.rs`, `tests/common/mod.rs`, the five `src` files
  that held the seven `CARGO_MANIFEST_DIR` sites, `Cargo.lock`, `scripts/build-test.sh`,
  `llm-wiki/build-test.md`, `llm-wiki/coding-style.md`.

#### The layout the visibility rules force, which is not the one the plan's file list implies

`test_support` is a component, so `a_components_api_is_declared_in_its_mod_rs` and
`nothing_re_exports_with_pub_use` apply to it: a `pub` item may only appear in `mod.rs`, and a
`pub use` facade is refused. A `pub(crate)` item in `test_support/golden_text.rs` is invisible to
the binaries outside the crate, so the four moved files could not simply land as `pub` modules.
What the shape has to be, and is:

- **Every type that crosses out is declared in `mod.rs`** — `MemoryLimit`, `Mode`, `MODES`,
  `NodeLine`, `RunNode`, `RegistryEntry`, `CorpusDeclaration`, `CsvRow`, `ResultDigest` — with its
  inherent `impl` beside it, which is what `coding-style.md` says a facade does instead of `pub use`.
- **Every function that crosses out is a one-expression delegate in `mod.rs`** to a `pub(crate)`
  body in a private sibling. A private module keeps its parent's private fields in reach, so
  `result_text.rs` still constructs `super::ResultDigest { schema, rows }`.
- `mode.rs` and `memory_limit.rs` have no body file left: both were nothing but types, so they are
  `mod.rs` in full. `mode.rs`'s `pub use peacockdb_core::planner::SMALL_TABLE_BYTES` is gone — a
  `pub use` the layout test forbids in `src/`, and nothing outside `mode.rs` used it.

`mod.rs` is 394 lines, which the 1000-line rule exempts anyway. The red-watch that this is real: a bare
`pub fn` appended to `test_support/registry.rs` makes `a_components_api_is_declared_in_its_mod_rs`
fail naming `test_support/registry.rs:391`, and it passes again once removed. The new component is
inside the guard's reach, not outside it.

#### `result_text.rs` had to move too, and the spec says it stays

Spec line 185 and plan step 3 both say `result_text.rs` stays because "only binaries that stay read
them". That is not true of `assert_results_match`, which plan step 3 *does* move: its exact arm is
`result_text::results_agree` / `first_difference`, and splitting the file would fork `formatters`,
`render_row`, `row_digests` and `columns_of`. So the whole file moved and `assert_results_match`'s
body moved into it. Its four remaining external items — `ResultDigest`, `digest_of`,
`rendered_rows`, `exceeds_rendered_size` — are declared in `mod.rs` for `corpus.rs` and
`test_golden_format`, which stay. **The page is wrong, not the move**: a helper cannot stay behind
when the only thing that calls it leaves.

`RESULT_GOLDEN_MAX_BYTES`, `batches_to_sorted_str`, `assert_sorted_str_approx`, `canonical_root`,
`canonical_data_dir`, `GpuResultMode` and `gpu_result_mode` stayed in `tests/common/mod.rs`. Several
of them have two audiences too, but the plan's step 3 list does not name them and "a helper moves
when a moving target needs it, and not before".

#### `tests/common/mod.rs` delegates through five inline modules

`pub mod mode { pub use peacockdb_core::test_support::{…}; }` and four like it, so no other test
file changed: a suite still writes `common::mode::MODES`. `tests/` is not read by the layout test
(`sources()` is `src/` only), so `pub use` is legal there. The module attribute became
`#![allow(dead_code, unused_imports)]` — `unused_imports` because a re-export no suite in a given
binary names is exactly the same situation the existing `dead_code` allow was for.

#### `inventory` had to become an optional real dependency

`inventory::collect!(RegistryEntry)` moved into the library, and a dev-dependency is not in the
library's dependency set. So `inventory = { version = "0.3", optional = true }` under
`[dependencies]` and `test-support = ["dep:inventory"]`; it stays a dev-dependency too, because the
test binaries call `inventory::submit!`. Cross-crate collection works — collect in the lib, submit
in the binary — proved by `test_cpu_corpus`'s `the_registry_matches_the_cpu_corpus_in_both_directions`
passing, whose CSV→inventory half fails on an empty inventory. `Cargo.lock` gained one line,
`peacockdb-core` under its own dependency list.

#### The feature is off in a plain build, proved in both directions with one probe

`pub fn probe_test_support() -> PathBuf { crate::test_support::testdata_root() }` appended to
`lib.rs`, then two commands on that same tree:

| Command | Result |
|---|---|
| `cargo build --features rust-only -p peacockdb-core` | `error[E0433]: failed to resolve: could not find test_support in the crate root`, exit 101 |
| `cargo test --features rust-only -p peacockdb-core --lib --no-run` | `Finished test profile`, exit 0 |

Same source, opposite answers: the self dev-dependency is what turns the feature on, and only for
`cargo test`. Run once early and once on the final tree; the probe was reverted both times.
`grep -rn 'test-support\|test_support' .github/workflows/` → **no hits, exit 1**, and every
`--features` line in `pipeline.yml` passes `rust-only` and nothing else. `cargo build --features
rust-only -p peacockdb` (the binary crate, which depends on the library without dev-dependencies)
builds clean.

#### The override, and the two things the plan's step 8 gets wrong

The negative control came first, before any source change: `PEACOCK_TESTDATA_DIR=/tmp/nonexistent
cargo test --features rust-only -p peacockdb-core --lib memory_estimation` → **11 passed**. The
variable was being ignored, which is what closed the loop had to fix.

After the sweep, the same command → **11 failed**, and over the whole lib **66 of 435 fail** with a
bogus root and **435/435 pass** without it. Two corrections to the step:

- **`--lib estimator` matches no test at all** — `0 passed; 435 filtered out`. A filter that selects
  nothing reports `ok`, so the step as written passes whatever the code does. The name is
  `memory_estimation`.
- **No failure names `/tmp/nonexistent`.** `grep -c` over the whole run is 0: the panic is
  DataFusion's `IoError(Os { code: 2, kind: NotFound })` from `register_tables_for`, which does not
  carry the path. The 66-fail / 0-fail pair is the evidence, not the message.

The strongest evidence is elsewhere and was not planned. **This worktree has no `tpch.sf1` or
`tpcds.sf1`** — they are generated, untracked, and only the primary checkout has them — so
`test_plan_goldens` fails 13 of 19 here for want of data, before and after this change alike.
Pointing `PEACOCK_TESTDATA_DIR` at `/tmp/peacock-testdata-slice3`, a directory of symlinks holding
this worktree's goldens and queries plus the primary checkout's two parquet trees, turns that into
**19/19** and `test_cpu_end_to_end` into **24 passed, 2 ignored**. An integration binary reading a
root at a path it was never built against, and every golden matching, is what the variable is for.
The composed root is outside the repo and the worktree stayed clean; the next slice can reuse it.

#### The seven sites, and what a grep for an eighth finds

All seven now read `crate::test_support::testdata_minimal_dir()` — the helper the same step moves,
rather than the plan's literal `testdata_root().join("tpch.minimal")` repeated seven times, so
"tpch.minimal" is spelled once. `git grep -n 'CARGO_MANIFEST_DIR' -- peacockdb-core/src` → **no
hits**. Four `use std::path::PathBuf;` lines went unused and were removed.

`crate::test_support::testdata_root()` is the only testdata root **in `peacockdb-core`**. The
remaining `CARGO_MANIFEST_DIR` readers there resolve the repo root to read committed *source*, which
#49 exempts by name: `test_ci_coverage`, `test_module_layout`, `test_golden_format`'s
`rust_sources`, `test_plan_goldens`'s wiki reader, and both `build.rs`.

**But the grep finds four more outside it**, and they are not in #49's enumerated residual:
`cost-report/src/main.rs` lines 1974, 2004, 2127 and 2240 each build a testdata path from
`env!("CARGO_MANIFEST_DIR")` inside a `#[cfg(test)]` fn — the two-row registry fixture, the whole
`../testdata` tree for the PR-comment size check, and `goldens/tpch.sf1`. Closing them means
`cost-report` dev-depending on `peacockdb-core` with `test-support`, which is a new dependency and
outside this task. The argument for leaving them: #49 exists because a binary is built on one host
and run on another, and `cost-report`'s tests run where the source is — they are never staged.
**Whoever retires #49 should decide about these four rather than inherit them silently.**

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `scripts/case-inventory.sh rust-only` | 1089 lines, **byte-identical** to `inv-rust-only.txt`; `compare-inventory.sh` → `identical` |
| `CUDF_ROOT=… scripts/case-inventory.sh cudf` | 1157 lines, **byte-identical** to `inv-cudf.txt`; `compare-inventory.sh` → `identical` |
| `CUDF_ROOT=… scripts/case-inventory.sh gpu` | 438 lines, **byte-identical** to `inv-gpu.txt` (hand `diff`) |
| `sha256sum` over `testdata/goldens` | identical, 170 files |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings, all 19 targets + lib |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu`, cold | 0 warnings |

Both inventories were taken twice — once before the doc-comment trim below, once after — and both
runs are byte-identical to the baselines. No case moved, in any shape.

CPU suites, all with the composed root: `--lib` 435, `test_module_layout` 11, `test_golden_format`
26, `test_ci_coverage` 7, `test_plan_goldens` 19, `test_cpu_end_to_end` 24 (2 ignored),
`test_corpus_goldens` 20, `test_cost_model` 3, `test_cpu_executors` 1, `test_layout_injection` 4,
`test_null_analysis` 8, `test_planner_join_capability` 13, `test_planner_join_refusals` 10, and
`test_cpu_corpus` sampled at one query across all five modes plus its four meta cases, 9. All ok, 0
failed. No device suite was run: nothing on this slice touches a device path.

The visibility dump grows by 61 records and loses none: 34 top-level bare `pub`, 8 `impl pub`, 19
`pub(crate)`. **Bare `pub` excluding `mod` goes 249 → 290, and excluding `test_support` it is still
exactly 249** — the ladder's production number did not move, which is the answer this slice should
give. `pub mod` goes 15 → 16 for the new component; it is a component in `lib.rs`, not a
`PUB_MODULES` exemption, so the 9 → 0 register is untouched.

#### Documentation the change falsified, fixed here

- `llm-wiki/coding-style.md`: `mode_named` is in `src/test_support/mod.rs`, not `tests/common/mode.rs`.
- `scripts/build-test.sh` (three comments) and `llm-wiki/build-test.md` (lines 118 and 443): "the CPU
  test crates bake the testdata path at compile time (#49)" is no longer why verda needs a
  `/media/data/peacockdb` symlink. Every binary honours the variable now; the reason is that
  `build-test.sh`'s **plain cpu branch does not set it** (`TESTDATA_ENV=":"`) while `--gpu` and
  `--rust-only` do. Setting it there is one line and is now correct, but it cannot be proved without
  a remote run, so it is **left as a follow-up** rather than done blind. `llm-wiki/build-test.md` is
  plan task 12's file; only the two false sentences were touched.
- `llm-wiki/tickets.md` #49 still describes the five `src` files as unfixed. Left alone deliberately:
  plan step 9 hands the retirement to the completeness pass. **#49's residual as that ticket
  enumerates it is closed** — modulo the four `cost-report` sites above.

#### Smaller things the next slice should know

- The doc blocks of `registry.rs` (23 lines) and `result_text.rs` (11) were already over
  `coding-style.md`'s 10-line cap before the move and travelled verbatim; a note added to each was
  removed again so the diff adds no line to an over-cap block. `test_support/mod.rs`'s own doc is
  exactly 10. Three in-body comments in `registry.rs` and one in `tests/common/mod.rs` are over the
  4-line cap and are likewise pre-existing.
- `rustfmt --edition 2024 peacockdb-core/src/test_support/mod.rs` is safe even though it is a
  `mod.rs`: every file it reaches through a `mod` declaration is one this slice created. `lib.rs` was
  **not** formatted — it is the crate root and reaches everything — and neither was
  `tests/common/mod.rs`: rustfmt on a copy of it wanted to reformat `canonical_root` and
  `assert_sorted_str_approx`, which predate the installed rustfmt, while leaving every line this
  slice added untouched. The five `src` files with the converted sites were not formatted either;
  their diffs are one expression each.
- Both target directories were `cargo clean -p peacockdb-core`ed for the cold warning counts and then
  fully rebuilt by the inventories, so they are warm again for all three shapes.
- `compare-inventory.sh` still rejects `gpu`, as slice 2 found. The `gpu` comparison in the table
  above is a hand `diff`. Task 9 is the slice that has to fix this or keep diffing by hand.

### 2026-09-10 — slice 4 done: the layout rules, and the tree that obeys them

Plan task 4, steps 1-10. Not committed. Eleven files created, two moved, 33 code files and
three wiki pages modified. The
five new assertions live in `peacockdb-core/tests/test_module_layout.rs`, which goes 1211 →
1874 lines.

#### The five rules, and what each printed when it fired

Every one was made to fail by construction and the violation reverted; the assertion names are
the plan's, and the plan's "rule 1 / rule 2" labels are the two directions in the other order.

| Violation | Assertion that fired | What it printed |
|---|---|---|
| `mod tests` renamed `mod gpu_tests`, gate left `#[cfg(test)]` | `a_test_module_is_named_for_its_rung` (and `a_rung_gate_implies_its_module_name`) | ``executor/row_range.rs:16: `mod gpu_tests` is gated #[cfg(test)] and its rung requires #[cfg(all(test, feature = "gpu"))]`` |
| `#[cfg(all(test, feature = "gpu"))]` above `mod tests` | `a_rung_gate_implies_its_module_name` (and the first) | ``executor/row_range.rs:16: #[cfg(all(test, feature = "gpu"))] sits on `mod tests` and belongs on gpu_tests`` |
| `#[cfg(test)] const X: u8 = 0;` in `planner/mod.rs` | `cfg_test_appears_only_on_a_test_module` | ``planner/mod.rs:145: #[cfg(test)] on `const X: u8 = 0` — test code in a production file`` |
| `planner/helpers/mod.rs` holding a `#[test]` | `a_test_only_path_carries_test` | `planner/helpers/mod.rs` |
| `#[cfg(test)] mod tests { }` appended to `planner/nulls.rs` | `a_test_module_lives_in_its_own_file` | `planner/nulls.rs:228: mod tests` |

The register is checked four ways, each watched separately on `plan::state_for`:

- doc stops naming the caller → ``plan/mod.rs:233: `state_for` is kept for planner/translator/schema_tests.rs, and its doc comment does not name it``
- entry's item renamed → ``plan/mod.rs: `states_for` is registered as test-only and no longer carries a `#[cfg(test)]`; drop the entry`` (plus the item itself reported as unregistered)
- `called_by` names a file that does not call it → ``plan/mod.rs: `state_for` names planner/translator/tests.rs as its caller and that file no longer calls it``
- `called_by` names a file that is gone → ``… and that file is gone``

Before the fix the three content rules were red and the two rung rules passed vacuously, which
is what the plan predicted: there is no `ffi_tests` or `gpu_tests` module yet. The first run
named 4 item-level gates in `partitioned.rs`, 10 inline `mod tests` blocks and 12 unnamed
test-only paths.

One reader bug was caught by its own near-miss fixture rather than by the tree:
`declared_item` stripped `const ` as a qualifier and as a kind, so `pub const X: usize = 1 <<
20;` came back nameless and would have been reported as an unregistered carve-out under a
blank name. Fixed to strip `const fn` only.

#### Step 3 found nineteen item-level gates, not four, and they part three ways

**Moved** — `driver/partitioned.rs`'s four (`hops`, `release_all`, `queue_len`, `last_call`).
They read three private fields of `Driver`, so `driver/tests/` cannot hold them: they went to a
new `partitioned/tests.rs` as an inherent `impl`, whose `pub(crate)` methods stay reachable
from `driver/tests/` because a method's visibility does not depend on the module the `impl` is
written in. The plan says "moves into the `mod tests` that uses it"; that module could not see
the fields, and widening them was the alternative.

**Deleted** — `translator::translate`, twelve lines. `planner::translate` now calls
`translator::Translator::new(..).translate(plan)` directly. `translator::translate_expr` stays:
`expr` is a private module of `translator` and `planner/mod.rs` cannot name it.

**Registered, eight entries in `TEST_ONLY_ITEMS`** — `cpu_backend::physical_expr`,
`executor::physical_expr`, `plan::state_for`, `planner::translate`, `planner::translate_expr`,
`translator::translate_expr`, and the two the plan calls neither: `accumulate.rs`'s
`compactions` and `join.rs`'s `makes_a_finish_pass`. Plus six `#[cfg(test)] use` lines in
`planner/mod.rs` and `planner/translator/mod.rs`, which the rule accepts in a file that holds
an entry, as the spec's "gated `use` lines … count as part of the declaration they serve".

Three notes on the register:

- **`has_finish_pass` is task 6's, not this slice's.** Plan task 6 step 5 declares the two-hop
  delegation and renames `CpuJoin::makes_a_finish_pass` with it, in the slice that raises the
  `cpu_backend` wall; task 4's mention is a forward reference. Doing it here would have added a
  second `#[cfg(test)]` item rather than removing one — `wire/tests.rs` reaches `CpuJoin`
  through the live `PUB_MODULES` exemption today, so the mod.rs hop buys nothing until that
  exemption goes. `coding-style.md` already states the rename; it is still true, just later.
- **`compactions` stays where it is**, which is the decision the plan asks to be recorded. Its
  caller is `cpu_backend/tests/accumulate.rs`, inside the same component, so a `cpu_backend/mod.rs`
  entry point would cross no boundary; and the counter is a private field of `accumulate`, so
  no test module can read it.
- **The register is not a `mod.rs`-only carve-out.** Two of the eight sit in implementation
  files because they read private state, and one sits in a subcomponent's `mod.rs`. The rule is
  "registered, live, and documented", not "in a component's mod.rs" — the narrower rule would
  have had to be weakened the moment it met `accumulate.rs`.

Every doc comment on a registered item now names its caller by path, and the test reads the
comment. `planner::translate`'s claimed "three of them, in two other components": it has two
callers, `plan_text/tests.rs` and `planner/memory_estimation/tests.rs`.

#### Steps 6-8: ten inline modules, not thirteen, and two renames

`git grep -ln '#\[cfg(test)\]' -- peacockdb-core/src | xargs grep -ln 'mod tests {'` finds
**ten**, not the plan's thirteen — task 2 moved and deleted the rest. Each `foo.rs` became
`foo.rs` + `foo/tests.rs`: `executor/forwarder` (24 lines), `executor/row_range` (39),
`plan/aggregate` (57), `plan/layout` (77), `plan/validate` (409), `plan_text/expr_text` (85),
`planner/memory_estimation` (283), `planner/translator/expr` (262),
`planner/translator/scan_mapping/parquet_meta` (170), `.../partition` (126). No import changed:
a `foo/tests.rs` is still a child of `foo`, so `use super::*` resolves as before.

`driver/mock.rs` → `driver/tests/mock.rs` and `driver/plans.rs` → `driver/tests/plans.rs`. This
one is not free: `driver/index/tests.rs` and `driver/single_partition/tests.rs` drive the same
mock backend from a sibling subcomponent, so the two modules are `pub(crate) mod` in
`driver/tests/mod.rs` and those two files now say `super::super::tests::{mock,plans}`. Ten
files under `driver/tests/` lost one `super::`.

`TEST_DIRS` is `["tests", "ffi_tests", "gpu_tests"]` (step 5), before any module moves.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package) | **1037 passed, 0 failed, 2 ignored**, exit 0 — 20 test binaries |
| `cargo test --features rust-only -p peacockdb-core --lib` | 435 passed, unchanged |
| `cargo test … --test test_module_layout` | 16 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings, 20 executables |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu`, cold | 0 warnings |
| `sha256sum` over `testdata/goldens` | identical, 170 files — checked before and after the suite run |
| `rustfmt --edition 2024 --check` on every file touched | clean |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, slice 3's composed root;
it still exists and still works. No device suite: nothing here touches a device path.

#### Inventories — the one target whose leaf-name set legitimately grows

| Shape | Result |
|---|---|
| `rust-only` | 1094 lines vs 1089. `compare-inventory.sh` → `DRIFTED`, and the whole diff is `test_module_layout` 11 → 16 |
| `cudf` | 1162 vs 1157, the same five and nothing else |
| `gpu` | 438 lines, **byte-identical** to `inv-gpu.txt` (hand `diff`; the tool still rejects `gpu`) |

The five names task 12's arithmetic must subtract:
`a_rung_gate_implies_its_module_name`, `a_test_module_is_named_for_its_rung`,
`a_test_module_lives_in_its_own_file`, `a_test_only_path_carries_test`,
`cfg_test_appears_only_on_a_test_module`.

**No `--lib` case read as drift**, which is the thing this slice could most easily have broken:
splitting `plan/validate.rs` moves 409 lines of cases from `plan::validate::tests::` to the
same path under a different file, and the normaliser ignores it by design (slice 1, step 3).
The baselines are unchanged on disk; the fresh inventories are in `/tmp` only.

#### The ladder

- Bare `pub` excluding `mod`: **290**, and **249 excluding `test_support`** — the production
  number did not move, which is right: this slice moved test code and deleted one
  `pub(crate)` fn.
- `pub mod`: **16**, unchanged. `PUB_MODULES`: **9 entries**, unchanged.
  `CROSS_COMPONENT_REACHES`: 1. `TEST_ONLY_ITEMS`: 8, new.
- Visibility dump 639 → **640**: `translator::translate` out (−1), `pub(crate) mod mock` and
  `pub(crate) mod plans` in (+2). Nothing else changed, verified by comparing the declaration
  column against slice 1's `visibility.txt`.
- `#[cfg(test)]` occurrences in `src/`: 50 → **46**: 25 test-module declarations, 8 registered
  items, 6 gated `use` lines serving them, and 7 inside doc comments and prose.

#### Documentation the change falsified, fixed here

- `llm-wiki/coding-style.md`: the carve-out bullet said a test-only entry point may sit in a
  component's `mod.rs` "and nowhere else in `src/`", and that its doc comment "is the only
  register there is". Both are now false — two registered items are private-state readers in
  implementation files, and `TEST_ONLY_ITEMS` is a register that checks the comment. Rewritten
  to state the two cases and name the register.
- `llm-wiki/build-test.md` line 39, the module-layout row: N was 11 and is 16, and the row
  named two registers where there are three. The five new rules are named in one sentence.
  Task 12 still owns the page's arithmetic.
- `llm-wiki/architecture.md` needed nothing: its one mention of `partitioned.rs` is about what
  the file owns, which did not change.

#### For the slices after this one

- **`partitioned/tests.rs` holds no `#[test]`.** It is a test module in the rung sense — gated,
  named, and in a `test` path — whose whole content is an inherent `impl`. A slice that assumes
  a `tests.rs` contains cases will mis-count it.
- **`driver/tests/{mock,plans}.rs` are `pub(crate) mod`**, unlike every other module under a
  `tests/` directory. Two subcomponent test modules depend on that; demoting them is an
  `E0603` on four lines.
- **The rung rules have never been satisfied by a real `ffi_tests` or `gpu_tests` module** —
  they have only been watched red on a renamed `tests`. Task 5 is the first slice that makes
  one pass for a real reason.
- `compare-inventory.sh` still rejects `gpu`; unchanged from slices 2 and 3.
- `planner/translator/expr.rs:176` fails `rustfmt --check` and did so at HEAD too — the
  installed rustfmt disagrees with the one the line was written under. Left alone.

### 2026-09-10 — slice 5 done: `test_gpu_batch` → `executor/ffi_tests/`, and the middle rung

Plan task 5, steps 1-6. Not committed. One file created, one deleted, four modified:

- Created `peacockdb-core/src/executor/ffi_tests/mod.rs` — the moved file, whole, minus its
  `#![cfg(not(feature = "rust-only"))]` inner attribute, with `use peacockdb_core::executor::{Batch,
  GpuBatch}` becoming `use super::{Batch, GpuBatch}` (same component, so `super::` and not `crate::`).
- Deleted `peacockdb-core/tests/test_gpu_batch.rs`.
- Modified `peacockdb-core/src/executor/mod.rs` (the declaration), `.github/workflows/pipeline.yml`
  (four hunks), `llm-wiki/build-test.md` (two falsified sentences).

#### The rung, watched in both directions

The filtered list was run **before** the move as the negative control, in the same shape and with
the same command:

| When | `scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests:: --list` |
|---|---|
| before the move | `0 tests, 0 benchmarks` |
| after | `3 tests, 0 benchmarks`, every one `executor::ffi_tests::…` |

Named, not counted: `a_batch_reports_what_it_was_built_with`,
`consume_yields_the_pair_the_ffi_call_needs`,
`dropping_a_detached_batch_releases_against_nothing`. They run, too —
`… --lib -- ffi_tests::` is `3 passed; 0 failed; 435 filtered out`.

The lower rung: `cargo test --features rust-only -p peacockdb-core --lib -- ffi_tests:: --list` →
`0 tests, 0 benchmarks`, exit 0, after a **successful compile** (`Compiling peacockdb-core` then
`Finished`), which is the half of step 3 that matters — a link error would prove the gate wrong in
the other direction. The rust-only `--lib` total is still 435; the cudf one is 438.

#### The rung rules pass for a real reason, and were watched failing for one

Slice 4 could only red-watch them on a renamed `mod tests`. Both now fire on the real module:
setting its gate to plain `#[cfg(test)]` makes `a_test_module_is_named_for_its_rung` and
`a_rung_gate_implies_its_module_name` fail, each naming `executor/mod.rs:32` and printing the gate
the rung requires. Reverted; `test_module_layout` is 16 passed.

#### Inventories — the three leaves moved, and nothing else did

| Shape | Lines | Against the baseline |
|---|---|---|
| `rust-only` | 1092 vs 1089 | `test_module_layout` 11 → 16, and the `test_gpu_batch 0 tests` summary line gone with the binary. No case either way |
| `cudf` | 1159 vs 1157 | the same five, plus the three leaves leaving `== test_gpu_batch` and arriving under `== --lib` as `ffi_tests::<leaf>` |
| `gpu` | 441 vs 438 | the same three, 435 → 438 (hand `diff`; the tool still rejects `gpu`) |

The `gpu` shape gaining them is the ladder working: `gpu` ⊃ ffi, so a device build compiles the
middle rung too. "Nowhere else" is about targets, and no other target moved.

Set equality was computed rather than read off the diff — leaf names extracted from both
inventories and compared as multisets: **no leaf lost in either shape**, and the only gains are
slice 4's five layout cases. Ten leaf names appear twice in the cudf inventory and three in
rust-only; both figures are unchanged from the baselines (the cpu/device halves of the executor
contract), so nothing this slice did duplicated a case.

#### The CI swap, and how it was checked without running anything

Four hunks in `pipeline.yml`, all in `dataset-matrix`:

1. the prebuild at :276 — `--test test_gpu_batch` out, `--lib` in, so the line still names every
   target the run step invokes;
2. the run step — `cargo test -p peacockdb-core --test test_gpu_batch` becomes
   `cargo test -p peacockdb-core --lib -- ffi_tests::`, same job, same feature shape;
3. the comment above the rust-only prebuild, which claimed `--lib` rides there *rather than* on the
   default-feature line. It now rides on both, as two different binaries, and the comment says so;
4. the gpu job's target list comment, which named `test_gpu_batch` as `test_gpu_abi`'s companion.

Checked three ways, none of them by reading: the file parses as YAML (7 jobs); `bash -n` over all
33 rendered `run:` blocks is clean; and both rewritten cargo commands were run locally in the cudf
shape. The second is worth keeping: the prebuild and the run step produced the **same lib
executable hash** (`peacockdb_core-738b5b595d84f9c4`), which is what the build/run split rests on —
a fingerprint mismatch would make the run step recompile silently.

`test_ci_coverage` is 7 passed after the edits. Its sweep enumerates targets that exist, so a
deleted target needs no exemption; the new line is asserted by task 11, not here.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package) | **1037 passed, 0 failed, 2 ignored**, exit 0 — 20 result lines, one fewer binary |
| `cargo test … --test test_module_layout` | 16 passed |
| `cargo test … --test test_ci_coverage` | 7 passed |
| `scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::` | 3 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu`, cold | 0 warnings |
| `sha256sum` over `testdata/goldens` | identical, 170 files — before and after the suite |
| `rustfmt --edition 2024 --check` on the new file | clean |

The package suite ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, slice 3's composed
root, at `--test-threads=2`. No device suite: nothing here touches a device path.

#### The ladder did not move, and could not have

- Bare `pub` excluding `mod`: **290**, **249 excluding `test_support`**. `pub mod`: **16**.
  `PUB_MODULES`: **9**, `CROSS_COMPONENT_REACHES`: 1, `TEST_ONLY_ITEMS`: 8. Visibility dump 640
  records. Every one unchanged from slice 4.
- The plan calls this target "3 cases, 2 items", but the two items — `Batch` and `GpuBatch` in
  `executor/mod.rs` — are still named from outside the crate by `test_gpu_abi`, `test_gpu_executors`
  and `test_layout_injection`, which move in tasks 6 and 9. Nothing could be demoted here, and no
  step asked for it. This slice is the shape proof; task 6 is the first that moves a number.

#### For the slices after this one

- **rustfmt reorders `mod` declarations alphabetically inside a contiguous group.** `mod tests;`
  followed directly by `mod ffi_tests;` is a rustfmt diff; a blank line between them makes two
  groups and keeps the ladder's reading order. Checked with a probe, since rustfmt on
  `executor/mod.rs` itself would reformat every file below it.
- `ffi_tests/mod.rs` is a directory with one file in it, as the plan's file list asks. Nothing
  forces the directory today — `ffi_tests.rs` would pass every rule — but the next ffi case has
  somewhere to land.
- `compare-inventory.sh` still rejects `gpu`; unchanged from slices 2, 3 and 4. The `gpu` row above
  is a hand `diff`, and that is now three slices in a row.
- The baselines on disk are untouched; the fresh inventories are `/tmp/inv-{rust-only,cudf,gpu}.txt`.
- `llm-wiki/build-test.md` keeps its stale `Runs` column and its arithmetic for task 12; only the
  two sentences this move falsified were touched — the GpuBatch row's example link, which pointed at
  a deleted file, and the dataset-matrix step list.

### 2026-09-10 — slice 6 dispatched, and the dispatch died at 19:00

The previous coordinator dispatched plan task 6 after committing slice 5 (`ec17a1d8`, 18:22).
The developer worked until 19:00:04 — the newest mtimes in the tree — and the whole session hit
the account's usage limit, which reset at 19:20. No slice 6 entry was written and nothing was
committed; the partial tree is what the dispatch left.

What the tree holds, from `git status` and `git diff --stat` only (21 files modified, 4 deleted,
`src/tests/` and `src/plan/tests/layout_injection.rs` untracked):

- `src/tests/{mod.rs,injection.rs,rebuild.rs,end_to_end.rs}` created; `tests/common/{injection,rebuild}.rs`,
  `tests/test_layout_injection.rs` and `tests/test_cpu_end_to_end.rs` deleted. **`end_to_end` came
  forward from plan task 10** because `test_cpu_end_to_end.rs` consumed `common::injection` — it
  could not stay behind once the trio left the crate boundary. `join_fixture.rs` stayed in
  `tests/common/`: its only consumers are the two planner targets task 7 moves.
- `cpu_backend/{mod,join,source,backend}.rs`, `executor/mod.rs`, `wire/tests.rs` touched — the
  `has_finish_pass` delegation and the wall (plan step 5-6), unproven.
- `tests/test_module_layout.rs` (44 lines), `pipeline.yml` (26), `build-test.md` (9),
  `compare-inventory.sh` (6), `test_support/{mod,result_text}.rs` touched.

None of it has been built or measured since. The re-dispatch starts from this tree rather than a
reset, on the developer's judgement once it compiles.

verda at re-dispatch: reachable but reprovisioned — host key changed, and after re-keying the new
host refuses the key (`Permission denied (publickey)`). Local runs for this slice.

### 2026-09-10 — slice 6 done: the injector trio, `test_layout_injection` and `test_cpu_end_to_end` into `src/`

Plan task 6, steps 1-8, plus task 10's steps 1-3. Not committed. Resumed from the tree the
19:00 dispatch left; nothing in it was reset. Five files created, four deleted, 21 modified:

- Created `peacockdb-core/src/tests/{mod.rs,injection.rs,rebuild.rs,end_to_end.rs}` and
  `peacockdb-core/src/plan/tests/layout_injection.rs`.
- Deleted `peacockdb-core/tests/common/{injection,rebuild}.rs`, `tests/test_layout_injection.rs`,
  `tests/test_cpu_end_to_end.rs`.
- Modified `src/lib.rs` (`#[cfg(test)] mod tests;`), `src/plan/tests/mod.rs`,
  `src/executor/mod.rs`, `src/executor/cpu_backend/{mod,join,source,backend}.rs`,
  `cpu_backend/tests/{backend,join}.rs`, `src/wire/tests.rs`, `src/test_support/{mod,result_text}.rs`,
  `tests/common/mod.rs`, `tests/test_module_layout.rs`, `.github/workflows/pipeline.yml`,
  `scripts/compare-inventory.sh`, `llm-wiki/{build-test,coding-style,tickets}.md`.

#### What was kept from the dead dispatch, what changed, what was added

The partial tree compiled and its `--lib` run was green on first contact, so it was kept whole.
Three decisions in it are the right ones and are recorded here as this slice's:

- **`test_cpu_end_to_end` came forward from plan task 10.** It was the fourth consumer of
  `common::injection` (step 1's grep: `test_layout_injection.rs`, `test_cpu_end_to_end.rs`,
  `test_planner_join_capability.rs`, `test_planner_join_refusals.rs`), and once the trio left the
  crate boundary it could not stay behind. Task 10's own steps 1-3 are therefore done here: both
  `#[ignore]` attributes and their `#182` messages travelled verbatim, and the run is 24 passed, 2
  ignored. Task 10 has nothing left but its ladder reading.
- **`join_fixture.rs` stayed in `tests/common/`.** Its only consumers are the two planner targets
  task 7 moves, and a file with no in-crate reader would be dead code under `src/tests/` until then.
- **`CpuSource`, `CpuJoin` and `CpuProbingJoin` are declared in `cpu_backend/mod.rs`**, with
  `join::Calls` `pub(crate)` for the field. Not a choice: `backend.rs` writes `type Source =
  CpuSource; type Join = CpuJoin;` in the `Backend` impl, and an associated type of a public trait
  impl must be nominally `pub` — `pub(crate)` on any of the three is `error[E0446]: crate-private
  type in public interface`, tried and reverted. A `pub struct` left inside a now-private `mod
  join` would satisfy rustc while escaping by inference through `<CpuBackend as Backend>::Join`,
  which is the case `no_public_signature_names_a_type_from_a_private_module` exists for, and
  `CpuExec` and `CpuUnload` already sit in `mod.rs` for the same reason. So the three types are
  the subcomponent's API and `source.rs`/`join.rs` are its implementation.
- `batches_to_sorted_str` moved from `tests/common/mod.rs` into `test_support/result_text.rs` with
  a `mod.rs` delegate: `end_to_end.rs` needed it from inside the crate and
  `assert_sorted_str_approx` still needs it from outside — the two-audience case the feature is
  for. One new bare `pub` in `test_support`, which the ladder excludes.

Changed by this dispatch: two `use` lines in `cpu_backend/mod.rs` were out of rustfmt's order
(`parquet` sorts before `physical_expr`) and were moved; three wiki sentences below. Added: every
measurement, and the red-watches.

#### `has_finish_pass`, two hops, and the register that grew by two

`cpu_backend/mod.rs` declares `pub(crate) fn has_finish_pass(node, build, probe, ctx) ->
Result<bool, PlanError>` as `CpuJoin::hash(..).map(|e| e.has_finish_pass())`; `executor/mod.rs`
declares the same signature delegating to it; `CpuJoin::makes_a_finish_pass` is
`has_finish_pass`. `wire/tests.rs` calls `crate::executor::has_finish_pass` and keeps both halves:
a refused cell is `Err`, an allowed cell's `bool` equals whether the recipe carries an `AtDone`
call. All three are `#[cfg(test)]`, so `TEST_ONLY_ITEMS` goes **8 → 10**: the `makes_a_finish_pass`
entry becomes `has_finish_pass` called by `cpu_backend/mod.rs`, and the two delegates are
registered with their callers — the same three-entry shape `physical_expr` already has. The `use
crate::executor::cpu_backend::join::CpuJoin` line and the comment naming the exemption are gone
from `wire/tests.rs`, so `CROSS_COMPONENT_REACHES` is **empty**; the register stays, as the spec
says, until task 4 of the chain deletes it.

#### The register, watched red in both directions on this tree

| Probe | What fired | What it printed |
|---|---|---|
| `executor/cpu_backend/join` put back in `PUB_MODULES`, forced by `injection.rs` | `every_pub_mod_exemption_is_still_forced_by_what_it_names` | `executor/cpu_backend/join names peacockdb-core/tests/common/injection.rs, which no longer exists` |
| the `wire/tests.rs` reach put back in `CROSS_COMPONENT_REACHES` | `only_the_parent_component_names_a_subcomponent` | `wire/tests.rs no longer names executor/cpu_backend, so the entry can go` |
| `TEST_ONLY_ITEMS` entry renamed back to `makes_a_finish_pass` | `cfg_test_appears_only_on_a_test_module` | ``join.rs: `makes_a_finish_pass` is registered as test-only and no longer carries a `#[cfg(test)]`; drop the entry`` (and `has_finish_pass` reported as unregistered) |
| `mod join;` → `pub mod join;` with no register entry | `pub_mod_declares_a_component_and_nothing_else` | ``executor/cpu_backend/mod.rs declares `pub mod join;` `` |

Each reverted; `test_module_layout` is 16 passed on the final tree.

#### CI

`pipeline.yml`: the `Layout injection mechanism (Rust)` step is deleted, `--test
test_cpu_end_to_end` is out of the rust-only prebuild and the run step, and the comment above
the rust-only `--lib` line now says it needs sf1 because the end-to-end tier rides in it. `grep
test_layout_injection\|test_cpu_end_to_end` over the file → no hits. Checked mechanically: the
file parses as YAML (7 jobs), and `bash -n` over all 32 rendered `run:` blocks (33 in slice 5,
one step fewer) is clean. `test_ci_coverage` is 7 passed; a deleted target needs no exemption.

#### Inventories — the 30 leaves moved, and the two summary lines that went

Set equality computed over leaf names (last `::` segment), per shape and over the union of the
three, as sets and as multisets; the baselines on disk are untouched and the fresh files are
`/tmp/inv6-{rust-only,cudf,gpu}.txt`.

| Shape | Lines | Leaves lost | Leaves gained | Duplicated leaves |
|---|---|---|---|---|
| `rust-only` | 1086 vs 1089 | none | slice 4's five layout cases | 3, unchanged |
| `cudf` | 1153 vs 1157 | none | the same five | 10, unchanged |
| `gpu` (`--lib` only) | 471 vs 438 | none | 33: the 30 below plus slice 5's three `ffi_tests` — the ladder, `gpu` ⊃ `rust-only` | 3, unchanged |
| union of three | 1092 distinct vs 1087 | none | the same five | 10, unchanged |

Per-leaf target movement, `rust-only`: 26 `test_cpu_end_to_end → --lib` (as
`tests::end_to_end::<leaf>`), 4 `test_layout_injection → --lib` (as
`plan::tests::layout_injection::<leaf>`), nothing else. `--lib` is 435 → **465** in `rust-only`
and 438 → **468** in `cudf` and `gpu`. The two summary lines that disappeared with their binaries:
`test_cpu_end_to_end  26 tests, 0 benchmarks` and `test_layout_injection  4 tests, 0 benchmarks`.
`compare-inventory.sh` reports `DRIFTED` for all three, as it must when cases change target.

`compare-inventory.sh` now accepts `gpu`: the dead dispatch added the arm (two lines and a
comment), and this is the first slice whose `gpu` row is the tool's answer rather than a hand
`diff`. Its `rc=1` above is the `DRIFTED` verdict, not the usage error slices 2-5 recorded.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package, `--test-threads=2`) | **1037 passed, 0 failed, 2 ignored**, exit 0 — 18 result lines, two fewer binaries than slice 5 |
| `… --lib` | 463 passed, 2 ignored (435 + 4 + 26) |
| `… --lib -- tests::end_to_end` | 24 passed, 2 ignored, both `#[ignore]` against #182 |
| `… --lib -- plan::tests::layout_injection` | 4 passed |
| `… --test test_module_layout` | 16 passed |
| `… --test test_ci_coverage` | 7 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu` | 0 warnings |
| `sha256sum` over `testdata/goldens` | identical, 170 files — before and after the suite |
| `rustfmt --edition 2024 --check` on the twelve touched leaf files | clean |
| the `mod.rs` files (`cpu_backend`, `executor`, `plan/tests`, `test_support`) | clean against empty stub children, so the `mod` chain was not followed into pre-existing files; `src/tests/mod.rs` clean directly, every child being this slice's |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, slice 3's composed root,
which still exists. No device suite: nothing here touches a device path. verda refused the key,
so everything ran locally.

#### The ladder — the first slice that moves it

- Bare `pub` excluding `mod`: **290 → 283**, and **249 → 241 excluding `test_support`**. Eight
  `impl pub fn` in `join.rs` and `source.rs` demoted to `pub(crate)`; one `pub fn` gained in
  `test_support/mod.rs`.
- `pub mod`: **16 → 14**. `PUB_MODULES`: **9 → 7**. `CROSS_COMPONENT_REACHES`: **1 → 0**.
  `TEST_ONLY_ITEMS`: **8 → 10**.
- Visibility dump 640 → **684**, +44 net: 39 `pub(crate)` items from `injection.rs` and
  `rebuild.rs`, now inside `src/` where the dump reads; `pub(crate) mod injection` and `pub(crate)
  mod rebuild` (the two planner targets task 7 moves reach them as `crate::tests::…`);
  `join::Calls`; the two `has_finish_pass` delegates; `batches_to_sorted_str` twice; minus the two
  `pub mod`. The three struct declarations moved files without changing count.
- `#[cfg(test)]` occurrences in `src/`: 46 → **51**: one more test-module declaration (`lib.rs`),
  two registered delegates, two mentions inside their doc comments.

#### Where a measurement contradicts the spec or plan

- **The spec's "108 → 79" for this slice is not what the ladder measures.** The production count
  went 249 → 241: the trio forced `pub mod` on `join` and `source`, whose *items* are now
  `pub(crate)`, but the three executor types stay `pub` in `mod.rs` (E0446 above) and every other
  `cpu_backend` item is still forced by `test_cpu_executors.rs`. The bulk of the 108 the spec
  counted are items in `plan/mod.rs` and `executor/mod.rs` that the trio named through a
  component wall that stays — moving the caller in-crate does not demote them, and by the spec's
  own accounting those are task 4's. Recorded, not bent.
- **Plan step 6's "converting their items to `pub(crate)` — the compiler names every consumer"
  is true of the functions and false of the three types**, for the E0446 reason.
- **The `TEST_ONLY_ITEMS` register grows here rather than shrinking**, 8 → 10, which the plan's
  step 5 does not say: two delegates are two more `#[cfg(test)]` items outside a test path.
- `end_to_end.rs` is **1052 lines**, over `coding-style.md`'s 1000. It was 1057 as
  `test_cpu_end_to_end.rs` and moved verbatim (rustfmt shortened it); the layout test carries no
  length rule, so nothing went red. Left for the reviewer to decide: splitting is not a move.

#### Documentation the change falsified, fixed here

- `llm-wiki/build-test.md`: the End-to-end and Layout-injection rows point at the new paths, the
  dataset-matrix step list no longer names the two targets, and the layout-rules row no longer
  says "the one cross-component reach". The `Runs` column and the arithmetic are task 12's.
- `llm-wiki/coding-style.md`: "Nine more exist under `executor/`" is seven, and
  `CROSS_COMPONENT_REACHES` is noted empty. `makes_a_finish_pass` stays in the Names section as
  the example of what the predicate rule forbids, which is still true.
- `llm-wiki/tickets.md` #182: `boundary()` is in `src/tests/end_to_end.rs`. The #176 line naming
  a `--test test_cpu_end_to_end` step is history of a CI failure and stays.

#### For the slices after this one

- Task 7's two planner targets are now the only consumers of `join_fixture.rs`, and they reach the
  injector as `crate::tests::injection` once inside. `src/tests/mod.rs` declares `injection` and
  `rebuild` `pub(crate) mod` for exactly that; `end_to_end` is private.
- Task 8 (`test_cpu_executors`) is now the sole forcer of all three remaining `cpu_backend`
  entries; when it moves, `cpu_backend`, `accumulate` and `emit` demote together and the register
  drops to the four `gpu_backend` entries.
- Task 10 has only its step 3 left: the ladder reading and the check that the only `pub` items a
  test crate still forces are the eight `corpus.rs`/`corpus_gpu.rs` names.
- rustfmt rewrote `injected_queries!(tpch/nested_loop_join, …)` as `tpch / nested_loop_join`
  inside `end_to_end.rs` — same tokens, the macro's `$dataset:ident / $query:ident` arm is
  unchanged. `--check` now holds on the file; a `#[rustfmt::skip]` would restore the reading.
- Both target dirs were `cargo clean -p peacockdb-core`ed for the cold counts and then rebuilt
  by the package run (`./target`) and the `gpu` build (`target-cudf-*`, which now holds the `gpu`
  fingerprint, not the default one).

### 2026-09-10 — slice 7 done: the planner four → `planner/tests/`, and the fixture with them

Plan task 7, steps 1-5. Not committed. Six files created, five deleted, eight modified:

- Created `peacockdb-core/src/planner/tests/{mod,null_analysis,join_capability,join_refusals,plan_goldens}.rs`
  and `peacockdb-core/src/tests/join_fixture.rs`.
- Deleted `peacockdb-core/tests/{test_null_analysis,test_planner_join_capability,test_planner_join_refusals,test_plan_goldens}.rs`
  and `tests/common/join_fixture.rs`.
- Modified `src/planner/mod.rs` (`#[cfg(test)] mod tests;`), `src/tests/mod.rs` (`pub(crate) mod
  join_fixture;`), `tests/common/mod.rs`, `src/test_support/registry.rs` and
  `tests/test_golden_format.rs` (one comment each naming the old path), `.github/workflows/pipeline.yml`,
  `llm-wiki/{build-test,tickets}.md`.

#### One target at a time, and the negative control first

`cargo test --features rust-only -p peacockdb-core --lib -- planner::tests --list` before any move:
`0 tests, 0 benchmarks`, after a successful compile. Then in the plan's order, each followed by
`--lib -- planner::tests` and the whole `--lib` at `--test-threads=2`:

| After | `planner::tests` | whole `--lib` |
|---|---|---|
| `null_analysis` | 8 passed | 471 passed, 2 ignored |
| `join_capability` + `join_refusals` (+ the fixture) | 31 passed | 494 passed, 2 ignored |
| `plan_goldens` | 50 passed | 513 passed, 2 ignored |

All four are `mod` under `planner/tests/mod.rs`, plain `#[cfg(test)]` at the component — pure Rust,
no rung gate. `null_analysis.rs` reaches `can_be_null` as `super::super::can_be_null` (a child of
`planner::tests`, so one `super::` is the test module, not the component); the other three name
`crate::plan`, `crate::planner`, `crate::plan_text`, `crate::wire`, `crate::test_support` and
`crate::tests::join_fixture`. `planner/memory_estimation/tests.rs` and `planner/translator/{tests,schema_tests}.rs`
are untouched and still run — they are inside the 513.

#### `join_fixture.rs`, and the one `tests/common` helper that moved with the goldens

`git grep -n join_fixture` before the move: `tests/common/mod.rs:13` (the declaration) and the two
planner targets, nothing else — slice 6's note holds. It is `src/tests/join_fixture.rs`,
`pub(crate) mod` beside `injection` and `rebuild`, every `pub` in it `pub(crate)`; `use
peacockdb_core::` → `use crate::`. Red-watched: one `pub const LANES` put back makes
`a_components_api_is_declared_in_its_mod_rs` fail naming `tests/join_fixture.rs:29`, reverted.

`canonical_root`, `point_canonical_root` and `canonical_data_dir` left `tests/common/mod.rs` too:
`test_plan_goldens` was their only caller (`git grep canonical_` finds `corpus_golden.rs` mentioning
the name in a comment and nothing else). They are private fns in `planner/tests/plan_goldens.rs`
rather than `test_support` items — one audience, so the feature is not the place — and the in-body
comment that said "two test binaries use this path" was cut to the atomic-rename reason, which is the
part still true. `tests/common/mod.rs`'s now-unused `use std::path::PathBuf` went with them.
`RESULT_GOLDEN_MAX_BYTES`, `assert_sorted_str_approx`, `GpuResultMode` and `gpu_result_mode` stay:
only the corpus binaries read them. Nothing new went to `test_support`.

#### `test_plan_goldens` verified and did not write

`UPDATE_CANONICAL` was unset for every run (`echo ${UPDATE_CANONICAL-unset}` → `unset`). The goldens
digest — `find testdata/goldens -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum | diff -
llm-wiki/tasks/test-layout-baselines/goldens.sha256` — is empty before the first move, after the
first `planner::tests` run that included the goldens, and after the final package run: 170 files,
byte-identical. `git status testdata` is empty.

The wiki reader (`every_refusal_names_a_ticket_that_exists`) still reads
`env!("CARGO_MANIFEST_DIR")/../llm-wiki`. Inside the crate the macro expands to the same
`peacockdb-core/` it did in the integration binary — both are compiled from the same package
manifest — so `../llm-wiki` still resolves to the checkout's wiki. It reads committed source, not
testdata, which is what #49 exempts by name; `testdata_root()` is untouched by it, and the ticket's
sentence naming the reader now names `planner/tests/plan_goldens.rs`.

#### The register: nothing to edit

No `PUB_MODULES` entry names any of the four files or `join_fixture.rs`: all seven remaining entries
are forced by `test_cpu_executors.rs` (three) and `test_gpu_executors{,/*}.rs` (four), as slice 6
left them. `test_module_layout` is 16 passed on the final tree; `test_ci_coverage` 7 passed after
the workflow edit.

#### CI, and the scripts

`pipeline.yml`: the three 25.02-leg steps (`Planner join capability`, `Planner join refusals`,
`Null analysis rules`) are deleted with their comments, and the block comment above them now
describes the three steps that remain (`test_cost_model`, `test_golden_format`,
`test_corpus_goldens`). The prebuild line `cargo test --no-run -p peacockdb-core --test
test_plan_goldens --lib` is `… --lib`; the run step's `cargo test -p peacockdb-core --test
test_plan_goldens` and its "plan tier first" comment are gone, and the comment on the rust-only
`--lib` line now says the plan goldens are why it needs sf1. Checked mechanically: the file parses
as YAML (7 jobs) and `bash -n` over all 29 rendered `run:` blocks is clean (32 in slice 6, three
steps fewer). `git grep` for the four names over the whole tree outside `llm-wiki/tasks` and
`llm-wiki/archive` → no hits.

`scripts/build-test.sh`, `build-test-shadgpu.sh`, `scripts/lib/shadgpu-env.sh`: `git grep` for the
four names → **no hits**, so nothing there ever named them as `--test` targets and #176's shape does
not arise. Neither script was touched.

#### Inventories — 50 leaves moved, four summary lines gone

Fresh files in `/tmp/inv7-{rust-only,cudf,gpu}.txt`; baselines on disk untouched. Leaf-name sets
(last `::` segment) compared per shape and over the union, as sets and multisets:

| Shape | Lines | `--lib` | Leaves lost | Leaves gained | Duplicated leaves |
|---|---|---|---|---|---|
| `rust-only` | 1074 vs 1089 | 435 → 515 | none | slice 4's five layout cases | 3, unchanged |
| `cudf` | 1141 vs 1157 | 438 → 518 | none | the same five | 10, unchanged |
| `gpu` | 521 vs 438 | 435 → 518 | none | 83: the 80 moved so far plus slice 5's three `ffi_tests` — the ladder | 3, unchanged |
| union of three | 1092 distinct vs 1087 | none | the same five | 10, unchanged |

Per-leaf target movement this slice, identical in `rust-only` and `cudf`: 8 `test_null_analysis →
--lib` (`planner::tests::null_analysis::<leaf>`), 13 `test_planner_join_capability → --lib`
(`planner::tests::join_capability::`), 10 `test_planner_join_refusals → --lib`
(`planner::tests::join_refusals::`), 19 `test_plan_goldens → --lib` (`planner::tests::plan_goldens::`).
Slices 5 and 6's 33 read the same as before. The four summary lines that disappeared with their
binaries: `test_null_analysis 8 tests, 0 benchmarks`, `test_plan_goldens 19 tests, 0 benchmarks`,
`test_planner_join_capability 13 tests, 0 benchmarks`, `test_planner_join_refusals 10 tests, 0
benchmarks`. `compare-inventory.sh` says `DRIFTED` for all three shapes, as it must.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package, `--test-threads=2`) | **1037 passed, 0 failed, 2 ignored**, exit 0 — 13 test executables, four fewer than slice 6 |
| `… --lib` | 513 passed, 2 ignored (463 + 50) |
| `… --lib -- planner::tests` | 50 passed |
| `… --test test_module_layout` | 16 passed |
| `… --test test_ci_coverage` | 7 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings, 13 executables |
| `cargo build --features rust-only -p peacockdb` (the CLI) | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu` | 0 warnings |
| `sha256sum` over `testdata/goldens` | identical, 170 files — before, between and after |
| `rustfmt --edition 2024 --check` on the five moved leaves and `registry.rs` | clean (`null_analysis`, `plan_goldens` and `join_fixture` were formatted; the two join files came back clean as they were) |
| `planner/tests/mod.rs`, `src/tests/mod.rs` | clean directly — every child is slice 6's or this slice's |
| `planner/mod.rs`, `tests/test_golden_format.rs` | clean against stub children |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, slice 3's composed root, which
still exists. No device suite: nothing here touches a device path. verda refused the key, so
everything ran locally.

`plan_goldens.rs` is 958 lines after rustfmt (960 as `test_plan_goldens.rs`, plus 44 lines of
`canonical_root` and its two companions, minus what rustfmt and the header lost) — under the
1000-line rule.

#### The ladder did not move, and the demotion that was tried

- Bare `pub` excluding `mod`: **283**, **241 excluding `test_support`**. `pub mod`: **14**.
  `PUB_MODULES`: **7**. `CROSS_COMPONENT_REACHES`: **0**. `TEST_ONLY_ITEMS`: **10**. All unchanged
  from slice 6.
- Visibility dump 684 → **696**: exactly `join_fixture.rs`'s eleven `pub(crate)` items and its
  `pub(crate) mod` line, verified by `diff` against a dump of HEAD's tree — nothing else changed.
- `#[cfg(test)]` occurrences in `src/` (`git grep -c`, summed): 52 → **53**, the one declaration in
  `planner/mod.rs`.

Seven items are now `pub` with no consumer outside the crate — `plan_text::{render_plan,
render_plan_memory}`, `wire::{Payloads, check_seq_kinds, depth, render_plan_recipes}` and
`executor::post_order_of_every_node` — found by grepping `tests/`, `peacockdb/src`,
`cost-report/src` and `peacockdb-ffi` for every name the four files imported. **All seven were
demoted to `pub(crate)` and reverted**: `cargo build --features rust-only -p peacockdb-core` then
emits **36 `dead_code` warnings** (the seven, plus everything under them in `plan_text/` and
`wire/recipes.rs` that only they reach), because their only callers are the plan-goldens test and
the CLI does not render plans. That is `coding-style.md`'s honest signal — a production item with
no shipping caller — and the answer it names is a caller, deletion or an `#[allow]` at the site,
none of which is a move. The other 29 names the four files imported are still named by
`test_cpu_executors`, `test_gpu_*`, the corpus binaries or the CLI, or sit in a `pub` field or
enum variant (`KeyDistribution` in `PartitionLayout`, `GpuUnion` in `NodeRef`), so nothing this
slice could bring down without a warning. Recorded for `visibility.md`.

#### Where a measurement contradicts the spec or plan

- **The plan's Interfaces line says this slice consumes `crate::tests::{injection, rebuild,
  join_fixture}`.** It consumes only `join_fixture`: none of the four files named
  `common::injection` or `common::rebuild` — slice 6's step-1 grep listed the two planner targets as
  injector consumers, and that was `join_fixture`'s directory, not the injector. Nothing in
  `planner/tests/` reaches `injection` or `rebuild`.
- **The spec's "50 with the two executor tiers, 15 with three more"** is still not what the ladder
  measures: 241 → 241 here, for the dead-code reason above. The spec's counting attributed these
  items to the targets that named them; the compiler attributes them to the production code that
  does not.
- `build-test.md`'s `Runs` column and the table's arithmetic are task 12's and were left; only the
  rows whose Examples link pointed at a deleted path (the planner-capability, null-analysis,
  join-refusals and nine plan-goldens rows), the two golden-producer cells and the flow diagram
  naming `test_plan_goldens`, the day-to-day-loop example command, and the dataset-matrix step list
  were changed. The `#L427`/`#L229` anchors are `#L708`/`#L265` in the new file.

#### For the slices after this one

- Task 8 (`test_cpu_executors`) is now the sole forcer of the three `cpu_backend` entries, as slice
  6 said; nothing here changed that.
- `tests/common/mod.rs` is 172 lines and keeps only what the two corpus binaries and
  `test_golden_format` read: the five delegating inline modules, `RESULT_GOLDEN_MAX_BYTES`,
  `assert_sorted_str_approx`, `GpuResultMode`, `gpu_result_mode`. Nothing a later slice moves reads
  it any more.
- `canonical_root` lives in `planner/tests/plan_goldens.rs`; a device test that ever needs the fixed
  `/tmp/peacock-plan-bytes-root` symlink has to reach it from there or move it to `src/tests/`.
- The `gpu` inventory (`--lib` only) now reads 518 and will read the device set on top of it once
  task 9 lands; `compare-inventory.sh gpu` works since slice 6.
- Both target dirs were `cargo clean -p peacockdb-core`ed for the cold counts and then rebuilt by
  the inventories and the package run; `target-cudf-*` holds the `gpu` fingerprint last.

### 2026-09-10 — the chain rebased over master's fourteen documentation commits

The control file said `rebase` after slice 7. Master had moved by fourteen commits since
`f6dcde07`, every one touching only `llm-wiki/**`: five new task specs and plans, the
hacks-audit report, tickets #198-#200, an Antipatterns section in `build-test.md`, one reviewer
bullet in `prompts.md`, and tasks 7-11 appended to this chain's board section.

`ENS-rmm-pool-budget` (task 3) rebased clean, `960add37` → `e49a2c04`, twelve commits replayed, no
conflict; PR #144 still targets master with twelve commits. `ENS-test-layout` then rebased onto it
with `--onto`, ten commits, no conflict; the diff between the old and new tips is master's
documentation and nothing else. Per the protocol a rebase that carries documentation alone
re-verifies nothing: task 3 stays `done`, task 4 goes `rebase needed(building)` → `building` with
no re-run. Slice 8's whole-package run is the first build on the new base and will say if
`tickets.md`'s merge broke a ticket reference a golden names.

verda is still refusing the key. Both branches force-pushed with `--force-with-lease`.

### 2026-09-10 — slice 8 done: `test_cpu_executors` → `executor/cpu_backend/tests/`, and the `cpu_backend` group closed

Plan task 8, steps 1-4. Not committed. Two files created, two deleted, nineteen modified:

- Created `peacockdb-core/src/executor/cpu_backend/tests/contract.rs` (the moved case) and
  `peacockdb-core/src/tests/executor_cases.rs` (the shared table, from `tests/common/executor_cases.inc`).
- Deleted `peacockdb-core/tests/test_cpu_executors.rs` and `tests/common/executor_cases.inc`.
- Modified `src/executor/mod.rs`, `src/executor/cpu_backend/{mod,accumulate,emit,backend}.rs`,
  `cpu_backend/tests/{mod,accumulate,backend,emit}.rs`, `src/tests/{mod,injection}.rs`,
  `tests/test_gpu_executors.rs`, `tests/test_gpu_executors/contract.rs`, `tests/test_module_layout.rs`,
  `.github/workflows/pipeline.yml`, `llm-wiki/{build-test,coding-style,tickets}.md`.

The whole-package run on the clean rebased tree came first, since it was the first build on the new
base: **1037 passed, 0 failed, 2 ignored, exit 0, 0 warnings** — nothing was red before the move.

#### Step 1: the table is a module, `src/tests/executor_cases.rs`, and task 9's device half reads it as one

Neither of the two shapes the plan offered — the `.inc` staying in `tests/common/` and `include!`d
from `src/` by a `../../../../` path, or moving as text under `src/tests/` — was taken. The table is a
**real module**: `pub(crate) mod executor_cases;` in `src/tests/mod.rs`, its five items `pub(crate)`
(`INPUT`, `Shape`, `Shape::order_is_the_answer`, `Case`, `CASES`; the struct's fields stay `pub`,
which the layout test's `is_bare_pub_item` excludes as fields), and the cpu half reads it as `use
crate::tests::executor_cases::{CASES, INPUT, Shape}`. `src/tests/` is what the spec calls "the shared
test support the component tests reach through `crate::tests::…`", and a table both engines' tests
read is exactly that. Under the `.inc` shapes the two `include!`s would have outlived the move, and the
table's `pub` items would sit in a file no guard reads — `sources()` in the layout test and
`visibility-dump.py` both walk `*.rs` only, so an `.inc` under `src/` is invisible to both. Verified:
`a_test_only_path_carries_test` follows `mod` declarations and `#[test]` lines in `.rs` files, and
never an `include!`, so where the `.inc` lived was never its business either way.

**The gpu binary reaches it by relative path until task 9**: `tests/test_gpu_executors.rs:66` is
`include!("../src/tests/executor_cases.rs")`. Including a `.rs` as text is why the file's header is
plain `//` and not `//!` — an inner doc attribute in an `include!` landing after other items is a
compile error — and the header says so. **Task 9's device half must match this**: when
`test_gpu_executors` lands under `executor/gpu_backend/gpu_tests/`, the `include!` goes and
`contract.rs` there writes `use crate::tests::executor_cases::{…}` like the cpu half; the header can
become `//!` in that same slice. The cudf-shape compile of the binary is the proof the path resolves:
`scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_executors --no-run` → 0 warnings, exit 0,
and the cudf inventory lists its 31 cases as before.

`pub` → `pub(crate)` on the five items is what made it a module the layout test accepts (`pub`
outside a `mod.rs` is refused; `pub(crate)` is not). Inside the gpu binary the same text at the crate
root is `pub(crate)` in a binary crate, which is fine. The stale `gpu_cases.inc` in the header — a
file that no longer exists — became `corpus_cases.inc`, the instrument it was actually naming.

#### The move itself

`contract.rs` is `test_cpu_executors.rs` with `mod common` gone, `peacockdb_core::` → `crate::`, and
four helpers **deduplicated against `cpu_backend/tests/mod.rs`** rather than carried: the binary's
`Given::of(schema, BatchLayout::MultipleBatches)`, `columns(&[…])`, `rows()` and `ctx()` are
`mod.rs`'s `Given::of_schema(schema)`, `schema_of(&[…])`, `schema_of(&GROUPED)` and `ctx()` to the
token — the binary always passed `MultipleBatches`, and `GROUPED` is the same `(k Utf8, v Int64)` pair.
Carrying them would have put two `Given`s in one module tree with `use super::*` silently shadowing
one; the sibling files all take their helpers through that glob. Rung `tests`, no gate: the
declaration is `mod contract;` in the existing `tests/mod.rs`, which was not overwritten.

Negative control before the move: `cargo test --features rust-only -p peacockdb-core --lib --
cpu_backend::tests::contract --list` → `0 tests, 0 benchmarks` after a successful compile. After:
`-- cpu_backend::tests` → **65 passed** (64 + `contract::every_case_answers_what_the_contract_says`),
whole `--lib` **514 passed, 2 ignored** (513 + 1).

#### Step 3: the wall, and the two shapes rustc forced on the way up

Dropping the three entries and demoting the three `pub mod` (`cpu_backend` in `executor/mod.rs`,
`accumulate` and `emit` in `cpu_backend/mod.rs`) was watched in stages, each one red for a reason:

1. Entries dropped, nothing else: `every_pub_mod_exemption_is_still_forced_by_what_it_names` red,
   naming all three `… names peacockdb-core/tests/test_cpu_executors.rs, which no longer exists`.
2. The three `pub mod` → `mod`: `cargo build` clean (rustc reads nominal visibility, so `pub` items
   inside a private module are fine by it), and `a_components_api_is_declared_in_its_mod_rs` red
   naming **21 items** in `accumulate.rs` and `emit.rs` — the whole surface the exemption had covered.
3. Everything `pub(crate)`: **`E0446` on exactly three**, `type BatchAcc = CpuAccumulator`, `type
   PartAcc = CpuPartitionAccumulator`, `type Emitter = CpuEmitter` in `backend.rs` — slice 6's finding
   for `CpuSource`/`CpuJoin`, reproduced for the other three associated types. Plus one `dead_code`:
   `LimitStream::seen`, `pub` with no caller anywhere (`git grep 'seen()'` → nothing outside its own
   file), which the exemption had been hiding. **Deleted**, not `#[allow]`ed — its doc claimed a
   driver reads it and none does.
4. The three types declared in `cpu_backend/mod.rs` with their inherent `impl`s left in
   `accumulate.rs`/`emit.rs` as `pub(crate)` methods — slice 6's `CpuJoin` shape. `CpuEmitter` and
   `CpuPartitionAccumulator` have private fields and moved as they were. `CpuAccumulator` was an
   **enum**, and an enum's variant payloads are as public as the enum: with `Coalesce`, `SortedRuns`,
   `AggregateBatches` and `LimitStream` at `pub(crate)` rustc emits four `private_interfaces`
   warnings (`type Coalesce is more private than the item CpuAccumulator::Coalesce::0 … reachable at
   visibility pub`) — the "escapes by inference" case, seen by the compiler this time because
   `<CpuBackend as Backend>::BatchAcc` makes the enum reachable. So it is now `pub struct
   CpuAccumulator { state: accumulate::State }` over a `pub(crate) enum State` with the four variants:
   the same private-field-over-`pub(crate)`-type shape as `CpuJoin { calls: join::Calls }`. Three
   match sites changed (`accumulate.rs` twice, `backend.rs`'s `HeldBytes`, and
   `tests/accumulate.rs`'s `compactions_over`); `backend.rs` and the test module see the private
   field because both are descendants of `cpu_backend`.
5. **`src/tests/injection.rs` reached through the wall** — `use crate::executor::cpu_backend::…` for
   all eight executor types, `E0603` once `cpu_backend` is private, and slice 6 had not seen it
   because the exemption was live. The injector needs the *types* (it is a `Backend` whose associated
   types are the CPU backend's) but not the module: eight aliases at its top, `type CpuSource =
   <CpuBackend as Backend>::Source;` and so on, `CpuProbingJoin` through `<CpuJoin as
   JoinExecutor<CpuBackend>>::Probing`. That is the path production code names them by, no
   `pub(crate) mod`, no new entry point, `TEST_ONLY_ITEMS` untouched at 10.

`cpu_backend/tests/{accumulate,backend,emit}.rs` lost their `crate::executor::cpu_backend::accumulate::…`
imports — the types now arrive through the `use super::*` chain like everything else in the module.

Red-watches on the final tree, each reverted, `test_module_layout` **16 passed** after:

| Probe | What fired | What it printed |
|---|---|---|
| `executor/cpu_backend/emit` put back, forced by `tests/test_gpu_executors.rs` (a file that exists) | `every_pub_mod_exemption_is_still_forced_by_what_it_names` | `executor/cpu_backend/emit names peacockdb-core/tests/test_gpu_executors.rs, which no longer reaches peacockdb_core::executor::cpu_backend::emit` |
| `mod cpu_backend;` → `pub mod cpu_backend;` with no entry | `pub_mod_declares_a_component_and_nothing_else` | ``executor/mod.rs declares `pub mod cpu_backend;` `` |

The register has **four entries, all `gpu_backend`**. `CROSS_COMPONENT_REACHES` was already empty
(slice 6 deleted the entry the plan's step 3 mentions); nothing to delete here.

#### CI, scripts, and the one literal left

`pipeline.yml`: the rust-only prebuild's `--lib \ --test test_cpu_executors` is `--lib`, and the
run step's `--test test_cpu_executors` line and its four-line comment are gone (the contract rides in
the `--lib` line above it). `grep test_cpu_executors` over the file → no hits. Checked mechanically:
the file parses as YAML (7 jobs) and `bash -n` over all **29** rendered `run:` blocks is clean, same
count as slice 7 — the retired line was inside a block, not a step. `test_ci_coverage` is **7 passed**.

`scripts/build-test.sh`, `build-test-shadgpu.sh`, `scripts/lib/*.sh`: `git grep test_cpu_executors --
scripts .github` → **no hits**. Nothing ever named it as a `--test` literal, so #176's shape does not
arise. One hit remains in the tree: `tests/test_ci_coverage.rs:291`, a string fixture in the matcher's
own unit test (`--no-run … --lib --test test_cpu_executors`, asserting that `--no-run` is not a run).
It is never handed to cargo; left for task 11, which rewrites that file.

#### Inventories — one leaf moved, one summary line gone

Fresh files `/tmp/inv8-{rust-only,cudf,gpu}.txt`; baselines on disk untouched. Leaf-name sets (last
`::` segment) per shape and over the union, as sets and multisets:

| Shape | Lines | `--lib` | Leaves lost | Leaves gained | Duplicated leaves |
|---|---|---|---|---|---|
| `rust-only` | 1071 vs 1089 | 435 → 516 | none | slice 4's five | 3, unchanged |
| `cudf` | 1138 vs 1157 | 438 → 519 | none | the same five | 10, unchanged |
| `gpu` | 522 vs 438 | 435 → 519 | none | 84: the 81 moved so far plus slice 5's three `ffi_tests` | 3, unchanged |
| union of three | 1092 distinct vs 1087; multiset 1102 vs 1097 | | none | the same five | 10, unchanged |

The moved leaf: `every_case_answers_what_the_contract_says`, `test_cpu_executors` → `--lib` as
`executor::cpu_backend::tests::contract::every_case_answers_what_the_contract_says` in both shapes
(in `cudf` its target set goes `{test_cpu_executors, test_gpu_executors}` → `{--lib,
test_gpu_executors}`, the device half staying put — that is one of the ten cudf duplicates, and it
is still ten). The summary line that disappeared with the binary: `test_cpu_executors  1 test, 0
benchmarks`. `compare-inventory.sh` says `DRIFTED` for all three, as it must.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package, `--test-threads=2`), before any change | 1037 passed, 0 failed, 2 ignored, exit 0 |
| the same, on the final tree | **1037 passed, 0 failed, 2 ignored**, exit 0 — 12 result lines, one fewer binary than slice 7 |
| `… --lib` | 514 passed, 2 ignored |
| `… --lib -- cpu_backend::tests` | 65 passed |
| `… --test test_module_layout` | 16 passed |
| `… --test test_ci_coverage` | 7 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings, 12 executables (13 in slice 7) |
| `cargo build --features rust-only -p peacockdb` (the CLI) | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu` | 0 warnings |
| `scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_executors --no-run` | 0 warnings — the new `include!` path resolves |
| `sha256sum` over `testdata/goldens` | identical, 170 files — before and after |
| `rustfmt --edition 2024 --check` on the ten touched leaves | clean (`executor_cases.rs` was formatted: rustfmt folds `INPUT`'s six rows onto one line; the rest came back clean as they were) |
| `cpu_backend/mod.rs`, `executor/mod.rs`, `cpu_backend/tests/mod.rs`, `src/tests/mod.rs` | clean against stub children |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, which still exists. No device
suite: the only device path touched is an `include!` line, proved by compiling. verda refused the key,
so everything ran locally.

#### The ladder — the `cpu_backend` wall, and what it brought down

- Bare `pub` excluding `mod`: **283 → 265**, and **241 → 223 excluding `test_support`**. Eighteen
  down: thirteen `impl pub fn` in `accumulate.rs` (11) and `emit.rs` (2) to `pub(crate)`, four state
  structs (`Coalesce`, `SortedRuns`, `AggregateBatches`, `LimitStream`) to `pub(crate)`, and `seen`
  deleted. The three executor types moved files without changing count.
- `pub mod`: **14 → 11**. `PUB_MODULES`: **7 → 4**. `CROSS_COMPONENT_REACHES`: **0**.
  `TEST_ONLY_ITEMS`: **10**.
- Visibility dump 696 → **699**, `diff`ed against a dump of HEAD's tree: the eighteen above, plus
  `pub(crate) enum State`, `pub(crate) mod executor_cases` and the table's five `pub(crate)` items.
- `#[cfg(test)]` occurrences in `src/`: **53**, unchanged.
- **What still forces `pub`**: `CpuSource`, `CpuExec`, `CpuAccumulator`, `CpuPartitionAccumulator`,
  `CpuEmitter`, `CpuJoin`, `CpuProbingJoin`, `CpuUnload` in `cpu_backend/mod.rs` and every `pub fn`
  on `CpuExec`/`CpuUnload` there — the associated types of `impl Backend for CpuBackend` (E0446 at
  `pub(crate)`) and the methods an in-crate caller reaches through them. Nothing outside the crate
  names any of them now; they are `pub` because `Backend` and `CpuBackend` are, which is
  `visibility.md`'s subject.

#### Where a measurement contradicts the spec or plan

- **Plan step 3 says "delete the `CROSS_COMPONENT_REACHES` entry that named the reach you just
  removed"** — there was none to delete; slice 6 emptied it with `has_finish_pass`. Step 3 was
  written before slice 6 recorded that.
- **"Convert their items to `pub(crate)`" holds for the functions and not for the types**, as
  slice 6 found for `join`/`source` — and one step further here: a `pub(crate)` payload inside a
  `pub` enum is a warning, not an error, so the enum had to become a struct to keep the build at 0.
- **The plan's Interfaces line, "`executor_cases.inc`, which stays in `tests/common/` for now"**: it
  did not stay, for the reasons under step 1. The one-copy invariant it exists for holds.
- **The spec's "50 with the two executor tiers"**: the ladder reads 223 after this slice, not 50, for
  the reason slices 6 and 7 gave — the spec counted items by the target that named them, the
  compiler counts them by the production interface that keeps them reachable.

#### Documentation the change falsified, fixed here

- `llm-wiki/build-test.md`: the executor-contract row's two links point at the new paths, and the
  dataset-matrix step list no longer names `test_cpu_executors`. The `Runs` column and the arithmetic
  are task 12's.
- `llm-wiki/coding-style.md`: "Seven more exist under `executor/`" is four.
- `llm-wiki/tickets.md` #174: `executor_cases.inc` is `src/tests/executor_cases.rs`.
- `tests/test_module_layout.rs`'s own prose: "the nine `PUB_MODULES` entries" and "these nine modules"
  (stale since slice 6) no longer carry a number, and the exempt-module example is a `gpu_backend`
  file, since `cpu_backend/accumulate.rs` is exempt no more.
- `tests/test_gpu_executors/contract.rs:1` names `executor_cases.rs`.

#### For the slices after this one

- **Task 9**: `test_gpu_executors.rs:66` is `include!("../src/tests/executor_cases.rs")`; the device
  half in `gpu_backend/gpu_tests/contract.rs` should `use crate::tests::executor_cases::{…}` and drop
  the `include!`, after which the table's header can be `//!`. `gpu_backend/{accumulate,emit}.rs`
  will meet the same `private_interfaces` question if `GpuAccumulator` is an enum with `pub(crate)`
  payloads; the struct-over-`pub(crate)`-enum shape here is the answer that keeps warnings at 0.
- The `gpu` inventory (`--lib` only) reads 519 and will read the device set on top once task 9 lands.
- Task 11: the fixture string at `test_ci_coverage.rs:291` names `test_cpu_executors`; harmless, but
  a real target name would stop a grep finding a ghost.
- Both target dirs were `cargo clean -p peacockdb-core`ed for the cold counts and rebuilt by the
  inventories and the package run; `target-cudf-*` holds the `gpu` fingerprint last.

### 2026-09-10 — slice 9 dispatched, and the dispatch died at 22:21

The previous coordinator committed slice 8 (`4ee0ce29`, 21:57) and dispatched plan task 9, the
device four. The developer worked until 22:21:21 — the newest mtime, `scripts/build-test.sh` — and
the session hit the account's usage limit, which did not reset until the next morning. No slice 9
entry was written and nothing was committed; the partial tree is what the dispatch left, and it is
committed as-is so a rebase can run under it.

What the tree holds, from `git status`, `git diff --stat` and a few greps — not a reading of the
diff (17 files modified, 9 deleted, three `gpu_tests/` directories untracked):

- The four targets are gone from `tests/`: `test_gpu_abi.rs`, `test_gpu_executors.rs` and its five
  children, `test_gpu_recipe_walk.rs`, `test_inc2_conformance.rs` — 3,348 lines. Ten new files under
  `executor/gpu_backend/gpu_tests/` (`mod`, `abi`, `accumulate`, `backend`, `contract`, `exec`,
  `join`), `wire/gpu_tests/mod.rs` and `executor/cpu_backend/gpu_tests/{mod,murmur_conformance}.rs` —
  3,327 lines. All three declared `#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`.
- `executor/gpu_backend/mod.rs` and its four subcomponents are `mod`, not `pub mod`; `PUB_MODULES`
  in the layout test is `&[]`. `gpu_backend/{accumulate,backend,emit,join,mod}.rs` carry the
  visibility sweep (95, 20, 18, 39, 66 lines).
- `src/tests/executor_cases.rs` has its `//!` header back and no `include!` names it anywhere —
  the device half reads it as a module, as slice 8 asked.
- `pipeline.yml` (66 lines), `scripts/build-test.sh` (49), `scripts/build-test-shadgpu.sh` (29),
  `scripts/lib/shadgpu-env.sh` (46), `tests/test_ci_coverage.rs` (4), `tests/test_module_layout.rs`
  (33) touched — the staging, the filter argument and the list shrink, unproven.

None of it has been built or measured. The re-dispatch starts from this tree rather than a reset,
on the developer's judgement once it compiles; plan task 9's steps 2-9 are all still open.

### 2026-09-11 — the chain rebased again, over eight more documentation commits

The control file said `rebase` (written 22:19 on the 10th, while slice 9's dispatch was still
alive). Master had moved by eight commits since `edb66b21`, every one under `llm-wiki/**`: the
casts/wire-schema/empty-answers chain dropped and archived, `operator-harness` and `operator-cases`
specs and plans added as tasks 8-9 of this chain (the rest renumbered 10-13), a Scope rule in
`prompts.md`'s helper section, and `README.md`'s spec outline.

Slice 9's dead tree was committed first (`e1b45f2e` after the rebase) so the rebase had a clean
tree to run on. `ENS-rmm-pool-budget` rebased clean, `e49a2c04` → `29546ff6`, twelve commits, no
conflict; `ENS-test-layout` then rebased onto it with `--onto`, fourteen commits, no conflict; the
diff between the old and new tips is master's documentation and nothing else, and the board
merged by hunk without a conflict — task 3 `done`, task 4 this branch's state, tasks 8-13 master's.
Documentation alone re-verifies nothing: task 3 stays `done`, task 4 goes `rebase needed(building)`
→ `building` with no re-run. Slice 9's cudf-shape build is the first build on the new base.

### 2026-09-11 — slice 9 re-dispatched from the dead tree

Hosts at dispatch: **verda** unreachable (connection timed out, so local runs for the CPU shapes);
**shad-gpu** up, `llm-gpu0h200`, and the neighbour still holds 53 GiB of the 143.7 — the device
run needs the card, so `[rmm] pool … could not be built` is the neighbour and goes to #178, not
to a diagnosis. Cudf toolchain on this host: `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2`.

### 2026-09-11 — slice 9 done: the device four → `gpu_tests`, and both scripts

Plan task 9, steps 2-9 on the tree the dead dispatch left (`e1b45f2e`), plus its step 1 re-read
and verified. Not committed. One file created, sixteen modified on top of that commit:

- Created `scripts/lib/rung-args.sh` — `rung_args`, the one rule for what a staged rust binary
  is run with on a GPU host, inlined into both runners' remote scripts.
- Modified `scripts/build-test-shadgpu.sh`, `scripts/build-test.sh`, `scripts/cargo-cudf.sh`,
  `peacockdb-core/src/executor/gpu_backend/{emit.rs, gpu_tests/{mod,accumulate,backend,contract,join}.rs}`,
  `src/wire/gpu_tests/mod.rs` (rustfmt only, see below), `tests/test_ci_coverage.rs` (one doc
  clause), `cpp/tests/gpu/test_cudf.cpp` (one comment path), `llm-wiki/{architecture,build-test,
  coding-style,tickets}.md`.

#### What the dead dispatch left, read against the plan

The four targets are under `gpu_tests/` as the plan says — `test_gpu_recipe_walk` →
`wire/gpu_tests/mod.rs`, `test_gpu_executors` and its five children plus `test_gpu_abi` →
`executor/gpu_backend/gpu_tests/{mod,accumulate,backend,contract,exec,join,abi}.rs`,
`test_inc2_conformance` → `executor/cpu_backend/gpu_tests/murmur_conformance.rs` — each declared
`#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`. Diffed against the old files with `git diff
<old>:<path> <new>:<path>`: the moves are `peacockdb_core::` → `crate::`/`super::`, `common::` →
`crate::test_support::`, the `#![cfg]`/`#[macro_use] mod common` header gone, the `#[path]`
declarations gone, and in the murmur gate the seven per-item `#[cfg(not(feature = "rust-only"))]`
gates dropped. The `include!` slice 8 flagged is gone: `gpu_tests/mod.rs` reads `use
crate::tests::executor_cases::INPUT`, `contract.rs` reads `{CASES, Shape}`, and
`executor_cases.rs` has its `//!` header. Nothing in it was unsalvageable; what it had not done
was build, measure, or finish `build-test.sh`, which named the lib as `peacockdb-core:--lib` in
every mode's list and then handed that to `cargo test --test --lib`.

`gpu_backend/mod.rs` and its four subcomponents are `mod`; the five executor types
(`GpuAccumulator`, `GpuPartitionAccumulator`, `GpuEmitter`, `GpuJoin`, `GpuProbingJoin`) are
declared in `mod.rs` with their `impl`s left in the leaves — slice 8's shape, including
`GpuAccumulator` as a `pub struct { state: accumulate::State }` over a `pub(crate) enum State`,
which is what keeps the `private_interfaces` warning at zero. `LimitStream::seen` deleted on this
side too (no caller; the build is 0 warnings without it).

#### Step 2: the filter selects the device set and nothing else

`scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu -- gpu_tests:: --list` →
**55 tests**, every path `executor::cpu_backend::gpu_tests::murmur_conformance::` (10),
`executor::gpu_backend::gpu_tests::{abi 4, accumulate 10, backend 2, contract 1, exec 12, join 6}`
(35) or `wire::gpu_tests::` (10). The first attempt exited 127 — the cudf-shape binary needs
`LD_LIBRARY_PATH` (`build-test.md`), and `--list` is a run — so the listing was taken the way
`case-inventory.sh` takes it.

#### Steps 3-4: the lib binary, staged and given its rung

`scripts/lib/shadgpu-env.sh` (the dead dispatch's, kept): `stage_cargo_test_binary` and a new
`stage_cargo_lib_binary` over one `stage_cargo_binary <name> <kind> <staged> <dir> <cargo args>`
whose python matches `target.name` **and** `kind in target.kind` — the lib and its test binary
share a name and a kind and only the latter has an `executable`. `--build` stages
`cpp/install/rust-tests/{test_gpu_corpus, peacockdb_core_gpu_lib}` (1.17 GB unstripped),
`--push-binaries` printed `deleting rust-tests/{test_inc2_conformance, test_gpu_recipe_walk,
test_gpu_executors, test_gpu_abi}` on the host, so the mirror is exact.

**The rung argument.** The dead dispatch had the right three-way rule inline in the shad-gpu
gate — no filter: the lib gets `gpu_tests::`; filter set: an `--exact` list of the names `--list
gpu_tests::` reports that contain the filter, since libtest ORs its filters and `PCK_TEST_FILTER`
alone would have run CPU cases matching it on the device (`--list join` is 104 cases across the
rungs, the intersection is 8). `build-test.sh` needed the same rule, so it is one function,
`rung_args` in `scripts/lib/rung-args.sh`, and each runner's unquoted heredoc carries
`$RUNG_ARGS_FN` (the file's text) and one line: `mapfile -t args < <(rung_args "\$t"
<lib-name> <rung> <filter> [<env prefix>])`. An empty rung means a mode that runs its lib whole,
which is what the cpu modes of `build-test.sh` pass. Exercised locally against the staged
binaries before it went to the host: other binary/no filter → `['']`, other/`q6` → `[q6]`,
lib/no filter → `[gpu_tests::]`, lib/`abi` → `[--exact, '', <4 abi names>]`, lib/`zzz` →
`[--exact, '']` and the binary lists `0 tests` on it.

`pipeline.yml` (the dead dispatch's, kept): the staging step defines `resolve <name> <kind>` and
`stage <name> <path>`, stages `test_gpu_corpus` from the one-line `for t in` array and the lib as
`peacockdb_core_gpu_lib`, both `--features gpu`; the remote loop sets `rung=gpu_tests::` for that
one filename. CI has no `PCK_TEST_FILTER`, so the two-line form is right there and the guard
(`running 0 tests`) stays unconditional. Checked mechanically: **YAML parses, 7 jobs; `bash -n`
over all 29 rendered `run:` blocks clean**; and `resolve()` was cut out of the rendered staging
step and run against real `--message-format=json` output — `peacockdb_core lib` names the lib
test binary, `test_gpu_corpus test` names the corpus binary, `peacockdb_core test` names nothing.

#### Steps 5-6: `build-test.sh`, and the two derived suites read by hand

- The `GPUSET` heredoc is `peacockdb-core:test_gpu_corpus` alone; the murmur literal and the ten
  lines above it are gone. **The lib is not in `gpu_runtime_targets()`**, against the plan's
  wording: `test_ci_coverage::gpu_runtime_targets()` reads the heredoc and asserts every entry
  matches an on-disk `--test` target, and the function is also the membership test
  `rust_only_targets` runs per file, so a lib entry there would have been "the GPU-runtime set"
  containing the rust-only lib. Each mode appends `lib_target` itself.
- The entry is `peacockdb-core:<staged name>` like every other, and the staging loop maps that
  name to `--lib` (`sel=(--lib); name=peacockdb_core; kind=lib`) with the kind-aware resolver.
  The name is per shape — `peacockdb_core_gpu_lib`, `peacockdb_core_rust_only_lib`,
  `peacockdb_core_lib` — so a `--run` after another mode's `--build` finds no binary rather
  than the wrong one. `--gpu` builds with `CARGO_FEATURES="--features gpu"`, which the script
  had never passed.
- Derived suites, evaluated by extracting lines 254-370 with the mode variables preset:
  `--gpu` → `test_gpu_corpus, peacockdb_core_gpu_lib` (2, `LIB_RUNG=gpu_tests::`);
  `--rust-only` → `test_corpus_goldens, test_cost_model, test_cpu_corpus, test_golden_format,
  peacockdb_core_rust_only_lib` (5); default → those four plus `peacockdb-ffi:test_ffi` and
  `peacockdb_core_lib` (6). `needs_cmake_targets` yields `test_gpu_corpus` and `test_ffi`; minus
  the GPU set, one. None empty; the guard stays.
- The remote script rendered for `--gpu` and `--rust-only` (a copy with `ssh "$HOST" bash` →
  `cat`, `--host dummy --run`) is `bash -n` clean and its loop line reads `mapfile -t args < <(rung_args
  "$t" 'peacockdb_core_gpu_lib' 'gpu_tests::' q6\'s)`. The resolver was run for real once:
  `cargo test --no-run --features rust-only -p peacockdb-core --lib --message-format=json |
  python3 … peacockdb_core lib` → `target/debug/deps/peacockdb_core-4633673c43c91be2`.
- **Unproven**: no remote ran `build-test.sh` (verda unreachable). The lib entry has never
  executed under it in any mode.

#### Step 7: on the device — four runs, one of them a red-watch

Each phase its own foreground call under `timeout`. The first `--patch --run-detached` patched
and then died on `ssh: Could not resolve hostname llm-gpu0h200.velkerr.ru: Temporary failure in
name resolution` (rc 255, the link); `--run-detached` alone went through on the retry. No `[rmm]
pool … could not be built`: every C++ binary reported its pool against **87.4 GiB free**, the
neighbour notwithstanding.

| Run | Script | Lib binary | Corpus | Result |
|---|---|---|---|---|
| 1 (`20260911T101059`) | the dead dispatch's inline rule | **55 passed, 519 filtered out, 9.47 s** | 8 passed, 5.38 s | exit 0, 5 C++ + 2 rust |
| 2 (`…T101556`) | `rung_args` | 55 passed, 519 filtered out, 9.51 s | 8 passed | exit 0 |
| 3 (`…T101829`), `PCK_RUN_CPP=0 PCK_TEST_FILTER=abi` | `rung_args` | **4 passed, 570 filtered out**, all four `gpu_backend::gpu_tests::abi::` | `running 0 tests`, no banner (filter set) | exit 0 |
| red-watch (`…T101859`), a copy with `RUST_LIB_RUNG=nomatch_tests::` | `rung_args` | `running 0 tests`, 574 filtered out | 8 passed | **`!!! peacockdb_core_gpu_lib ran 0 tests … nothing was verified`, exit 1** |
| 4 (`…T103603`), the final tree rebuilt, pushed and patched | `rung_args` | 55 passed, 519 filtered out, 9.20 s | 8 passed, 5.32 s | exit 0 |

519 filtered out is the CPU rung (516) plus the FFI rung (3), which is the ladder read off the
binary. Run 3 is the intersection: a filter that names 4 device cases and would name none of the
CPU ones, on a binary holding all 574. The red-watch is the spec's backstop: the guard is armed
for the lib's rung because the rung never travels as `PCK_TEST_FILTER`. The banner now prints
the arguments the binary ran with rather than the filter, since for the lib they differ.

"Roughly what the five staged binaries took" cannot be compared here — no log of the old five is
on this host. What is measured: 55 cases in one process in 9.2 s, beside the corpus binary's 8
in 5.3 s, each process paying its own RMM pool.

#### Step 8: the wall, already down

`PUB_MODULES` is `&[]`; `CROSS_COMPONENT_REACHES` 0; `TEST_ONLY_ITEMS` 10. `test_module_layout`
**16 passed**. Red-watches on the final tree, each reverted:

| Probe | What fired | What it printed |
|---|---|---|
| `mod gpu_backend;` → `pub mod gpu_backend;` in `executor/mod.rs` | `pub_mod_declares_a_component_and_nothing_else` | ``executor/mod.rs declares `pub mod gpu_backend;` `` (15 passed, 1 failed) |
| `wire/mod.rs`'s `gpu_tests` gate → plain `#[cfg(test)]` | `a_test_module_is_named_for_its_rung` and `a_rung_gate_implies_its_module_name` | ``wire/mod.rs:28: `mod gpu_tests` is gated #[cfg(test)] and its rung requires #[cfg(all(test, feature = "gpu"))]`` (14 passed, 2 failed) |

#### Inventories — the device set moved, and the three ungated cases with it

Fresh files `/tmp/inv9-{rust-only,cudf,gpu}.txt`; baselines untouched. Leaf-name sets (last
`::` segment) per shape against the baselines, as sets and multisets:

| Shape | Lines | `--lib` | Leaves lost | Leaves gained | Duplicated leaves |
|---|---|---|---|---|---|
| `rust-only` | 1059 (1071 in slice 8) | 435 → 516, unchanged since slice 8 | **3**: `cpu_reference_2col_partition_ids_for_probe`, `pmod_handles_negative_hashes`, `step_i_comet_murmur3_public_api_compiles_and_runs` | slice 4's five | 3, unchanged |
| `cudf` | 1071 (1138) | 435 → 519 | **48** = the 55 device leaves minus the 7 whose names the cpu tier shares | the same five | 10 → 3: the 7 shared names are no longer pairs here |
| `gpu` | 577 (522) | 435 → 574 | none | 132 = the 84 of slices 5-8 plus 55, minus the 7 shared names already present | 3 → 10, **the cudf baseline's ten exactly** (7 cpu/gpu executor pairs, 3 driver pairs) |
| union of three | 1092 distinct vs 1087 | | **none** | the same five | |

The device set itself: the 55 leaves of `test_gpu_abi` + `test_gpu_executors` +
`test_gpu_recipe_walk` + `test_inc2_conformance` in the cudf baseline **equal** the 55 leaves
under `gpu_tests::` in the fresh `gpu` `--lib`, as sets. In the fresh `rust-only` and `cudf`
inventories the only device leaves present are those 7 shared names, and each is the
`cpu_backend::tests::` copy. The four `--test` summary blocks (`0 tests` under rust-only) are gone
from both CPU inventories, which is the 12-line drop. `compare-inventory.sh` says `DRIFTED` for
all three, as it must.

**The coverage change.** The murmur gate's three cases without a device gate —
`cpu_reference_2col_partition_ids_for_probe`, `pmod_handles_negative_hashes`,
`step_i_comet_murmur3_public_api_compiles_and_runs` — ran on dataset-matrix under both CPU shapes
until this slice. They are device-rung cases now, run on shad-gpu only: the whole-package
rust-only count reads **1034 where slice 8 read 1037**, and that is them. The plan asked for the
move whole and for this to be said; it is not a lost case (the union above has them), it is a
case that changed host.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core` (whole package, `--test-threads=2`) | **1034 passed, 0 failed, 2 ignored**, exit 0, 0 warnings, 8 result lines plus the doc-test line, `test_gpu_corpus` among them at 0 |
| `… --lib` | 514 passed, 2 ignored |
| `… --test test_module_layout` | 16 passed |
| `… --test test_ci_coverage` | 7 passed |
| `cargo build --features rust-only -p peacockdb-core`, cold (`cargo clean -p`) | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold | 0 warnings, 8 executables (12 in slice 8) |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh build -p peacockdb-core --features gpu`, cold | 0 warnings |
| `scripts/cargo-cudf.sh test -p peacockdb-core --no-run`, cold | 0 warnings, 8 executables |
| `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`, cold and again after rustfmt | 0 warnings |
| `--lib --features gpu -- gpu_tests:: --list` | 55, all under `gpu_tests` paths |
| `build-test-shadgpu.sh --build` (twice: before and after rustfmt) | 0 warnings, two binaries staged |
| shad-gpu, final tree | 55 + 8 passed, exit 0 (table above) |
| `sha256sum` over `testdata/goldens` | identical to the baseline, 170 files |
| `pipeline.yml` | YAML parses (7 jobs), `bash -n` on 29 rendered `run:` blocks clean, `resolve()` run against real json |
| `bash -n` on both scripts and on the rendered remote scripts (shad-gpu with and without a filter; `build-test.sh` `--gpu` and `--rust-only`) | clean |
| `git grep '#\[cfg(test)\]' -- peacockdb-core/src`, summed | 53, unchanged |
| `git grep -l '#\[test\]' -- peacockdb-core/src` outside a `test` path | 0 |
| `rustfmt --edition 2024 --check` | `abi.rs`, `exec.rs`, `gpu_backend/{accumulate,backend,join}.rs`, `executor_cases.rs`, `cpu_backend/tests/contract.rs`, `ffi_tests/mod.rs` clean as they were; `gpu_tests/{accumulate,backend,contract,join}.rs` and `gpu_backend/gpu_tests/mod.rs`, `wire/gpu_tests/mod.rs` **formatted** (import order after `peacockdb_core::` → `crate::`; the old files were clean), `gpu_backend/emit.rs` formatted (one pre-existing hunk); the seven `mod.rs` clean against stub children |
| `murmur_conformance.rs` | **left unformatted**: 11 hunks, 103 lines, all pre-existing in `test_inc2_conformance.rs` — a move, not a reformat |
| `git grep` for the four old target names outside `llm-wiki/tasks`, `llm-wiki/archive`, `llm-wiki/reports` | no hits in code, scripts, workflows or the three wiki pages |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`. verda: connection timed
out, so the CPU shapes ran here.

#### The ladder — the `gpu_backend` wall, and the register at zero

- Bare `pub` excluding `mod`: **265 → 242**, and **223 → 200 excluding `test_support`**.
  Twenty-three down, the mirror of slice 8's eighteen plus five: eighteen `impl pub fn` →
  `pub(crate)` (`accumulate.rs` 11, `emit.rs` 2, `join.rs` 5), four state structs (`Collapse`,
  `SortedRuns`, `AggregateBatches`, `LimitStream`) → `pub(crate)`, and `seen` deleted. Also
  `JoinCall` → `pub(crate)` and `GpuAccumulator` enum → struct, which move between rows rather
  than off the count; the five executor types changed files without changing it.
- `pub mod`: **11 → 7**. `PUB_MODULES`: **4 → 0**. `CROSS_COMPONENT_REACHES`: 0.
  `TEST_ONLY_ITEMS`: 10.
- Visibility dump 699 → **696**, `diff`ed against a dump of `7f358558`'s tree: exactly the items
  above and nothing else.
- **What still forces `pub`** on this side: `GpuSource`, `GpuExec`, `GpuExport`, `GpuAccumulator`,
  `GpuPartitionAccumulator`, `GpuEmitter`, `GpuJoin`, `GpuProbingJoin` and the `pub fn`s on them
  in `gpu_backend/mod.rs` — the associated types of `impl Backend for GpuBackend`, E0446 at
  `pub(crate)`, as slices 6 and 8 found for the CPU side. Nothing outside the crate names them.

#### Where a measurement contradicts the spec or plan

- **The spec's "`pub mod` down from 15 to six" reads seven**: `lib.rs` declares `common`,
  `executor`, `plan`, `plan_text`, `planner`, `wire` — the six — and `test_support`, which task 3
  added after the spec was written. Every subcomponent `pub mod` is gone.
- **Plan step 5, "`gpu_runtime_targets()` becomes `test_gpu_corpus` plus the lib entry"**: the
  lib is appended per mode instead, for the two reasons under steps 5-6 (the coverage guard's
  reader, and the function doubling as the rust-only membership test).
- **Plan step 4 / the spec's runner section say nothing about a filter meeting the rung.** Both
  runners forward `PCK_TEST_FILTER` to every binary, and libtest ORs filters, so the lib needed
  the `--exact` intersection or a developer's filter would have turned the rung off. That is
  `rung_args`, shared rather than written twice.
- **The plan's "roughly what the five staged binaries took"** has no number to compare against on
  this host; 9.2 s for 55 is what was measured.
- **`build-test.md`'s cpp-build-2502 and gpu-tests bullets** described five staged targets and no
  rung argument; rewritten from the scripts, not from the spec. The test table's `Runs`/`N`
  columns and the headline arithmetic are task 12's, as slice 8 left them.

#### Documentation the change falsified, fixed here

- `llm-wiki/build-test.md`: the five test-table rows that linked the four old files now link
  `cpu_backend/gpu_tests/murmur_conformance.rs`, `wire/gpu_tests/mod.rs`,
  `gpu_backend/gpu_tests/{exec,abi}.rs` and name `gpu_backend::gpu_tests::contract`; the
  cpp-build-2502 bullet stages the device rung as two binaries; the gpu-tests bullet says which
  binary takes `gpu_tests::` and that `running 0 tests` is an error; the `cargo-cudf.sh` example
  is `--lib --features gpu --no-run`; the `PCK_TEST_FILTER` bullet says how the gpu lib takes it.
- `llm-wiki/architecture.md`: the murmur gate's path.
- `llm-wiki/coding-style.md`: the paragraph excusing `test_inc2_conformance`'s name is gone with
  the name.
- `llm-wiki/tickets.md` #134: the three openers that compare and the one that does not, by module.
- `cpp/tests/gpu/test_cudf.cpp:81`, `scripts/cargo-cudf.sh:5`: the path and the example.
- `tests/test_ci_coverage.rs:490`: a clause naming `test_gpu_executor_misc`, a target that does
  not exist (pre-existing; one line).

#### For the slices after this one

- **Task 10**: `src/tests/mod.rs` gains `end_to_end`; nothing here touches it. The rust-only
  package count to move from is 1034.
- **Task 11**: `gpu_job_staged_targets()` reads the one `for t in test_…` line, and the lib is
  staged by a **separate** `stage peacockdb_core_gpu_lib …` line in the same step — the plan's
  "in the GPU job's `for t in …` array" is not where it is, because that reader takes `test_`
  names. The rung reaches the lib at `pipeline.yml:642` (`rung=gpu_tests::` keyed on the
  filename) and in `build-test-shadgpu.sh` through `RUST_LIB_STAGED`/`RUST_LIB_RUNG` and the
  `mapfile … rung_args` line, not on the invocation line the `--test-threads=1` reader matches.
  `the_three_gpu_target_lists_agree` compares `{test_gpu_corpus}` three ways today and passes.
  The fixture string at `test_ci_coverage.rs:291` still says `test_cpu_executors`.
- **Task 12**: the test table's four device rows are one rung now (55 on shad-gpu through one
  binary); the `Runs` column for the murmur gate's three CPU-runnable cases is `shad-gpu` only.
- `build-test.sh`'s lib entry in all three modes is rendered and syntax-checked, never run — the
  first `--host verda --rust-only --all` after this is its first execution.
- Both target dirs were `cargo clean -p peacockdb-core`ed for the cold counts;
  `target-cudf-rapids-cuda-12.2` holds the `gpu` `--lib` fingerprint last, `./target` the
  rust-only package run's. `cpp/install/rust-tests/` holds the two staged binaries (ignored).

### 2026-09-11 — slice 10 dispatched: plan task 11, and plan task 10's step 3

Slice 9 committed as `94fe9d7f`. Plan task 10's move was absorbed by slice 6 (`end_to_end.rs`,
24 passed, 2 ignored), so what remains of it is step 3 — reading the visibility dump for every
item still `pub` because a test crate names it, expected to be the eight `corpus.rs` and
`corpus_gpu.rs` force — and that rides with plan task 11, the `test_ci_coverage` shrink and the
four rung assertions each shown red. Plan task 12 is the slice after. verda still unreachable.

### 2026-09-11 — slice 10 done: `test_ci_coverage` — one assertion per rung, each shown red

Plan task 11, steps 1-5, and plan task 10's step 3 (the dump reading). Not committed. Two files
modified, nothing created or deleted:

- `peacockdb-core/tests/test_ci_coverage.rs` — 716 → 996 lines, 8 tests (7 before).
- `llm-wiki/build-test.md` — the CI-wiring-guard row: its subject, its anchors, its N (6 → 8).

`pipeline.yml`, `build-test.sh` and `build-test-shadgpu.sh` were edited only as red-watch
probes and are byte-identical to HEAD (`git diff --stat -- .github scripts` empty). No `src/`
file was touched, so the whole-package run was not repeated; slice 9's 1034 stands.

#### The negative control: what the guard at HEAD could not see

Before any edit, each of the four lines was deleted from `pipeline.yml` and the guard run:

| Deleted | HEAD's guard |
|---|---|
| (a) `cargo test --features rust-only -p peacockdb-core --lib` | **7 passed** — `line_runs_lib_tests` was still satisfied by the ffi line, which is a `--lib` run too |
| (b) `cargo test -p peacockdb-core --lib -- ffi_tests::` | **7 passed** — nothing read it |
| (c1) the `stage peacockdb_core_gpu_lib "$(cargo test --no-run … --lib --features gpu …)"` line | **7 passed** — `gpu_job_staged_targets()` reads the `for t in test_…` array only |
| (c2) `[ "\$tname" = peacockdb_core_gpu_lib ] && rung=gpu_tests::` | **7 passed** — nothing read it |
| (d) `cargo build --features rust-only -p peacockdb` | 6 passed, 1 failed — the one line already guarded |

So three of the four rungs could stop running with the guard green, and the fourth's assertion
was answered by the wrong line. That is what this slice closes.

#### What the guard reads now

- **Two rung matchers over `line_runs_lib_tests`**: `line_runs_cpu_rung` (features `rust-only`,
  no filter after `--`) and `line_runs_ffi_rung` (no `--features`, filter exactly `ffi_tests::`),
  over two small tokenisers `cargo_features` and `lib_run_filter`. Each rung line fails the
  other's matcher, which is the property (a) lacked.
- **The device rung, read as structure and not as a command line**: `gpu_job_step(name)` cuts a
  step body out of the workflow by its `name:` (it replaces `gpu_test_step()`, now
  `gpu_job_step(GPU_RUN_STEP)`); `gpu_job_staged_targets()` reads the `for t in test_…` array
  inside the staging step rather than anywhere in the file; `gpu_job_staged_by_name()` reads
  every folded `stage <literal> …` line — the lib's, since the loop's is `stage "$t"` — and
  `gpu_job_staged_lib()` takes the one that is `--no-run … --lib --features gpu`;
  `gpu_job_lib_rung()` reads the run loop's `[ "$tname" = <name> ] && rung=<filter>` line as
  `(name, filter)`. The assertion is then: a staged lib exists, the rung line names that file and
  `gpu_tests::`, and every rust invocation in the loop carries `$rung`. The plan's "in the `for t
  in …` array" is stale, as slice 9 said; what is asserted is what is there.
- **The four assertions live in one new test**, `each_rung_has_its_ci_line_and_the_cli_is_built`,
  and the `--lib`/CLI assertions that sat inside the sweep moved into it; the sweep keeps the
  target enumeration, the `GpuJob` array check and the stale-entry check. `workflow_lines()` is
  the shared reader.
- **The three lists gain the lib.** `the_three_gpu_target_lists_agree` still compares
  `{test_gpu_corpus}` three ways and now also reads `(staged name, rung)` from each runner:
  pipeline.yml (staging line + loop line), `build-test-shadgpu.sh` (`RUST_LIB_STAGED`,
  `RUST_LIB_RUNG`), `build-test.sh`'s `[ "$MODE" = "gpu" ]` branch (`LIB_STAGED`, `LIB_RUNG`);
  all three must be `Some` and equal. Then `rung_reaches_the_binary` answers the question the
  variables alone cannot: in each script the loop must hand `"$t"` and both variables to
  `rung_args`, and every `"$t" --…` invocation in that loop must carry `"${args[@]}"`. The loop
  is found from its feed line outward (`rposition` to the enclosing `for`), because the two
  scripts head it differently (`for t in …rust-tests/*` and `for name in $RUST_TEST_NAMES`) —
  the first draft anchored on `rust-tests/` and landed on a comment at `build-test.sh:234`,
  which is the one wrong turn this slice took and it was red, not silent.
- **`both_gpu_runners_pass_test_threads_one`** was checked against its question rather than its
  string: the flag sits on the invocation line itself in both runners (`"$t" --nocapture
  --test-threads=1 "${args[@]}"`), so it applies to every staged binary and nothing `rung_args`
  returns can remove it. Probe s6 below shows the reader still fires.
- The fixture at the old line 291 names `test_cpu_corpus`; `grep` for the seven retired target
  names over the file finds nothing.

#### Red-watch, `pipeline.yml` — every probe reverted, file byte-identical after

| Probe | What fired | What it printed |
|---|---|---|
| (a) rust-only `--lib` line deleted | `each_rung_has_its_ci_line_and_the_cli_is_built` | `no workflow line runs the cpu rung — the lib whole under --features rust-only. … Add `cargo test --features rust-only -p peacockdb-core --lib` to dataset-matrix.` |
| (b) `--lib -- ffi_tests::` line deleted | same | `no workflow line runs the ffi rung — `--lib -- ffi_tests::` at default features. … Add `cargo test -p peacockdb-core --lib -- ffi_tests::` to dataset-matrix.` |
| (c1) the two-line `stage peacockdb_core_gpu_lib …` deleted | same | `the `Build and stage rust GPU test binaries` step stages no lib binary: no `stage <name> "$(cargo test --no-run … --lib --features gpu …)"` line. … Lines naming a file outright: []` |
| (c2) the `rung=` line deleted | same | `the `Run GPU tests` loop hands no file a rung: no `[ "$tname" = <name> ] && rung=<filter>` line. The staged lib `peacockdb_core_gpu_lib` holds every rung, …` |
| (c3) `\$rung` dropped from the invocation, assignment left | same | `a rust invocation in the `Run GPU tests` loop does not pass $rung, so the assignment above it reaches nothing: env LD_LIBRARY_PATH=… "\$t" --nocapture --test-threads=1 > "\$tlog" 2>&1 \|\| rc=1` |
| (c4) rung keyed on `peacockdb_core_lib` | same | `… `peacockdb_core_gpu_lib` is staged, `peacockdb_core_lib` gets `gpu_tests::`` (`assert_eq` left/right printed) |
| (c5) `rung=device_tests::` | same | `… `peacockdb_core_gpu_lib` is staged, `peacockdb_core_gpu_lib` gets `device_tests::`` |
| (d) CLI build line deleted | same | `no workflow line builds the peacockdb CLI. … Add `cargo build --features rust-only -p peacockdb` to dataset-matrix.` |

Every probe: 7 passed, 1 failed, the one test.

#### Red-watch, the two scripts — every probe reverted, both files byte-identical after

| Probe | What fired | What it printed |
|---|---|---|
| (s1) `RUST_LIB_STAGED=peacockdb_core` | `the_three_gpu_target_lists_agree` | `the three runners disagree about the lib binary or its rung — pipeline.yml Some(("peacockdb_core_gpu_lib", "gpu_tests::")), build-test-shadgpu.sh Some(("peacockdb_core", "gpu_tests::")), build-test.sh --gpu Some(("peacockdb_core_gpu_lib", "gpu_tests::"))` |
| (s2) `RUST_LIB_RUNG=gpu::` | same | `… build-test-shadgpu.sh Some(("peacockdb_core_gpu_lib", "gpu::")) …` |
| (s3) `build-test.sh` gpu branch `LIB_RUNG=""` | same | `… build-test.sh --gpu Some(("peacockdb_core_gpu_lib", "")) …` |
| (s4) `build-test.sh`'s `mapfile … rung_args` line deleted | same | `scripts/build-test.sh: the lib's rung does not reach the binary — no line hands "$t", $LIB_STAGED and $LIB_RUNG to rung_args` |
| (s5) `"\${args[@]}"` dropped from the shad-gpu invocation | same | `scripts/build-test-shadgpu.sh: the lib's rung does not reach the binary — an invocation runs without the arguments rung_args produced: env LD_LIBRARY_PATH="\$PATCHED_LD" "\$t" --nocapture --test-threads=1 > "\$rlog" 2>&1` |
| (s6) `--test-threads=1` dropped from the shad-gpu invocation | `both_gpu_runners_pass_test_threads_one` | `scripts/build-test-shadgpu.sh runs a staged GPU binary without --test-threads=1: env LD_LIBRARY_PATH="\$PATCHED_LD" "\$t" --nocapture "\${args[@]}" > "\$rlog" 2>&1` |

#### The matcher pins, watched in the other direction

Each clause of the two rung matchers was weakened in turn to see the unit test refuse it:

| Weakened | `line_matcher_rejects_both_false_coverage_modes` |
|---|---|
| cpu rung without the features check | **stayed green** on the first pass — nothing pinned "the lib whole at default features is not the cpu rung"; that pin was added, and the weakening then fails at it |
| cpu rung without the filter check | red: `a filtered rust-only run is not the whole cpu rung` |
| ffi rung without the features check | red: `ffi_tests:: under rust-only selects nothing` |
| ffi rung matching a filter that merely starts with `ffi_tests::` | red: `a filter that merely starts with the rung's path is a narrower selection` |

The device readers are pinned as line parsers: `staged_by_name_of` refuses `stage "$t" …` and
`stage() {`; `lib_rung_of` refuses `[ -x "$t" ] || continue` and a commented-out assignment.

#### Plan task 10 step 3: the dump, read

`scripts/visibility-dump.py peacockdb-core/src` → 696 records (unchanged from slice 9). The
test crates reach the crate only through `peacockdb_core::` paths — every such reference was
listed (`grep -rhoE 'peacockdb_core::…' peacockdb-core/tests/`), and the `test_module_layout`
hits are string fixtures, not reaches. Setting `test_support` aside (`pub` by design, excluded
from the ladder), the items a test crate names are:

| Named by | Items | Forced by |
|---|---|---|
| `corpus.rs` and `corpus_gpu.rs` | `GpuNode`, `validate` (`plan/mod.rs`); `RecipePlan`, `attach_recipes` (`wire/mod.rs`); `RunReport`, `GpuBackend`, `GpuContext` (`executor/mod.rs`); `render_run` (`plan_text/mod.rs`) | **a test crate alone — the spec's eight, exactly** |
| `corpus.rs` | `CpuBackend`, `executor::run`, `planner::plan`, `register_tables_for`, `build_session_state` | the CLI too (`peacockdb/src/main.rs` names all five, plus `BatchSizing`, `PlanKnobs`, `SMALL_TABLE_BYTES` — the CLI's eight) |
| `corpus_gpu.rs`, as methods | `RecipePlan::wire_nodes`, `RecipePlan::bytes` (`wire/mod.rs:315,325`) | a test crate alone — two `impl pub fn` items on one of the eight, which leave with it when `test-support.md` removes `RecipePlan` |
| `corpus.rs`, `corpus_gpu.rs`, as a method | `CpuBatch::record_batch` | the CLI too (`main.rs:59`) |

No other `pub` item outside `test_support` is named by a test crate: nothing is a lift the spec
did not predict and nothing is a move that did not happen. The two `RecipePlan` methods are the
one thing the spec's count of eight does not spell out, and they are the same forcing source.
`GpuContext` is built by struct literal, so its fields — not items — are what `corpus_gpu.rs`
reaches; no `GpuNode` trait method is called from either file.

The two ladders, unchanged from slice 9 since no `src/` file moved:

- Bare `pub` excluding `mod`: **242**, **200 excluding `test_support`** (178 `top pub` + 71
  `impl pub`, minus the 7 `pub mod`). `pub mod`: **7**, all in `lib.rs` (`common`, `executor`,
  `plan`, `plan_text`, `planner`, `wire`, `test_support`).
- The register: `PUB_MODULES` **0**, `CROSS_COMPONENT_REACHES` **0**, `TEST_ONLY_ITEMS` 10. The
  plan's `grep -c 'PubModule {\|CrossComponentReach {'` reads 2, which are the two struct
  declarations; the arrays are `&[]`.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --test test_ci_coverage` | **8 passed**, 0 failed — `the_three_gpu_target_lists_agree`, `both_gpu_runners_pass_test_threads_one` and the new `each_rung_has_its_ci_line_and_the_cli_is_built` among them |
| `… --test test_module_layout` | 16 passed |
| `cargo test --features rust-only -p peacockdb-core --no-run` | 0 warnings |
| `pipeline.yml` after every restore | parses as YAML (7 jobs); `bash -n` clean on all 29 rendered `run:` blocks (python `yaml.safe_load`, each `run:` written to a file and `bash -n`ed) |
| `git diff --stat -- .github scripts` | empty — every probe restored from a copy taken before it |
| `sha256sum` over `testdata/goldens` against `test-layout-baselines/goldens.sha256` | identical, 170 files |
| `test_ci_coverage.rs` | 996 lines; not rustfmt-clean, as HEAD's was not (27 hunks at HEAD, all hand-wrapped assert messages; formatted it would be 1133 lines, over the 1000-line rule) — left in the file's own style, new lines kept within its existing 120-column maximum |
| `git grep` over the file for the seven retired target names | no hits |

No device run: nothing here touches a device path, and the scripts were only probed and
restored. verda unreachable; everything local.

#### Where a measurement contradicts the spec or plan

- **"test_ci_coverage shrinks to about 300 lines"**: it is 996, from 716. The spec's estimate
  assumed the lists were the bulk; they were four `INTENTIONALLY_NOT_IN_CI` entries and one
  doc clause, gone in slice 9 (716 against master's 720). What remains is guards and their pins
  — the matcher unit tests the spec says must stay whole — and this slice adds the four rung
  assertions, four readers and their pins. Nothing in the file could be cut without deleting a
  guard; recorded, not bent.
- **The plan's "the staged lib binary in the GPU job's `for t in …` array"**: the lib is staged
  by its own `stage` line, as slice 9 said, and that is what is asserted.
- **The spec's table gives `test_ci_coverage` N = 6**; it is 8, and `build-test.md` now says so.
- **The plan's register count command** (`grep -c 'PubModule {\|CrossComponentReach {'`) counts
  the struct definitions and reads 2 at an empty register; the arrays are what is zero.

#### Documentation the change falsified, fixed here

- `llm-wiki/build-test.md`: the CI-wiring-guard row — one assertion per rung and the CLI, the
  lib agreement across the three runners, three anchors (`#L610`, `#L680`, `#L753`), N = 8.
  Nothing else on the page names the guard's subject in a way this slice changed.

#### For plan task 12

- The test table's `Runs`/`N` columns and the headline arithmetic, as slices 6-9 left them; the
  CI-wiring-guard row is done.
- The four device rows are one rung (55 on shad-gpu through one binary), and the murmur gate's
  three CPU-runnable cases run on shad-gpu only — slice 9's note stands.
- `test_ci_coverage.rs` sits at 996 lines. The next assertion added to it crosses the
  1000-line rule; the split that would pay is the runner readers (`rust_gpu_runner_*`,
  `rung_reaches_the_binary`, the three-lists test) into a second file, which is not this
  task's move and was not made.

### 2026-09-11 — slice 11 dispatched: plan task 12, the wiki from measured numbers

Slice 10 committed as `4a2f653b`. This is the last slice: `build-test.md`'s two tables from the
final inventories, `coding-style.md`'s rung ladder and its Visibility section brought to what the
register now says, `architecture.md` checked, #49 retired, and the final proof. verda still
unreachable; shad-gpu is not needed.

### 2026-09-11 — slice 11 done: the wiki, from measured numbers

Plan task 12, steps 1-6. Not committed. Four wiki files modified, no code, no script:
`llm-wiki/build-test.md`, `llm-wiki/coding-style.md`, `llm-wiki/tickets.md`,
`llm-wiki/archive/archived-tickets.md`. `architecture.md` was checked and needed nothing.

#### Step 1-2: the two tables, and the arithmetic

Fresh inventories on the final tree, `/tmp/inv11-{rust-only,cudf,gpu}.txt` (baselines on disk
untouched). Every N in the first table is a per-module count read off the `gpu` inventory's `--lib`
(574 cases, the superset) plus the seven binaries' counts from the `cudf` one, and `peacockdb-ffi`'s
two. Each row's rung is its module's declaration: `tests` (cpu), `ffi_tests` (ffi), `gpu_tests`
(gpu); each row's tier is the path — `tests/test_*.rs`, `src/tests/`, `<component>/tests/`,
`<component>/<sub>/tests/`, `foo/tests.rs`.

The first table, per block:

- **cpu** 987 = `--lib` under `rust-only` 516 + `test_cpu_corpus` 448 + `test_corpus_goldens` 20 +
  `test_cost_model` 3. The 516 by tier: crate integration internal 26 (`tests::end_to_end`);
  component 59 + 4 + 50 + 21 + 13 + 1 = 148 (`plan::tests` 63 = rules 31, joins 23, aggregate 5,
  layout injection 4; `planner::tests` 50 = capability 13, null analysis 8, refusals 10, plan goldens
  19; `wire::tests` 21; `plan_text::tests` 13; `executor::tests` 1); subcomponent 90 + 65 + 38 = 193
  (`driver::tests` 90; `cpu_backend::tests` 65 = 64 + the contract's 1; `translator::tests` 29 +
  `schema_tests` 9); module unit 43 + 5 + 13 + 31 + 11 + 27 + 3 + 16 = 149 (driver's four
  `accounting` 14, `index` 4, `scheduler` 15, `single_partition` 10; `forwarder` 3 + `row_range` 2;
  `expr_physical` 13; `validate` 23 + `layout` 4 + `aggregate` 4; `memory_estimation` 11;
  `translator::expr` 13 + `parquet_meta` 6 + `partition` 8; `expr_text` 3; `expr_writer` 16).
  26 + 148 + 193 + 149 = 516. `test_cpu_corpus` 448 is 447 in Corpus cpu (444 cells + 3 declaration
  checks) and 1 in Registry ↔ CSV, cpu.
- **ffi** 5 = `executor::ffi_tests` 3 + `peacockdb-ffi --test test_ffi` 2. The `cudf` `--lib` is 519
  = 516 + 3, and `ffi_tests::` matches 0 lines in the `rust-only` inventory.
- **gpu** 63 = `--lib -- gpu_tests::` 55 (`cpu_backend::gpu_tests::murmur_conformance` 10,
  `gpu_backend::gpu_tests::{abi 4, accumulate 10, backend 2, contract 1, exec 12, join 6}` = 35,
  `wire::gpu_tests` 10) + `test_gpu_corpus` 8 (7 in Corpus device: 6 cells + the regeneration guard;
  1 in Registry ↔ CSV, device). The `gpu` `--lib` is 574 = 519 + 55, and `gpu_tests::` matches 0
  lines in the `cudf` inventory.

First table 987 + 5 + 63 = **1055**. Second table, Rust: `test_golden_format` 26 +
`test_ci_coverage` 8 + `test_module_layout` 16 + `cost-report` 37 (`cargo test -p cost-report --
--list` → `37 tests`) = **87**. Rust 1055 + 87 = **1142**. C++ **66**: `--gtest_list_tests` on
the five staged `cpp/install/bin/peacock_*_tests` binaries — cpu 11, gpu 6, plan 27, tpch 4,
tpchv 4 — plus `TEST(` counts in the five sources that are not built here — streamed 4, nodes 1,
multi-gpu 4 + 4 + 1. Python **369**: `def test_` counts, 41 (`test_duckdb_cost.py`) + 216 (the
ten prototype files the CI step globs) + 19 (`test_tpch.py`) + 93 (`test_tpch_corpus.py` 22 +
`plans_tpcds.QUERIES` 71). Headline **1577 = 1142 + 66 + 369**.

**Against the headline the page carried**, 1569 = Rust 1135 + C++ 65 + Python 369: the old N
column already summed to 1142 for Rust — slices 4 and 10 raised `test_module_layout` 11 → 16
and `test_ci_coverage` 6 → 8 in the rows and nobody re-summed the header, so the "four-case
discrepancy task 2 closed" had reopened by seven. C++ moves 65 → 66 because `peacock_gpu_tests`
lists six cases: task 3 added `RmmPool.ReservesTheDeclaredBudget` and the row was never
re-counted. Python is unchanged. Recorded, not bent: the header follows the rows.

The check is mechanical — a script sums the N column per block and per table and compares
against the block headers and the headline (`/tmp/s11-sum.py`, twelve lines of python;
`blocks: cpu 987/987, ffi 5/5, gpu 63/63; computed (1577, 1142, 66, 369) == header`). Run on
the old page it cannot parse two tables, which is the red; on the new one it says `OK`.

Other changes on the page: every `#L<n>` anchor in both tables is gone — sixteen of the
twenty-five pointed at a line that no longer holds the named case (`test_cost_model.rs#L36` is
`fn cost_total`, `plan_goldens.rs#L708` is a format string, `main.rs#L1552` is blank), so the
example name is the link and a file link is what stays true. The `Runs` legend says the first
table carries the job in its block header. `case-inventory.sh`'s row names the `gpu` shape
(slice 1's note). The `rust-only` section no longer says the GPU test files are gated at file
level: `test_gpu_corpus` is, the `ffi_tests` modules are `not(rust-only)`, and the device tests
are `gpu_tests` under `feature = "gpu"`.

#### Step 3: `coding-style.md`

Two bullets added above the `#[cfg(test)]` one: the test-code rules (no test code in a production
file, `foo.rs` beside `foo/tests.rs`, `test` in every test-only path, `#[cfg(test)]` on a
declaration only) and the rung ladder with both gates spelled out and the both-directions rule.
The carve-out bullet lost its restatement of the path rule (one rule, one place). The Names
section's `test_inc2_conformance` paragraph is gone — confirmed absent; slice 9 removed it. The
Visibility bullet that said "Four more exist under `executor/` … They expire when `test-layout.md`
moves those files" is present state now: seven `pub mod`, all in `lib.rs`, both registers empty;
and a new bullet names the eight items `corpus.rs`/`corpus_gpu.rs` force, with the 200 raw count.

#### Step 4: `architecture.md`, checked and not changed

Each sentence the spec and plan name, grepped against the tree: "The scheduling rule" —
`executor/driver/` with `partitioned.rs`, `single_partition.rs`, `scheduler.rs` (all exist);
"Four things in `executor/driver/accounting.rs` are load-bearing" (exists); the wire-format
table's `expr_writer.rs`, `node_writer.rs`, `join.rs`, `aggregate_writer.rs` under `wire/` (all
exist); "Rehash and the comet hash" — `executor/cpu_backend/spark_partitioning.rs` and the gate
at `cpu_backend/gpu_tests/murmur_conformance.rs` (both exist; slice 9 renamed the gate). All 27
relative links in the page resolve, and so do every link in `build-test.md` and
`coding-style.md`. Nothing false found, nothing edited.

#### Step 5: #49 retired

Moved whole from `tickets.md` to `archived-tickets.md` under Done, first entry, with a Done line:
one testdata root, `test_support::testdata_root()`, called by the five `src` files and delegated
to by `tests/common/mod.rs`; `git grep CARGO_MANIFEST_DIR -- peacockdb-core/src` is empty. The
four `cost-report` sites slice 3 flagged stay: that crate's tests run where the source is and are
never staged, which is outside what the ticket is about. Index row: Infrastructure 23 → 22, #49
dropped; the anchors in the file and the numbers in the index are the same 80. Two code comments
name #49 as the reason for the root (`test_support/{mod,testdata}.rs`) and stay — the number
resolves in the archive.

#### Step 6: final proof, on the final tree

| Check | Result |
|---|---|
| `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3 cargo test --features rust-only -p peacockdb-core -- --test-threads=2` | **1035 passed, 0 failed, 2 ignored**, exit 0, `warning` 0 times in the log; 8 binaries: `--lib` 514 + 2 ignored, `test_ci_coverage` 8, `test_corpus_goldens` 20, `test_cost_model` 3, `test_cpu_corpus` 448, `test_golden_format` 26, `test_gpu_corpus` 0, `test_module_layout` 16 |
| `sha256sum` over `testdata/goldens` vs `goldens.sha256` | empty diff, 170 files — before the suite and after it |
| `cargo test --features rust-only -p peacockdb-core --no-run`, cold (`cargo clean -p`) | 0 warnings, 8 executables |
| `scripts/cargo-cudf.sh build -p peacockdb-core`, cold | 0 warnings |
| `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` | 0 warnings |
| `test_support` probe: `pub fn probe_test_support()` in `lib.rs` naming `crate::test_support::testdata_root()` | `cargo build --features rust-only` → `error[E0433]: could not find test_support in the crate root`, rc 101; `cargo test … --lib --no-run` → Finished, rc 0; reverted, `git diff --stat -- peacockdb-core` empty |
| `cargo test -p cost-report -- --list` | 37 tests |
| `git grep -n '#\[cfg(test)\]' -- peacockdb-core/src` | 53 lines: 27 `mod tests`/`mod schema_tests` declarations (+4 `cfg(all(test, …))` rung gates found separately: `ffi_tests` ×1, `gpu_tests` ×3), 10 registered `TEST_ONLY_ITEMS`, 6 gated `use` lines serving them, 10 inside doc comments and prose. Nothing else |
| `git grep -l '#\[test\]' -- peacockdb-core/src` | 48 files, every path containing `test`; 0 outside |
| `test_module_layout` on the final tree | 16 passed (inside the package run) |

Inventories — the three shapes against the baselines, leaf names (last `::` segment) as sets and
multisets; `compare-inventory.sh` says `DRIFTED` for all three, as it must:

| Shape | Cases | Leaves lost | Leaves gained | Duplicated leaves |
|---|---|---|---|---|
| `rust-only` | 1037 (baseline 1034) | the murmur gate's 3 CPU-runnable cases, now device rung (slice 9) | 6: slice 4's five layout cases, slice 10's `each_rung_has_its_ci_line_and_the_cli_is_built` | 3, unchanged |
| `cudf` | 1048 (1097) | 48 = 55 device leaves − 7 names the cpu tier shares | the same 6 | 10 → 3, the 7 shared names no longer pairs here |
| `gpu` | 574 (435) | none | 132 = the 133 moved into `--lib` − 7 shared + the 6 | 3 → 10, the cudf baseline's ten exactly |
| union of three | 1093 distinct (1087) | **none** | the same 6 | |

Against slice 9's `/tmp/inv9-*.txt` the only diff in every shape is `test_ci_coverage` 7 → 8.

The ladders: bare `pub` excluding `mod` **242**, **200 excluding `test_support`** (74 struct,
46 fn, 31 enum, 13 trait, 5 const, 2 type at top level; 70 `impl fn`, 1 `impl const`);
`pub mod` **7**, all in `lib.rs` (`common`, `executor`, `plan`, `plan_text`, `planner`, `wire`,
`test_support`); `PUB_MODULES` **0**, `CROSS_COMPONENT_REACHES` **0**, `TEST_ONLY_ITEMS` **10**;
dump 696 records. All unchanged from slice 10, as they must be — no code moved.

#### Where a measurement contradicts the spec, the plan or the dispatch

- **The dispatch's "expect 1034 passed"** is slice 9's figure; slice 10 added one
  `test_ci_coverage` case without re-running the package, so 1035 is right.
- **The spec's "the four-case discrepancy that sum has today is closed here"** was a
  seven-case one by the time this slice ran (above), and the C++ row was off by one for a
  different reason. Both closed by re-summing.
- **The spec's "`pub mod` is down from 15 to six"** reads seven, for slice 9's reason:
  `test_support` is a component `lib.rs` declares, added by task 3 after the spec was written.
- **The spec's "Done when" wording for the `#[cfg(test)]` carve-out** — "in a component's
  `mod.rs`" — is narrower than what the tree holds and `coding-style.md` states: two registered
  items sit in implementation files because they read private state (`accumulate::compactions`,
  `join::has_finish_pass`) and one in a subcomponent's `mod.rs`. Slice 4 recorded the widening;
  the register checks all ten.
- **"`test_ci_coverage` is near 300 lines"**: 996, slice 10's finding, unchanged.
- **The spec's cross-checks "one per rung plus the binaries"** hold as measured: 516 / 3 / 55
  and the seven binaries, with each filtered rung matching nothing in the shape below it.

#### "Done when", clause by clause

- *exactly eight items `pub` because a test crate forces them, the named eight, forced by
  `corpus.rs`/`corpus_gpu.rs`* — **holds**: the eight are `top pub` in the dump at the named
  files, and the `use peacockdb_core::` lines of the two corpus files name exactly them beyond
  what the CLI also names (slice 10's reading, re-grepped here).
- *`PUB_MODULES` is empty* — **holds**, `&[]` at `test_module_layout.rs:38`.
- *`pub mod` is down from 15 to six, counted by `visibility-dump.py`* — **seven**, not six:
  the spec predates `test_support`; every subcomponent `pub mod` is gone, which is what the
  clause means.
- *the raw bare-`pub` count is whatever task 4 inherits* — 242, 200 outside `test_support`.
- *no production file contains a `#[test]`* — **holds**, 48 files all under a `test` path.
- *no `#[cfg(test)]` anywhere but a test-module declaration or a carve-out entry point* —
  **holds** under the two-case rule (above); 27 + 4 declarations, 10 registered items, 6 gated
  `use` lines, and the rest prose.
- *every test module declares the lowest rung it needs and is named for it, both directions* —
  **holds**: `a_test_module_is_named_for_its_rung` and `a_rung_gate_implies_its_module_name`
  pass, and the filtered lists are 0 / 3 / 55 across the three shapes with no lower-rung case
  in either filter.
- *every test-only path in `src/` carries `test`* — **holds**, `a_test_only_path_carries_test`
  passes.
- *`crate::test_support::testdata_root()` is the only testdata root, and #49 closes* —
  **holds**: no `CARGO_MANIFEST_DIR` in `peacockdb-core/src`, `tests/common/mod.rs` delegates,
  #49 archived under Done.
- *a plain `cargo build` cannot name `test_support`* — **holds**, the E0433 probe above.
- *`test_ci_coverage` is near 300 lines and asserts one CI line per rung plus the CLI build* —
  **996 lines**; the four assertions exist and were each watched red in slice 10.
- *`build-test.md`'s two tables add to the headline* — **holds**, 1055 + 87 + 66 + 369 = 1577,
  checked by script.
- *the leaf-name set is the one the baselines recorded — moves, no deletions* — **holds**:
  nothing lost in any shape or in the union; gained only the six cases slices 4 and 10 wrote.

#### For the completeness pass

- `/tmp/s11-sum.py` is the arithmetic check and lives in `/tmp` only; the page carries the
  per-block totals in its header rows, so a reader can re-add by hand.
- The C++ multi-GPU, streamed and node-timing rows are source counts (`TEST(` lines), not a
  `--gtest_list_tests`: those binaries are `EXCLUDE_FROM_ALL` or manual and are not built here.
- verda unreachable throughout; shad-gpu not needed. Both target dirs were cleaned for the cold
  counts and rebuilt; `./target` holds the rust-only package build, `target-cudf-*` the `gpu`
  `--lib` fingerprint last.

### 2026-09-11 — all twelve slices in; task 4 is reviewing, PR #145 against `ENS-rmm-pool-budget`

Slice 11 committed as `d30e15e6`. Twenty-one commits over task 3's tip, 124 files, and the PR's
commit count matches the branch's, so it carries this task alone. Review round 1 dispatched.

### 2026-09-11 — review round 1: 0 blocking, 4 important, 7 nits

The reviewer verified by text: every one of the 1087 baseline leaves present and in the module the
spec names; both `#[ignore]`s travelled; all 29 test-module declarations agree name and gate;
`gpu_tests::` 55, `ffi_tests::` 3, rust-only `--lib` 516; the `test_ci_coverage` readers ported
and run over mutated copies of the workflow and scripts, every probe red; `build-test.md`'s
arithmetic re-added; `pub mod` 7, bare `pub` 200 outside `test_support`.

Important:

1. `scripts/build-test.sh:349,357,367` — `lib_target` is appended in every mode, so `RUST_TESTS` is
   never empty and the "derived suite must not be EMPTY" guard at `:395` can no longer fire. Fix:
   derive into `DERIVED`, assert non-empty, then append the lib.
2. `scripts/lib/rung-args.sh:27-28` — with a filter set, a failed `--list` pipeline (binary will
   not load, wrong env prefix) yields the same `--exact ''` as an empty intersection, and
   `build-test-shadgpu.sh:391` suppresses the zero-test guard because a filter is set. Reproduced
   with `/bin/false` as the binary. Fix: capture the list, return non-zero on failure, read
   `rung_args` through a checked assignment so the status reaches `rc`; print "0 of N rung cases
   match" for a genuine empty intersection.
3. `tests/test_module_layout.rs` is 1827 lines (1211 at the parent); `tests/test_ci_coverage.rs` is
   996 only because rustfmt was withheld (1133 formatted). `coding-style.md`'s 1000-line rule
   exempts `mod.rs`/`common.rs` only. Fix: `tests/test_module_layout/{…}.rs` submodules declared
   with `mod` from the main file (same target — `workspace_test_targets()` reads `tests/*.rs`
   only), the runner readers of `test_ci_coverage.rs` the same way, then rustfmt both.
4. `scripts/build-test.sh:505-530,656-660` — no mode has executed the lib entry. Fix:
   `scripts/build-test.sh --rust-only --build` locally (needs no host) proves the lib stages under
   its shape name; a `--host verda --rust-only --all` when verda answers.

Nits (5-11): `build-test.md:18` "a block's N is one CI line's case count" is false — the cpu
header lists four lines; `build-test.md:370` and `case-inventory.sh:33-34` "no device case lives
in a binary" contradicts `test_gpu_corpus` 8; `build-test.sh:657` a missing staged binary is
"skipping" and green, and the loop has no `running 0 tests` guard; `test_module_layout.rs:1149`
`declares_mod` misses a one-line `#[cfg(test)] mod gpu_tests;` — constructed input is green under
both rung rules; comment caps: `test_module_layout.rs:1024` doc 11 lines, `compare-inventory.sh:2`
header 12, `pipeline.yml:285/375/385` 7/6/5 lines above a command in a `run:` body; `lib.rs:3`
"Six components" vs `coding-style.md` seven; `build-test.md:94` "was watched red" is history in a
current-state page.

### 2026-09-11 — round 1 addressed

All four important findings and the seven nits. Not committed. Files touched: `scripts/build-test.sh`,
`scripts/build-test-shadgpu.sh`, `scripts/lib/rung-args.sh`, `scripts/case-inventory.sh`,
`scripts/compare-inventory.sh`, `.github/workflows/pipeline.yml`, `peacockdb-core/src/lib.rs` (doc
only), `peacockdb-core/tests/test_module_layout.rs` + new `tests/test_module_layout/{near_miss,
privacy,test_code,tree,visibility,walls}.rs`, `peacockdb-core/tests/test_ci_coverage.rs` + new
`tests/test_ci_coverage/runners.rs`, `llm-wiki/{build-test,coding-style}.md`. Every finding was
verified on the tree before it was fixed; one (2) reproduces differently from how it was stated.

#### 1 — the empty-suite guard could not fire: confirmed, fixed, watched

Confirmed with a copy of HEAD's script whose three derivers `return 0`: `--rust-only --run` printed
`peacockdb_core_rust_only_lib: missing, skipping` and exited **0**; `--gpu` got past the guard too.
(The default mode did fire, for a side reason: `needs_cmake_targets | grep -vxF -f <(empty)` exits
1 inside the process substitution and `set -e` ends it before `lib_target` prints — not a guard.)
Fix as proposed: each mode derives into `DERIVED`, the guard reads `${#DERIVED[@]}`, then
`RUST_TESTS=("${DERIVED[@]}" "$(lib_target)")`. The same three-way copy on the fixed script: all
three modes print `error: the derived Rust suite is EMPTY for mode '…'` and exit 1.

#### 2 — a failed `--list` and an empty intersection: distinguishable now, status reaches `rc`

Verified at HEAD by sourcing `rung_args`: a lib binary that will not load (a stub exiting 127 with
the loader's message) and a genuine empty intersection (`zzz` against the real lib) both returned
`[--exact, '']`. **The reviewer's `/bin/false` case was not green at HEAD, though**: both runners
then *run* the binary with those arguments, it fails to load again, and its non-zero status is
folded into `rc` — reproduced through both rendered gates (`!!! peacockdb_core_gpu_lib FAILED (exit
127)`, exit 1). What was green at HEAD is the adjacent case the same hole covers: a lib that loads
but whose rung lists nothing (a rust-only lib staged under the gpu name; a renamed `gpu_tests`)
under a filter — `--exact ''`, `running 0 tests`, guard suppressed, **exit 0 in both runners**.

`rung_args` now captures the listing (`listed=$(…) || return 1`), returns 1 when the rung lists no
case, prints `rung_args: 0 of N <rung> cases match '<filter>'` to stderr for a real empty
intersection, and emits the empty name *before* `--exact` — a caller reading it through `$(…)`
loses trailing newlines, and `--exact` alone runs every case (comment at the site). Both runners
read it through a checked assignment, `if ! args_text=$(rung_args "$t" …); then echo "!!! … could
not list its cases — it was not run"; rc=1; continue; fi; mapfile -t args <<< "$args_text"`. A
temp-file form was tried first and dropped: its `> "$alog"` redirect matches
`is_rust_gpu_runner_invocation`, which would then demand `--test-threads=1` on the listing line.

Exercised locally on the staged rust-only lib, eight cases: other binary/no filter `['']`, other/`q6`
`[q6]`, lib/no filter `[tests::]`, lib/empty rung `[q6]`, lib/`plan_goldens::tpcds_tp1` `['',
--exact, <2 names>]` and the binary then runs exactly those 2, lib/`zzz` `['', --exact]` with the
`0 of 516` line and the binary runs 0, lib will not load → return 1, rung `nomatch_tests::` →
`lists no case`, return 1. Then the rendered gates (`ssh … bash` → `bash`, a local remote dir):

| Case | `build-test.sh --gpu` HEAD → fixed | shad-gpu gate HEAD → fixed |
|---|---|---|
| A: lib will not load, filter `abi` | exit 1 (run failed) → exit 1, `could not list its cases` | exit 1 (`FAILED (exit 127)`) → exit 1, `could not list` |
| B: rung lists nothing, filter `abi` | **exit 0** → exit 1, `gpu_tests:: lists no case` | **exit 0** → exit 1 |
| C: rung lists nothing, no filter | **exit 0** (no zero guard, finding 7) → exit 1, `ran 0 tests … nothing was verified` | exit 1 at HEAD already (slice 9's guard) |
| D: lib not staged, no filter | **exit 0**, `missing, skipping` → exit 1, `is not staged on the host` | n/a (globs) |
| E/F: intersection of 2 / of 0, rung `tests::` | | 2 passed, 514 filtered / `0 of 516 … match 'zzz'`, `running 0 tests`, exit 0 |

Both runners' rendered remote scripts (`--rust-only`, `--gpu`, default; shad-gpu with and without a
filter) are `bash -n` clean; `test_ci_coverage` 8 passed over the edited scripts, so the
`rung_reaches_the_binary` and `--test-threads=1` readers still find their lines.

#### 3 — the two files over 1000 lines: split into submodules, rustfmt clean

`tests/test_module_layout.rs` (1827) → a 43-line root holding the `//!` header, six `mod`
declarations, and `repo_root`/`src_root` — the root keeps `repo_root` because `build-test.sh`'s
classifier greps `tests/*.rs` for that name — plus `test_module_layout/`: `tree.rs` 79 (sources,
components, `code_only`), `visibility.rs` 368 (where `pub` may appear, `PUB_MODULES`,
`files_naming`), `walls.rs` 328 (sibling reach, `CROSS_COMPONENT_REACHES`, `super::` climbs),
`privacy.rs` 224 (private types in signatures, the compile probe), `test_code.rs` 614 (`RUNGS`,
`TEST_ONLY_ITEMS`, the mod-declaration readers, the four placement rules), `near_miss.rs` 279 (the
one near-miss test). `tests/test_ci_coverage.rs` (996 unformatted) → 647 + `test_ci_coverage/
runners.rs` 510 (the pipeline-gpu-job, shad-gpu and `build-test.sh` readers, `gpu_runtime_targets`,
`rung_reaches_the_binary`, the invocation readers, and the four tests over them). All under 1000
after `rustfmt --edition 2024`, which is now clean on both roots and every child.

- **Declared with `#[path = "test_module_layout/x.rs"] mod x;`**, not a bare `mod`: a test target
  is a crate root, so `mod x;` resolves to `tests/x.rs`, which cargo would make a target. The tree's
  own precedent is the old `test_gpu_executors.rs`, same comment. A `tests/<name>/main.rs` layout
  was rejected because `workspace_test_targets()` reads `tests/*.rs` and would stop counting it.
- No `pub use`; cross-module items are `pub(crate)` and reached by `use crate::<module>::…` —
  absolute paths, no `super::`. `grep` over the new files for `pub use` and `super::`: nothing.
- `workspace_test_targets()` still counts 8 targets: the directories carry no `.rs` extension.
  `case-inventory.sh`'s `find -maxdepth 1 -name 'test_*.rs'` the same.
- `files_naming`'s own-file exclusion now excludes the target's directory and root file from
  `file!()`, since the string fixtures live in `near_miss.rs` and the function in `visibility.rs`.
  Red-watched: with the directory clause dropped, `each_reader_sees_the_violation_and_not_its_
  near_miss` fails at "the guard states the rule and must not report itself as forcing the
  exemption"; restored.
- The layout test's own rules: every `#[test]` is under `tests/test_module_layout/` or
  `tests/test_ci_coverage/`, both paths carrying `test`.
- `--list` before and after, leaf names: `test_module_layout` 16 = 16, `test_ci_coverage` 8 = 8,
  both sets identical. Paths gained a module prefix (`near_miss::each_reader…`, `runners::
  the_three_gpu_target_lists_agree`), so a `compare-inventory.sh` line for these two targets moves
  while the leaf set does not.
- Wiki pointers moved with the items: `coding-style.md` names `test_module_layout/test_code.rs`'s
  `TEST_ONLY_ITEMS` and `test_module_layout/{visibility,walls}.rs`'s registers; `build-test.md`'s
  two example links now resolve to `privacy.rs` and `runners.rs`. Every `tests/` link on the page
  resolves.

#### 4 — the lib entry, executed

`scripts/build-test.sh --rust-only --build` locally (twice, before and after the script edits):
five binaries staged under `cpp/build26/install/rust-tests/`, the lib as
**`peacockdb_core_rust_only_lib`** (1.1 GB unstripped), `--list` on it 516 tests, 0 warnings.
`test_ci_coverage` and `test_module_layout` are not staged, as the classifier intends. Then the
rendered `--rust-only --run` gate against those binaries, `REMOTE_DIR` a local directory holding
them and a real `testdata/` copy: `test_corpus_goldens` 20, `test_cost_model` 3, `test_cpu_corpus`
448, `test_golden_format` 26, **`peacockdb_core_rust_only_lib` 514 passed, 2 ignored**, exit 0 —
on the final scripts, so the new loop (`tee`, `PIPESTATUS`, the zero guard) ran the whole rust-only
suite once. One wrong turn: the first run had `testdata` as a symlink and the five tpcds plan
goldens failed on q27's `file_groups` path — `relative_to_testdata` canonicalises the root but
DataFusion prints the path it was given — a harness artefact, not a script or engine fault (verda's
`$REMOTE_DIR/testdata` is a real directory); re-run with a real directory, green.

**verda: answers, unusable.** `ssh verda` first refused with a changed ED25519 host key
(reprovisioned); re-keyed per `build-test.md`'s rented-hosts rule (`ssh-keygen -R`, keyscan), and
it then answers `Permission denied (publickey)` — the new instance does not carry this key. No
`--host verda --rust-only --all`; the local rendered gate above is the lib entry's execution.

#### 7 — a missing staged binary, and `running 0 tests`, in `build-test.sh`

Agreed: the suite is named per mode and shape, so a binary the host lacks is a `--run` after
another mode's `--build` — the very case the shape-named lib exists to expose — and skipping it
green defeats that. Now `!!! <name> is not staged on the host — nothing was verified`, `rc=1`. The
loop tees each binary into `/tmp/<name>.rustlog`, folds `PIPESTATUS[0]` into `rc`, and fails on
`^running 0 tests` when no filter is set, the shad-gpu gate's rule. Cases C and D above.

#### 8 — `declares_mod` and a one-line gate

Confirmed red first: the pin `declares_mod("#[cfg(test)] mod gpu_tests;") == Some(("gpu_tests",
false))` failed with `left: None`. `split_attributes` now strips a line's leading `#[…]`
attributes; `declares_mod` reads past them, `mod_declarations` takes a `#[cfg(` on the
declaration's own line as its gate before scanning upward, and `test_gates` sits a same-line gate
on the rest of that line rather than on the next — the same hole, pinned too (`test_gates("#[cfg
(test)] fn hops() {}\nmod tests;")[0].sits_on == "fn hops() {}"`). Tree red-watch: `wire/mod.rs`'s
gate rewritten as `#[cfg(test)] mod gpu_tests;` on one line — both rung rules fire (`is gated
#[cfg(test)] and its rung requires #[cfg(all(test, feature = "gpu"))]`; `sits on mod gpu_tests and
belongs on a name below the rung its gate names`), 14 passed 2 failed; reverted, diff empty.

#### 5, 6, 9, 10, 11 — the nits

- 5: `build-test.md` line 18 now reads "a block's header sums its CI lines, one per binary" — the
  cpu header is four lines in `pipeline.yml` (`--lib`, `test_cpu_corpus`, `test_corpus_goldens`,
  `test_cost_model`), checked by grep.
- 6: `test_gpu_corpus` is a `--test` binary holding 8 device cases, so "no device case lives in a
  binary" was false. The true reason `gpu` lists `--lib` alone: no `tests/*.rs` reads `feature =
  "gpu"` (grep: none), so the seven binaries list under `cudf` exactly what they would list under
  `gpu`. Said so in `build-test.md` and `case-inventory.sh`.
- 9: counted with a script over every touched file (contiguous comment lines; 10 at indent 0, 4
  inside a body). Cut: `TestOnlyItem`'s doc 11 → 10, `compare-inventory.sh`'s header 12 → 10,
  `pipeline.yml`'s three run-body comments 7/6/5 → 4/4/4. Counted and left: the overruns that
  predate this branch (`build-test.sh` 8/110/233/275/…, `build-test-shadgpu.sh` 2/153/186,
  `test_ci_coverage.rs`'s header and `gpu_runtime_targets` doc, `pipeline.yml` 157/181/251/…) —
  `git blame` against the parent shows no line of those added here, and a comment-only pass over
  them is a separate change.
- 10: `lib.rs` says seven components and names `test_support` as the harness's, behind its feature.
- 11: the "watched red" clause is gone from the CI-wiring-guard row.

#### The shad-gpu run — one cycle on the new gate

`--build` (0 warnings; `test_gpu_corpus` and `peacockdb_core_gpu_lib` staged), `--push-binaries`,
`--patch --run-detached`, then `--run-status` polled; each a foreground call under `timeout`. No
`[rmm] pool … could not be built`: every C++ binary reported its pool against 103 GiB free with
37 GiB held by the neighbour.

| Run | Result |
|---|---|
| `20260911T123134` (full gate) | 5 C++ binaries; **`peacockdb_core_gpu_lib` 55 passed, 519 filtered out, 14.9 s; `test_gpu_corpus` 8 passed**; `GPU test run OK`, exit 0 |
| `PCK_RUN_CPP=0 PCK_TEST_FILTER=abi --run` | lib **4 passed, 570 filtered out**, all four `gpu_backend::gpu_tests::abi::`; corpus `running 0 tests`, no banner; exit 0 |
| probe, a copy with `RUST_LIB_RUNG=nomatch_tests::`, filter `abi` | `rung_args: nomatch_tests:: lists no case in peacockdb_core_gpu_lib`, `!!! … could not list its cases — it was not run`, `ran 1 rust test binaries`, **exit 1** |
| probe, a copy whose listing runs under `env LD_LIBRARY_PATH=/nonexistent`, filter `abi` | the loader's `libcudf.so: cannot open shared object file`, `!!! … could not list its cases`, **exit 1** — the reviewer's "wrong env prefix", provoked |
| `PCK_RUN_CPP=0 --run`, the real gate again | 55 + 8, exit 0 — so the host's last recorded run is green |

Both probe copies were deleted; `git status` shows nothing under `scripts/` but the five edits.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3 cargo test --features rust-only -p peacockdb-core -- --test-threads=2` | **1035 passed, 0 failed, 2 ignored**, exit 0, `warning` 0 times, 9 result lines (8 binaries + doc-tests) |
| `--test test_module_layout`, `--test test_ci_coverage` | 16 passed, 8 passed; leaf sets identical to the pre-split listing |
| `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` (`CUDF_ROOT=…/rapids-cuda-12.2`) | 0 warnings |
| `build-test-shadgpu.sh --build` | 0 warnings |
| `sha256sum` over `testdata/goldens` vs `test-layout-baselines/goldens.sha256` | empty diff, 170 files |
| `pipeline.yml` | YAML parses (7 jobs); all 29 rendered `run:` blocks `bash -n` clean |
| `bash -n` | `build-test.sh`, `build-test-shadgpu.sh`, `lib/rung-args.sh`, `lib/shadgpu-env.sh`, `case-inventory.sh`, `compare-inventory.sh`, and the rendered remote scripts (three modes; shad-gpu with and without a filter): clean |
| `rustfmt --edition 2024 --check` | `test_module_layout.rs` + six children, `test_ci_coverage.rs` + `runners.rs`: clean |
| `wc -l`, files the length rule covers | `test_module_layout.rs` 43, `near_miss` 279, `privacy` 224, `test_code` 614, `tree` 79, `visibility` 368, `walls` 328; `test_ci_coverage.rs` 647, `runners.rs` 510; `build-test.sh` 704, `build-test-shadgpu.sh` 430, `rung-args.sh` 46, `case-inventory.sh` 53, `compare-inventory.sh` 33 |
| `git grep -l '#\[test\]' -- peacockdb-core/src` outside a `test` path | 0 |
| ladders | bare `pub` excluding `mod` **242**, **200 excluding `test_support`**; `pub mod` **7**; dump 696; `PUB_MODULES` 0, `CROSS_COMPONENT_REACHES` 0, `TEST_ONLY_ITEMS` 10 — unchanged, no `src/` code moved (`lib.rs` doc only) |

Cases added: none. Pins added inside `each_reader_sees_the_violation_and_not_its_near_miss`: the
one-line `declares_mod`, `mod_declarations` and `test_gates` inputs. Scratch: `/tmp/r1/` only;
`cpp/build26/install/rust-tests/` and `cpp/install/rust-tests/` hold the staged binaries (ignored).

### 2026-09-11 — review round 2: all eleven closed, 0 blocking, 0 important, 2 nits

The reviewer re-ran its ported readers over the new tree: every round-1 finding closed, two of
them differently than proposed and soundly — finding 2 (a binary that will not load was already red
through the run status; the hole was a loaded lib whose rung listed nothing under a filter, now red
in both runners) and finding 4 (the lib entry executed through the rendered `--rust-only` gate
locally, verda being reprovisioned with the key refused; the `--gpu` and default modes of
`build-test.sh` are still unexecuted end to end). Cargo cannot pick up the split files as targets
(no `main.rs` under either directory); `workspace_test_targets()` and `rust_only_targets()` count
the same eight.

Two nits: a doc comment reassigned by round 1's insertion in `planner/tests/plan_goldens.rs`
(`digest_of`'s two lines now head `canonical_root`) — moved back by the coordinator, a comment-only
change; and `test_code.rs`'s attribute walks treating any `#[`-led line as attribute-only, which is
a false red on shapes rustfmt does not emit — the safe direction, left. Task 4 goes to `completing`.
