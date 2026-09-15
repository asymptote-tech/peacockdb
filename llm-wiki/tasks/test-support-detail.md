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

### 2026-09-11 — reviewing: PR #147 against `ENS-test-layout`

Plan task 3 green in the fresh window (entry above). Committed as `83e90c4f`, pushed, PR #147
opened against `ENS-test-layout` — base verified, 5 commits, the branch alone. The cudf shape
is CI's to build on this PR. Reviewer round 1 dispatched next.

### 2026-09-11 — review round 1: 0 blocking, 2 important, 5 nits

Reviewer's reading of PR #147. The move is behaviour-neutral (every non-mechanical change
checked against `git show ENS-test-layout:…`), the facade is real, coverage unchanged, rungs
clean. Findings:

- **important, `privacy.rs`** — the rule scans a `pub` declaration up to its `{` and `pub`
  field lines, so three spellings pass green: a `pub enum` variant payload
  (`Planned(Box<dyn crate::plan::GpuNode>)`), a `pub trait`'s method signatures, and a private
  alias laundering the type (`type Tree = Box<dyn crate::plan::GpuNode>; pub fn tree() -> Tree`).
  None reachable today (`mod.rs` has no private `type`, both `pub enum`s fieldless, no
  `pub trait`). The reviewer's Python simulation showed `pub type`, `pub use`, generic bounds,
  multi-line `where`, an `impl` block's `pub fn`, `impl Trait` returns, fn-pointer params and a
  tuple struct's `pub` field all go red. Fix: extend the capture to the matching `}` for
  `pub enum`/`pub trait`, check a private `type` alias's right-hand side in `mod.rs`, and pin
  the composition in `near_miss.rs` on a fixture holding the probe and these three.
  → developer.
- **important, `coding-style.md:162-164` and `privacy.rs:88-90`** — "takes and returns
  strings, paths, its own types or nothing" is an allow-list the code does not satisfy
  (`owed_rows(&[RecordBatch])`, `take_rows(&mut HashMap<String, usize>, …)`, `rel_tol:
  Option<f64>`, `wanted_rows(u64, u64, Option<u64>)`); the spec states the rule by what it
  forbids. Page fixed by the coordinator; the assertion message → developer.
- nit, `privacy.rs:131,187` — `bound_name("self")` binds the literal `self` for
  `use crate::planner::{self, …}`, so a violation spelled `planner::PlanKnobs` is reported as
  "names `self`" against every `&self`. Loud but misleading. → developer, same file.
- nit, `build-test.md:7` — header 1577/1142 → 1578/1143 (the layout row is 17). Fixed.
- nit, `build-test.md:355` — "`over_cap` takes strings": it takes `Option<usize>` and `&Mode`
  and is read by `test_corpus_goldens`. Fixed.
- nit — stale comments: `test_cost_model.rs:4` named `common/cost_model.rs`,
  `test_corpus_goldens.rs:679` said the gpu binary links through `mod common`. Fixed.
- nit — file-top comments over the ten-line cap, carried from the originals:
  `cost_model.rs` 12, `corpus_gpu.rs` 11. Trimmed to 10 each, rustfmt clean.

For the signoff: the spec's "`corpus_golden.rs`, `result_text.rs` and `cost_model.rs` stay in
`tests/common/` untouched" could not hold — `corpus.rs` calls into both and `src/` cannot see
`tests/` — so the deviation is forced, and the reviewer asks that the signoff name it.

### 2026-09-11 — round 1 fixes: the rule reads bodies and aliases, and says what it forbids

Code half of round 1, on `2e25b738`. Two files, both in `tests/test_module_layout/`:
`privacy.rs` and `near_miss.rs`. No other file; `git diff --stat -- src/test_support/mod.rs` is
empty after the probes below. **`test_module_layout` stays at 17 cases** — every pin went into
the one near-miss test, as the first entry's did — so the inventory expectation is unchanged.

**Important 1, the reach.** First the finding was confirmed on the tree: with the three
spellings appended to `mod.rs` — `pub enum Outcome { Planned(Box<dyn crate::plan::GpuNode>) }`,
`pub trait Probe { fn tree(&self) -> Box<dyn crate::plan::GpuNode>; }`, and
`type Tree = Box<dyn crate::plan::GpuNode>;` behind `pub fn laundered() -> Tree` — the rule
ran green (`1 passed`). Then:

