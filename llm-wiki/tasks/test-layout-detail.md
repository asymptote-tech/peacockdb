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
