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
