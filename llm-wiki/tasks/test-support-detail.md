# test-support — run detail

Spec: [`test-support.md`](test-support.md). Plan: [`test-support-impl.md`](test-support-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 5. Branch `ENS-test-support`, forked at `2a3245df`, the tip of
task 4's branch. **Its PR targets `ENS-test-layout`**, not master: task 4 is `done` but not merged,
so it is the parent that exists.

## How this task is dispatched

Two dispatches: plan tasks 1-2 together (the baselines are five commands and the move is 698
lines), then plan task 3 in a fresh window, so the proof is re-measured by someone who did not
make the move. Each ends by appending here.

## Hosts at dispatch (2026-09-11)

- **verda**: answers but refuses the key (`Permission denied (publickey)`) — reprovisioned; local
  runs for the CPU shapes.
- **shad-gpu** up, `llm-gpu0h200`, a neighbour holding 37 GiB of the 143.7. Needed only if the
  device corpus binary's path changes — it does, since `corpus_gpu.rs` moves, so one cycle at the
  end proves `test_gpu_corpus` still runs its 8.
- `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` on this host.

## What task 4 left for this task

- The eight forced items: `GpuNode`, `validate` (`plan/mod.rs`), `RecipePlan`, `attach_recipes`
  (`wire/mod.rs`), `RunReport`, `GpuBackend`, `GpuContext` (`executor/mod.rs`), `render_run`
  (`plan_text/mod.rs`) — plus, task 4's completeness reading found, two methods on one of them
  (`RecipePlan::wire_nodes`, `RecipePlan::bytes`, named by `corpus_gpu.rs`).
- Bare `pub` excluding `mod`: 242, 200 outside `test_support`. `pub mod`: 7. Registers: 0 / 0.
  `TEST_ONLY_ITEMS`: 8.
- `tests/common/mod.rs` re-exports the moved harness from `peacockdb_core::test_support` with
  `pub use`; `corpus.rs`, `corpus_gpu.rs`, `corpus_golden.rs`, `result_text.rs`, `cost_model.rs`
  and `corpus_cases.inc` are what remain there.
- Rust-only package: 1035 passed, 2 ignored. `--lib` 514. Inventories 1037 / 1048 / 574.

## Run log

### 2026-09-11 — plan tasks 1-2 dispatched: baselines, and the move

Board set to `building`.

### 2026-09-11 — plan tasks 1-2 done: the baselines, and the corpus harness inside the crate

Plan task 1 steps 1-4 and plan task 2 steps 1-7. Not committed. Nine files created, four deleted,
nine modified:

- Created `llm-wiki/tasks/test-support-baselines/{inv-rust-only,inv-cudf,inv-gpu}.txt`,
  `visibility.txt`, `goldens.sha256`; and `peacockdb-core/src/test_support/{corpus,corpus_gpu,corpus_golden,cost_model}.rs`.
- Deleted `peacockdb-core/tests/common/{corpus,corpus_gpu,corpus_golden,cost_model}.rs`.
- Modified `peacockdb-core/src/test_support/mod.rs` (400 → 562 lines), `tests/common/mod.rs`
  (170 → 31), `tests/{test_cpu_corpus,test_gpu_corpus,test_corpus_goldens}.rs`,
  `tests/test_module_layout/{privacy,near_miss}.rs`, `llm-wiki/coding-style.md`,
  `llm-wiki/build-test.md`.

#### Plan task 1: the baselines, and the count

Taken on task 4's tip before any edit, into `test-support-baselines/`: `inv-rust-only.txt` 1037
cases (`--lib` 516, `test_cpu_corpus` 448, `test_corpus_goldens` 20, `test_golden_format` 26,
`test_module_layout` 16, `test_ci_coverage` 8, `test_cost_model` 3, `test_gpu_corpus` 0),
`inv-cudf.txt` 1048 (`--lib` 519, `test_gpu_corpus` 8, the rest as above), `inv-gpu.txt` 574
(`--lib` alone), `goldens.sha256` 170 files, `visibility.txt` 696 records.

**The bare-`pub` count is 242, and 200 outside `test_support`.** The plan's step 3 says "around
174"; that figure predates task 4, which added the `test_support` component's 42 declarations
and moved nothing production. The measurement is right and the plan's estimate is stale. The
number this task must not change is the **200**: task 6's ladder starts from it.

#### The move, and the two files the spec says stay that could not

`corpus.rs` and `corpus_gpu.rs` moved as the spec says. So did `corpus_golden.rs` and
`cost_model.rs`, which the spec says "name zero crate items and are read only by binaries that
stay, so they stay in `tests/common/` untouched" — the second half is false: `corpus.rs` calls
`corpus_golden::assert_or_merge` and the three path builders in `cpu_case`, and
`CostModel::load().cost_text_from_cpu` to derive the cost golden; `corpus_gpu.rs` calls
`assert_section` and `section_of`. A file in `src/` cannot reach `tests/common/`, and a copy on
each side is the drift the feature exists to stop, so both came along — the same finding task 4's
slice 3 made about `result_text.rs`, for the same reason: a helper cannot stay behind when a
thing that calls it leaves. Three items of `tests/common/mod.rs` followed for the same reason:
`RESULT_GOLDEN_MAX_BYTES` (now a `pub(crate) const` in `corpus.rs`), and `GpuResultMode`,
`gpu_result_mode` and `assert_sorted_str_approx` (private in `corpus_gpu.rs`, the only reader of
all three; the comparator was ungated under `#![allow(dead_code)]` before and would have been a
`dead_code` warning under `rust-only` anywhere ungated inside the crate).

The shape is slice 3's: every module private, every item `pub(crate)`, and what crosses out is
declared in `mod.rs` — a one-expression delegate per function, the type with its inherent `impl`
beside it. `CostModel`'s four methods became free functions in `cost_model.rs` taking `&CostModel`,
with `load`, `cost_text_from_cpu` and `cost_text_from_sections` delegating from the `impl` in
`mod.rs`; `parse` has no external caller and is `pub(crate)` only. `SKIPPED` and `Regeneration`
moved to `mod.rs` outright, since a `const` and a fieldless `enum` are declarations. `read_back`
was deleted: its doc said only `corpus_golden`'s own tests need it, and none exist — inside the
crate it was a `dead_code` warning, and its one `use` (`Seek`) went with it.

#### The API `mod.rs` now declares, and who reads each

21 new bare `pub` items, all with signatures over strings, paths, `RecordBatch` slices, `u64`s,
`HashMap`/`HashSet<String>` and the harness's own types:

| Items | Read by |
|---|---|
| `cpu_case`, `authoritative_mode` | `test_cpu_corpus` |
| `gpu_case` (gated `not(feature = "rust-only")`) | `test_gpu_corpus` |
| `over_cap` | `test_corpus_goldens` |
| `SKIPPED`, `Regeneration`, `cpu_golden`, `cost_golden`, `result_golden`, `section_of`, `assert_section`, `merge_section`, `merged_text` | the three above |
| `Category`, `CostModel` with `load`, `cost_text_from_cpu`, `cost_text_from_sections` | `test_cost_model` |
| `wanted_rows`, `owed_rows`, `take_rows`, `without_its_limit` | `test_golden_format` |

Two existing items narrowed: `Mode::knobs` (`-> PlanKnobs`) and the field `Mode::sizing`
(`BatchSizing`) are `pub(crate)`. Both were read only by `corpus.rs` and in-crate tests, and both
were the real violations the new rule found on its first run, before the probe.

`test_cpu_corpus.rs`, `test_gpu_corpus.rs` and `test_corpus_goldens.rs` no longer declare
`mod common` at all: each `use`s `peacockdb_core::test_support::{…}` directly and names none of
the eight. `tests/common/mod.rs` is re-exports only now, for `test_golden_format` and
`test_cost_model`, which this task is not about; retiring it is six `use` lines in those two
files, left for whoever next touches them.

**`corpus_cases.inc` stays at `tests/common/corpus_cases.inc`.** Where the `include!` sits is what
matters, not where the file does: `corpus_query!` is defined by each binary and expands inside
it, so `inventory::submit!` runs in that binary's crate and `inventory` collects per linked
binary. Both `include!("common/corpus_cases.inc")` lines are unchanged. The file could not go to
`src/`: nothing there compiles it, the layout test's `sources()` reads `.rs` only, and a
declaration list is the binaries' data rather than the harness's. Both registry assertions pass —
cpu here, gpu on shad-gpu — which is the property the location was chosen to keep.

#### The rule, and what it printed

`no_test_support_signature_names_a_component_type` in `test_module_layout/privacy.rs`: reads
`test_support/mod.rs` alone, takes the components off `lib.rs` (`components()`) minus
`test_support`, collects the names every `use crate::<component>…` binds (`component_imports`:
brace groups, `as` aliases, crate-root groups, wrapped statements, `*` reported as a glob), then
scans every bare-`pub` declaration (`pub_declarations`) and every `pub` field (`pub_fields`) for an
inline `crate::<component>::` path or an imported name as a whole identifier
(`component_type_in`). A `pub const`/`static` is cut at its `=` (`signature_only`), since `MODES`
spells `BatchSizing::Budgeted` in its value. The four readers are pinned in
`each_reader_sees_the_violation_and_not_its_near_miss`; the pin caught one reader bug before the
rule ever ran on the tree — an outer ` as ` split ran before the brace group was peeled, so
`{A, B as C}` bound `B` and lost `C`.

| Run | Result |
|---|---|
| First run on the unedited tree | red: ``test_support/mod.rs:132: pub fn knobs(&self) -> PlanKnobs { names `PlanKnobs` `` and ``mod.rs:122: pub sizing: BatchSizing, names `BatchSizing` `` |
| Both narrowed to `pub(crate)` | 17 passed |
| The spec's probe, `pub fn tree() -> Box<dyn crate::plan::GpuNode> { unimplemented!() }` appended | red: ``a `pub` in test_support names a type from a component, so the type is on the surface under the harness's name and the facade is a rename:`` / ``  test_support/mod.rs:402: pub fn tree() -> Box<dyn crate::plan::GpuNode> { names `crate::plan::GpuNode` `` — 16 passed, 1 failed |
| The other spelling, `use crate::plan::GpuNode;` plus `pub fn tree() -> Box<dyn GpuNode>` | red: ``mod.rs:403: pub fn tree() -> Box<dyn GpuNode> { names `GpuNode` `` |
| Reverted (file restored from a copy, `git diff --stat` shows only the two narrowings) | 17 passed |

The sentence is in `coding-style.md`'s Visibility section, under the eight's bullet, which now
says nothing outside the crate names them.

#### Everything measured, on the final tree

| Check | Result |
|---|---|
| `PEACOCK_TESTDATA_DIR=… cargo test --features rust-only -p peacockdb-core -- --test-threads=2`, the whole package | **1036 passed, 0 failed, 2 ignored**, exit 0, 0 warnings, 9 result lines — task 4 ended at 1035, and the one more is the rule |
| `… --test test_cpu_corpus` | **448 passed**, 0 failed, 508 s — 444 cells, the registry assertion and three meta cases; run twice, before and after rustfmt |
| `… --test test_corpus_goldens` | 20 passed |
| `… --test test_module_layout` | **17 passed** |
| `… --test test_cost_model`, `test_golden_format`, `test_ci_coverage` | 3, 26, 8 passed |
| `cargo build --features rust-only -p peacockdb-core` | 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --no-run` | 0 warnings |
| `scripts/cargo-cudf.sh test -p peacockdb-core --no-run` | 0 warnings, `test_gpu_corpus` built |
| `scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` | 0 warnings |
| shad-gpu, run `20260911T150035-1193155` (`--build` 0 warnings, two binaries staged; `--push-binaries`; `--patch --run-detached`; `--run-status` polled, each a foreground call under `timeout`) | `peacockdb_core_gpu_lib` **55 passed, 519 filtered out, 14.90 s**; `test_gpu_corpus` **8 passed, 7.89 s**; `GPU test run OK`, exit 0. No `[rmm] pool … could not be built`: every C++ binary reported its pool against 103 GiB free. One cycle, no re-run needed |
| `sha256sum` over `testdata/goldens` against `goldens.sha256` | identical, 170 files — checked after the corpus run |
| `scripts/case-inventory.sh rust-only` vs baseline | 1038 vs 1037: `compare-inventory.sh` → `DRIFTED`, the whole diff is `test_module_layout` 16 → 17, the one new rule; leaf-name set diff is that one name |
| `… cudf` vs baseline | 1049 vs 1048, the same one name; `test_gpu_corpus` still lists 8 |
| `… gpu` vs baseline | 574, **byte-identical** (hand `diff`; the tool still rejects `gpu`) |
| `scripts/visibility-dump.py` | 748 records vs 696; **bare `pub` excluding `mod` 263, and 200 excluding `test_support`**; `pub mod` 7; the records outside `test_support` are byte-identical to the baseline |
| `grep -rn` for the eight, `wire_nodes`, `bytes` over `peacockdb-core/tests/` | two hits, both the new rule's own doc comment naming the spec's probe (`privacy.rs:53-54`); no code line. The near-miss fixtures use `crate::plan::Expr` so the grep stays clean |
| `grep -rn 'peacockdb_core::' peacockdb-core/tests/` outside `test_support` paths | no hits outside the two guards' string fixtures |
| `lib.rs` probe `pub fn probe_test_support() -> PathBuf { crate::test_support::testdata_root() }` | `cargo build --features rust-only -p peacockdb-core` → `error[E0433]: failed to resolve: could not find test_support in the crate root`, exit 101; reverted, `git diff --stat -- lib.rs` empty |
| `grep -rn 'test-support' .github scripts` | no hits, exit 1 |
| `rustfmt --edition 2024 --check` | clean on every `src/test_support/*.rs`, `privacy.rs`, `near_miss.rs`, `tests/common/mod.rs`, `test_gpu_corpus.rs` |

Suites ran with `PEACOCK_TESTDATA_DIR=/tmp/peacock-testdata-slice3`, slice 3's composed root.
verda refuses the key; everything CPU-side ran here.

#### Where a measurement contradicts the spec or plan

- **The spec's "corpus_golden.rs, result_text.rs and cost_model.rs … stay in tests/common/
  untouched"**: two of the three moved, for the reason above; the third had already moved in
  task 4. `tests/common/` holds `mod.rs` (re-exports) and `corpus_cases.inc`.
- **"698 lines"**: 1097 moved — 508 + 190 + 281 + 118 — plus the 100 or so from
  `tests/common/mod.rs`.
- **The plan's "around 174"** for the bare-`pub` count: 242 measured, and 200 outside
  `test_support`; see task 1 above.
- **The plan's task 3 step 4, "Expected: Task 1's figure"**: the raw figure is 263, not 242,
  because `mod.rs` declares 21 facade items and lost one (`knobs`). The invariant is the
  production figure, **200 = 200**, and `visibility.txt` compared with `test_support` lines
  dropped is identical. Whoever runs task 3 should read it that way rather than as a regression.
- **"No test case moves … the counts too"**: rust-only and cudf each gain exactly one case, the
  new layout rule, which the spec itself orders. Nothing else moved in any shape.
- **The plan's step 4, "takes and returns strings, `Mode`, `MemoryLimit` or nothing"**: the rule
  is written by what it forbids, as the spec says, not that allow-list — `RecordBatch` slices,
  `PathBuf`, `HashMap<String, usize>`, `Option<&'static Mode>` and the harness's own
  `Regeneration`/`CostModel` are all in signatures the corpus tier needs.
- **`test_cpu_corpus.rs` and `test_corpus_goldens.rs` are not rustfmt-clean**, and were not at
  HEAD (4 and 5 hunks of hand-wrapped lines in code this task did not touch — the first check
  read 0 because a temp copy without `common/` beside it made rustfmt error rather than diff).
  Left in their own style as slice 10 did; the one hunk in lines this task wrote, the import block
  in `test_corpus_goldens.rs`, was matched to rustfmt's wrap by hand.

#### For plan task 3

- The `--lib` count under `--features gpu` is 574 locally and 55 device cases ran on shad-gpu
  (`gpu_tests::` filter); the eight corpus cases are the ones `corpus_gpu.rs` now serves from
  inside the crate.
- `tests/common/mod.rs` keeps `#![allow(unused_imports)]`: each of the two suites reading it uses
  a subset of its re-exports.
- The `knobs`/`sizing` narrowing means a binary outside the crate cannot build `PlanKnobs` from a
  `Mode` any more; nothing outside did.

### 2026-09-11 — stopped on the control file, after plan tasks 1-2

Plan tasks 1-2 committed as `aae9cd9b`. The control file said `stop` when the developer returned,
so the run ends here with task 5 at `building`. **Next: plan task 3** (the proof, in a fresh
window) — then commit, push, PR against `ENS-test-layout`, review. Nothing is unproven in the
tree: every check in the entry above ran on the committed state.

### 2026-09-11 — plan task 3 dispatched: the proof, in a fresh window

Fresh worktree at `47bd42bd`; the tree is code-identical to `aae9cd9b`, which the entry above
measured. Hosts: verda's hostname does not resolve, so CPU shapes run locally; shad-gpu up with
a neighbour at 37 GiB, not needed — plan task 3 has no device step and the device cycle above
ran on this code. `/tmp/peacock-testdata-slice3` is gone; `testdata/{tpch,tpcds}.sf1` are
symlinks into `/home/dmitry/peacockdb/testdata`. `target/` is warm for `rust-only`; no
`target-cudf-*` exists here, so the cudf half of step 1 is left to CI's `cpp-build-2502` and
`dataset-matrix` on the PR rather than paid as a cold opt-3 build for a shape the entry above
already built from this code. The developer runs the rust-only half of step 1, steps 2-5, and
the whole rust-only package as the handoff run.

### 2026-09-11 — plan task 3 done: the proof, in a fresh window

Re-measured on `935af8da` (code-identical to `aae9cd9b`) by a developer who did not make the
move. No code file changed; this entry is the only edit. Everything ran locally (verda's hostname
does not resolve); no `PEACOCK_TESTDATA_DIR`, `UPDATE_CANONICAL` or `PCK_*` variable was set, so
the binaries read the compile-time default root through the `testdata/{tpch,tpcds}.sf1`
symlinks, and nothing could write a golden. The cudf half of step 1 was not run, per the dispatch:
no `target-cudf-*` exists here, the entry above built it from this code with 0 warnings, and CI
builds that shape on the PR. Every check is green.

| Check | Result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --no-run` | exit 0, **0 warnings**, 1m 37s; eight executables listed, `test_gpu_corpus` among them |
| `cargo build --features rust-only -p peacockdb-core` | exit 0, **0 warnings** |
| `cargo test --features rust-only -p peacockdb-core -- --test-threads=2`, the whole package, in the background under `timeout 2700` with a 2-minute monitor | **1036 passed, 0 failed, 2 ignored**, exit 0, 0 warnings, 9 result lines, ~8 min wall: `--lib` 514 passed / 2 ignored (125 s), `test_ci_coverage` 8, `test_corpus_goldens` 20, `test_cost_model` 3, `test_cpu_corpus` **448** (307 s), `test_golden_format` 26, `test_gpu_corpus` 0, `test_module_layout` **17**, doc-tests 0 |
| `scripts/case-inventory.sh rust-only` vs `test-support-baselines/inv-rust-only.txt` | **1038 vs 1037**; `compare-inventory.sh` → `DRIFTED`, exit 1, and the unified diff is exactly two lines: `test_module_layout` `16 tests` → `17 tests` and `+ privacy::no_test_support_signature_names_a_component_type`. Per-binary: `--lib` 516, `test_cpu_corpus` 448, `test_corpus_goldens` 20, `test_golden_format` 26, `test_module_layout` 17, `test_ci_coverage` 8, `test_cost_model` 3, `test_gpu_corpus` 0 |
| Leaf-name set diff (last `::` segment, sorted unique) | 1034 → 1035, the one added name above; nothing removed |
| `sha256sum` over `testdata/goldens` vs `goldens.sha256`, **after** the suite | `diff` empty, exit 0; 170 files both sides; `git status --short testdata/` empty |
| `scripts/visibility-dump.py` | 748 records vs the baseline's 696; **bare `pub` excluding `mod`: 263 raw, 200 excluding `test_support`**; `pub mod` 7. Records outside `src/test_support/` are **byte-identical** to `visibility.txt` with its `test_support` lines dropped (`diff` exit 0, whether the filter is the substring or the path; the only record naming `test_support` outside that directory is `top pub mod test_support lib.rs`, present in both). `test_support` records: 63 → 115 |
| `grep -rnw` for the eight names and `wire_nodes` over `peacockdb-core/tests/` | exactly the two doc-comment hits, `test_module_layout/privacy.rs:53-54`; no code line |
| `grep -rnw bytes` over `peacockdb-core/tests/` | no `RecipePlan::bytes`: the `.bytes()` calls are `MemoryLimit::bytes()` in `test_golden_format.rs:492-494` (the harness's own type, `test_support/mod.rs:75`), plus the `near_miss.rs:129-130` string fixture and two prose comments. A bare substring grep also matches `output_bytes`/`batch_bytes`/`as_bytes` in five files, which is why the entry above's "two hits" is the whole-word reading |
| `grep -rn 'test-support' .github scripts` | no hits, exit 1 |
| `git status --short` | only this file |

Nothing contradicts the previous entry's figures: 1036/0/2, 448, 17, 1038 vs 1037 by the one
rule, 170 goldens identical, 263/200/7, two doc hits, no workflow flag. The package run was ~8
minutes here against the ~15 the dispatch budgeted, with the corpus binary at 307 s against 508 s
last time — the default testdata root instead of the composed `/tmp` one is the only difference
in the invocation.