- `pub_declarations` is now a thin call on `declarations(text, starts)`, and for a head line
  that is `pub enum` or `pub trait` the capture runs to the `}` that closes the body, counted
  by braces, rather than to the first `{`; a `;` inside a trait body no longer ends it. The
  private-module rule shares the reader, so it reads enum and trait bodies in every component
  `mod.rs` now too, and stays green on the tree. A trait's default method *bodies* are inside
  that capture — a false positive there would be loud, not silent, and none exists.
- `type_aliases(text)`: every `type` or `pub(…) type` alias, wrapped or not, read as a
  declaration whose right-hand side is checked. Chosen over refusing a private `type` outright
  because it is the same rule applied to one more declaration shape rather than a new
  prohibition — an alias over std or harness types stays allowed — and because checking every
  alias catches an alias of an alias at its root. A bare `pub type` is excluded there, being a
  declaration already.
- `component_types_on_the_surface(text, components)` is the composed scan — imports, `pub`
  declarations with bodies, `pub` fields, aliases, `code_only`, `signature_only`,
  `component_type_in` — returning `(line, head, name)` sorted, and the test formats it. The
  composition is pinned in `near_miss.rs` on a fixture holding the spec's probe plus the three
  spellings (expects exactly four hits at lines 1, 4, 7, 10) and on the same four shapes over
  std, arrow and harness types plus a `pub(crate)` field and `MODES`'s initializer (expects
  none). Per-reader pins beside it: an enum body ends at its own brace and carries the payload;
  a trait's `;` does not end it; `type_aliases` sees a private and a `pub(crate)` alias, wrapped,
  and not a `pub type` or a `let`.

**Important 2, the message.** `privacy.rs`'s assertion now states the prohibition, matching the
corrected `coding-style.md` bullet: "No parameter, return or `pub` field names a component's
type; std, arrow and the harness's own types are what remain. The engine type stays behind a
pub(crate) body."

**Nit, `self`.** In `component_imports`, a group member that binds to `self` now pushes the
component's name (`{self as p, …}` still binds `p`, via `bound_name`). Pinned: `use
crate::planner::{self, BatchSizing};` binds `["planner", "BatchSizing"]`; `pub fn knobs(&self)
-> planner::PlanKnobs {` is attributed to `planner`; `pub fn knobs(&self) -> usize {` names
nothing.

| Run | Result |
|---|---|
| The three spellings appended to `mod.rs`, before any change | **green** (`1 passed`) — the finding, confirmed |
| Pins added to `near_miss.rs`, readers not yet written | `E0432: unresolved imports component_types_on_the_surface, type_aliases` |
| Scan factored and `type_aliases` added, `self` and bodies not yet fixed | red at the first pin: ``assertion `left == right` failed: `self` in a group is the module it sits under / left: ["self", "BatchSizing"] / right: ["planner", "BatchSizing"]`` |
| `self` and body capture fixed, the three spellings still in `mod.rs` | near-miss **ok**; the rule **red**: ``test_support/mod.rs:564: pub enum Outcome { names `crate::plan::GpuNode` `` / ``mod.rs:568: pub trait Probe { names `crate::plan::GpuNode` `` / ``mod.rs:572: type Tree = Box<dyn crate::plan::GpuNode>; names `crate::plan::GpuNode` `` — 16 passed, 1 failed; `no_public_signature_names_a_type_from_a_private_module` ok with bodies read |
| `use crate::planner::{self, …}` plus `pub fn probe_knobs(&self) -> planner::PlanKnobs` on `impl Mode` (a first try as a free fn was not Rust: `self` parameter is only allowed in associated functions) | red, once: ``mod.rs:131: pub fn probe_knobs(&self) -> planner::PlanKnobs { names `planner` `` — not "names `self`" against every receiver |
| `mod.rs` restored from a copy; `git diff --stat -- src/test_support/mod.rs` | empty |
| `cargo test --features rust-only -p peacockdb-core --test test_module_layout` | **17 passed**, 0 failed, 0 warnings |
| `cargo test --features rust-only -p peacockdb-core --test test_ci_coverage` | **8 passed**, 0 warnings |
| `rustfmt --edition 2024 --check` on `privacy.rs`, `near_miss.rs` | clean (applied once: it re-wrapped three asserts and the `if`) |
| Comment caps on both files | longest doc block 10 (`no_public_signature…`'s, pre-existing; `pub_declarations`'s trimmed from 11 back to 10), longest body comment 3 |
| `git status --short` | the two test files and this one |

Not re-run: the rust-only package (nothing outside the layout test's two files changed), the
cudf shapes, the device cycle.

### 2026-09-11 — review round 2: 0 blocking, 0 important, 3 nits — the round closes

Every round-1 item verified closed on `95a9ff31`; the reviewer re-ran its Python model of the
new readers and the private-module rule with the body-reading capture over every `mod.rs`
under `src/`: no false positive. Nits:

- `declarations` (`privacy.rs:318`) counts braces over the raw line, so a `}` inside a
  variant's doc comment ends a `pub enum`/`pub trait` body early and the lines below go unread
  — silent. Fix: count over `code_only(lines[i])`. Not reachable today; **taken anyway**, one
  line and a pin, because a silent hole in this guard is the class the task exists to close.
- A `pub(crate) type` alias in a submodule reached by `use corpus::Tree;` in `mod.rs` binds a
  name neither `component_imports` (reads `crate::` only) nor `type_aliases` (reads `mod.rs`
  only) sees. Outside the spec's stated scan; no `type` line exists under `test_support/`.
  Dropped.
- Trait default method bodies now sit inside the private-module rule's capture, so a default
  body in a component `mod.rs` calling into a private module reads as a signature — loud, and
  none exists. Dropped.

### 2026-09-11 — control file said `rebase`: the chain moves onto master `02069415`

Master carried three commits past `188c23ce`, the chain's base: `3ab7047c` (reports, docs only),
`62335cff` (`cpp/CMakeLists.txt`: `cudf_ROOT` also seeds `CMAKE_PREFIX_PATH`, so cuVS and Arrow
are found on a dev host) and `02069415` (`scripts/build-test-shadgpu.sh`, `scripts/setup-glibc.sh`:
the patch step follows the build host's glibc). Not documentation alone, so it re-verifies. It
is also why the human asked now: this box was reprovisioned to Ubuntu 24.04 / glibc 2.39 (the
worktree dates from 21:43 UTC, the timezone in the watchdog log flipped from -07:00 to +00:00),
and without `02069415` a binary built here dies at load on shad-gpu after a patch to 2.35.

Branches with code: `ENS-rmm-pool-budget` (task 3, done, PR #144 → master), `ENS-test-layout`
(task 4, done, PR #145), `ENS-test-support` (task 5, reviewing, PR #147). Files master changed
that a branch also changed: `build-test.md` on all three (wiki, coordinator's), and
`scripts/build-test-shadgpu.sh` on task 4 (code, developer's). `tasks.md` differs from master's
only in the states and PR numbers of tasks 3-5, so every board conflict resolves to the branch.
Order: task 3 rebased and carried back to `done` (developer re-runs its shad-gpu cycle — which is
also the first device cycle from this host — then CI green on the rebased PR), then task 4 onto
it, then task 5. Marks written on this board before anything moves.

### 2026-09-12 — tasks 3 and 4 are done again; task 5 rebased onto task 4's new tip `1bcf4e97`

Task 3: rebased clean, the developer's shad-gpu cycle from this box green (52 + 63, patched to
2.39), CI attempt 1 lost the GPU job to a neighbour at 14.9 GiB free (#178 dated), attempt 2
green on `d41f223a`. Task 4: one conflict in `build-test-shadgpu.sh`, resolved additively by the
coordinator (master's `BUILD_GLIBC` block above slice 9's `RUST_TESTS` block; `bash -n` clean,
the developer read the composition and called it right), then 1035 on the package and
52 + 55 + 8 through the merged script; CI green on `df56c6fc` first attempt. This branch then
rebased clean onto `1bcf4e97`: 10 commits, non-documentation differences from `3e211d10` are
master's three files alone. `rebase needed(reviewing)` until the developer re-runs this task's
proof — the rust-only package, the layout test, the device cycle — and the cudf `--no-run` shape
plan task 3 left to CI, now that `target-cudf-rapids-cuda-12.2` is warm.

### 2026-09-12 — re-proven after the rebase: the package, the goldens, the cudf shape, the device

Re-proof of `50654da1` from dev (Ubuntu 24.04, glibc 2.39), right after task 4's cycle on the same
worktree, so every target dir was warm and each build was incremental. No code file changed; this
entry is the only edit. No `PEACOCK_TESTDATA_DIR`, `UPDATE_CANONICAL` or `PCK_*` variable set.
Nothing heavy ran concurrently.

| Command | rc | Result |
|---|---|---|
| `cargo test --features rust-only -p peacockdb-core --test test_module_layout` | 0 | **17 passed**, 0 warnings |
| `--test test_ci_coverage` | 0 | 8 passed, 0 warnings |
| `cargo test --features rust-only -p peacockdb-core -- --test-threads=2`, background under `timeout 3600`, 2-minute monitor | 0 | **1036 passed, 0 failed, 2 ignored**, `warning` 0 times, 9 result lines: `--lib` 514 + 2 ignored (89 s), ci_coverage 8, corpus_goldens 20, cost_model 3, cpu_corpus 448 (188 s), golden_format 26, gpu_corpus 0, module_layout **17**, doc-tests 0; 365 s wall |
| `find testdata/goldens -type f -print0 \| LC_ALL=C sort -z \| xargs -0 sha256sum \| diff - …/goldens.sha256`, after the suite | 0 | empty |
| `CUDF_ROOT=…/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --no-run` | 0 | 0 warnings, 50 s; eight executables listed (`lib`, `test_ci_coverage`, `test_corpus_goldens`, `test_cost_model`, `test_cpu_corpus`, `test_golden_format`, `test_gpu_corpus`, `test_module_layout`) — the shape plan task 3 left to CI |
| `scripts/build-test-shadgpu.sh --build` | 0 | 37 s, 0 warnings; `test_gpu_corpus` and `peacockdb_core_gpu_lib` staged |
| `--push-binaries` | 0 | 11 s; both rust binaries shipped |
| `--patch` | 0 | 10 s; `glibc 2.39 already installed in /home/info/glibc-2.39; skipping the build`; `Verified: every shipped executable uses /home/info/glibc-2.39/lib/ld-linux-x86-64.so.2` |
| `nvidia-smi` before the run | — | 37043 MiB used of 143771, 106041 free |
| `--run-detached` | 0 | pid 1916198 |
| `--run-status`, run `20260912T005457-135989` | 0 | `FINISHED, exit code 0` at the first 2-minute poll; `GPU test run OK` |

On the host: `peacock_cpu_tests` 11, `peacock_gpu_tests` 6, `peacock_plan_tests` 27,
`peacock_tpch_tests` 4, `peacock_tpchv_tests` 4 — 52, `ran 5 C++ test binaries`. Pool lines
`1.0`, `1.0`, `69.0 GiB reserved of 103.0 GiB free` and `30.0 GiB reserved of 100.6 GiB free`; no
`could not be built`, no `Maximum pool size exceeded`, so #178 gets no line. **`peacockdb_core_gpu_lib`
55 passed, 519 filtered out, 21.36 s**, all 55 `test` lines under `gpu_tests::`.
**`test_gpu_corpus` 8 passed, 10.14 s** — the binary this task rewired through `gpu_case`:
`a_device_run_under_a_regeneration_writes_no_golden`, `gpu_tpch_q19_tp1_single`,
`gpu_tpch_q6_tp1_{single,rowgroup}`, `gpu_tpch_q6_tp4_{single,rowgroup,sized}`,
`the_registry_matches_the_gpu_corpus_in_both_directions`. `GLIBC_2` appears nowhere in the
345-line gate log. Scratch under `/tmp/t5-*` only; `git status --short` shows this file alone.

### 2026-09-12 — round 2 nit: brackets are counted over code, not prose

On `951d907a`, after the rebase. `declarations` in `privacy.rs` counted `{`/`}` over the raw
line and applied `code_only` only to the captured text, so a `}` in a variant's doc comment
closed a `pub enum`/`pub trait` body early and every variant below it went unread — silent,
since a short capture reports nothing. One line: the bracket loop now runs over
`code_only(lines[i])`. Pinned in `near_miss.rs` beside the four-spelling fixture, on the
reviewer's probe `pub enum E {\n    /// like `}` in prose\n    A(Box<dyn crate::plan::GpuNode>),\n}`.

| Run | Result |
|---|---|
| Pin added, reader unchanged | red: ``assertion `left == right` failed: the body runs to the brace that is code / left: [] / right: [(0, "crate::plan::GpuNode")]`` |
| `code_only` inside the loop | `cargo test --features rust-only -p peacockdb-core --test test_module_layout` → **17 passed**, 0 failed, 0 warnings; no new case |
| `rustfmt --edition 2024 --check` on both files | clean (one re-wrap of the fixture's `let`) |
| `git status --short` | `privacy.rs`, `near_miss.rs`, this file |

### 2026-09-12 — completing: no blocking or important finding outstanding

Round 2 closed on `95a9ff31`; the rebase re-proof is green on `50654da1`; the brace nit is
closed with a pin. Board to `completing`. The completeness pass is two readings dispatched
together: the reviewer (what is wrong) and a fresh analyst (what is missing, and which
`architecture.md` sentences the branch falsified), neither seeing the other's list.

### 2026-09-12 — completeness pass, the analyst's reading

Read on `a3cf60ad` against `ENS-test-layout` at `1bcf4e97`: the 21-file diff as one change, the
spec's Validation and Done-when, `architecture.md` entire, `build-test.md`, `coding-style.md`,
`visibility.md`, the board. Read-only; nothing built. The reviewer's list was not seen.

**0 blocking, 0 important. `architecture.md`: none falsified.**

What was checked, and how:

- **Consumers.** `mod common` is declared by `test_golden_format` and `test_cost_model` alone,
  and the shim's six re-exports are exactly the names those two use. No moved item survives in
  two places: `assert_sorted_str_approx`, `GpuResultMode`, `gpu_result_mode`,
  `RESULT_GOLDEN_MAX_BYTES`, `Regeneration`, `SKIPPED`, `CostModel` and `Category` are each
  declared once in the crate. `read_back` is deleted and had no caller.
- **Reachability.** Every path a case, an oracle or a golden takes is intact: `cpu_case` →
  `assert_answer` (three oracle keywords, exhaustive match) → `assert_or_merge` →
  `regeneration()` / `merge_section`; `gpu_case` → `assert_section` (never writes) →
  `assert_result` (five keywords, exhaustive). `corpus_gpu` keeps the `not(rust-only)` gate it
  had; the lock in `merge_section` is std's `File::lock`, so no dev-only crate rode into the
  library. The 8 device cases and the 448 cpu cases ran on this code (entries above).
- **The facade.** Nothing under `test_support/` outside `mod.rs` is bare `pub`; no `pub use`,
  no macro. Every bare `pub` in `mod.rs` is over std, arrow, `RecordBatch` or a harness type;
  `Mode::sizing` and `Mode::knobs` are `pub(crate)`, so `BatchSizing`/`PlanKnobs` cannot be
  reached through `Mode`. The guard reads `components()` off `lib.rs`, which includes `common`,
  so it is stricter than the spec's five. The eight and `RecipePlan`'s two methods: zero code
  hits under `tests/`, zero hits in `peacockdb`, `peacockdb-ffi` and `cost-report` — the
  `coding-style.md` sentence "nothing outside the crate names them any more" is true. The one
  road left from a binary to an engine type is the crate's own `pub mod` facades, where the
  eight are still `pub`; `visibility.md` claims that demotion as its own.
- **Done-when.** Each item has evidence in this file — the E0433 probe, the workflow grep,
  both registry assertions, 1037 → 1038 by the one rule, 170 goldens identical, the guard red
  on the spec's probe and on the three further spellings — except CI, in progress on
  `a3cf60ad` at the time of reading (cost-report, which carries the layout rule, green).
- **`build-test.md`.** The header's 1578/1143 is the +1 the rule adds; the `test-support`
  paragraph and the layout-rule row cover what the branch added. No wiki page names a moved
  file by its old path except the dated reports, which pin line numbers at a master SHA.
- **`architecture.md`.** The paths it names and the thirteen code files the branch changed do
  not intersect. Every sentence about tests describes content the branch did not touch:
  "Targeted unit tests are the coverage; the plan goldens are not" (Planning); "A naive rescan
  survives as a test-only oracle" (The scheduling rule); `plan_executor_internal.h` and
  `GpuWorker` / `WorkerPool` (Interfaces); "the divergence surfaces only at the root, for a
  query whose corpus line names a result golden or an oracle" (What guards it, and what does
  not); `cpp/tests/gpu/test_multi_gpu_*` (Multi-GPU notes); "Peacock cost is a re-reading of
  the execution golden" and `testdata/cost_model.conf` (Cost model and the DuckDB oracle). "The
  Rust side's own traits — `Backend`, the executor families, `GpuNode` — are in Execution above"
  (Interfaces) says where they are declared, not that they are `pub`.

For the signoff, two spec sentences the branch does not satisfy, both by design and both
already in this file:

- "`corpus_golden.rs`, `result_text.rs` and `cost_model.rs` … stay in `tests/common/`
  untouched" — `corpus.rs` calls both and `src/` cannot see `tests/`, so 1097 lines moved, not
  698.
- "the eight items stop being `pub` as a result" (twice in the spec) — they stay `pub`. The
  branch keeps the 200 invariant and leaves the demotion to task 6, whose spec claims it; the
  board prose says the eight stop *needing* `pub`, which is what the branch does.

One residue with no owner: `tests/common/mod.rs` is now a 31-line `pub use` shim for two
suites. Retiring it is six `use` lines in `test_golden_format.rs` and `test_cost_model.rs`;
neither this spec nor `visibility.md`'s carry-over list names it.

Below the bar, comment-only, the coordinator's to apply if it wishes: `testdata/cost_model.conf:2`
still says "Parsed by tests/common/cost_model.rs" (the class round 1's `test_cost_model.rs:4`
nit fixed), and `peacockdb-core/Cargo.toml`'s `inventory` comment says "see
`common::assert_registry_matches_csv`", a name that resolves in no spelling now that the
`common::registry` re-export is gone.

### 2026-09-12 — completeness pass, the reviewer's reading: 0 blocking, 0 important

Read on `a3cf60ad` without the analyst's list. Every spec claim re-checked against the tree: the
facade (whole-word grep for the eight, `wire_nodes`, `bytes` — only the rule's doc comment and
fixtures), all 63 `pub` declarations and 34 `pub` fields in `mod.rs` over std/arrow/harness types,
a Python port of the rule red on both probe spellings and on the pre-narrowing tree, the feature
gate and the self dev-dependency, no workflow flag, both registry assertions, the 170-file
goldens digest, static `#[test]` counts per binary against the inventory baseline (layout 17 vs
16, the one rule), the visibility dump (263 / 200 / 7, records outside `test_support`
byte-identical), token-normalised bodies of every moved helper against the parent, comment caps
counted, both wiki pages' sentences true. Round-2 closures re-checked and the dropped shapes
agreed with. The deviation list for the signoff confirmed and extended (the 22 facade items, the
`knobs`/`sizing` narrowing, the shim).

### 2026-09-12 — completeness approved

Both readings closed with nothing to apply; `architecture.md` has no falsified sentence. Two
comment-only residues the analyst named below the bar are fixed by the coordinator:
`testdata/cost_model.conf:2` and `peacockdb-core/Cargo.toml:46` now point at `test_support`.
Signoff appended to the spec. `done` waits on CI for the head that carries these two lines.

### 2026-09-12 — done

CI run `34664010380` on `60b4844a`, the head that carries code, green on every job: both cuDF
legs, the 25.02 GPU build, the remote device tier, cost report, changed paths, S3 metadata.
Task 5 is `done`; PR #147 awaits the human's merge, after task 4's #145.
