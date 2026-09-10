# module-layout — run detail

Branch `ENS-module-layout`, forked off `ENS-drop-mode-name` at 787c1e5c. PR targets
`ENS-drop-mode-name`, not master.

## Facts a restarted coordinator needs

- There is no `module-layout-impl.md`. This chain does not use one; the spec is the plan, and
  its "Where everything goes" table plus the per-commit order in "Validation" are what the
  developer works from.
- Task 1 (`drop-mode-name`) is `done`, PR #141 open against master, checks green. Its head is
  this branch's base.
- The spec's validation bar is the whole task: no golden may move after the quarantined
  `GpuHashJoin` commit, and the `--list` case inventory must come back byte-identical.

## Baselines — taken before the first move

They live in `llm-wiki/tasks/module-layout-baselines/`, with the scripts that took them, so a
restarted developer compares rather than re-derives. The directory is task scaffolding and is
deleted with this detail file when the task is archived.

| File | What | Command |
|---|---|---|
| `goldens.sha256` | 170 files under `testdata/goldens/` | `find testdata/goldens -type f \| sort \| xargs sha256sum` |
| `inv-rust-only.txt` | case inventory, rust-only shape | `case-inventory.sh rust-only` |
| `inv-cudf.txt` | case inventory, cudf shape | `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 case-inventory.sh cudf` |
| `visibility.txt` | every `pub`/`pub(...)` item with its declaring file | `visibility-dump.py peacockdb-core/src` |
| `visibility-items.txt` | the same, file and visibility dropped — the move-invariant set | `visibility-dump.py --items peacockdb-core/src` |
| `residue-gate.sh` | task 1's gate, `src` exclusion dropped, `--untracked`, strip-and-rematch | run it |

Digest of the golden list: `27eac51d87ed7a0e418f01893b5930c49048ce604349052ce686197e63615650`.

### The case inventory is byte-identical only after normalization, and the spec says otherwise

The spec asks for `--list` to come back byte-identical. That holds for the eighteen integration
targets, whose case names are file-scoped and do not move. It cannot hold for `--lib`: a lib case
is named by its module path, so `batch_partitioned::cpu_backend::tests::accumulate::X` becomes
`executor::cpu_backend::tests::accumulate::X` by construction. Recorded as drift, and the invariant
actually checked is one step weaker and still exact:

- 437 lib cases in both shapes, and the suffix from the last `::tests::` onward is unique across
  all 437 (verified), so the suffix set is a faithful identity for a lib case.
- Compare with `compare-inventory.sh`, which drops everything before the last `::tests::` on lib
  lines and compares the integration targets verbatim.

Per-target counts at baseline, rust-only / cudf: `--lib` 437/437, `test_ci_coverage` 7,
`test_corpus_goldens` 20, `test_cost_model` 3, `test_cpu_corpus` 448, `test_cpu_end_to_end` 26,
`test_cpu_executors` 1, `test_golden_format` 24, `test_layout_injection` 4, `test_null_analysis` 8,
`test_plan_goldens` 19, `test_planner_join_capability` 13, `test_planner_join_refusals` 10,
`test_inc2_conformance` 3/10. cudf-only: `test_gpu_abi` 4, `test_gpu_batch` 3, `test_gpu_corpus` 8,
`test_gpu_executors` 31, `test_gpu_recipe_walk` 10.

### The visibility baseline, and where the spec's counts drift

573 records. Top-level, excluding `mod` and `use`: **174** `pub` items, not the spec's 170 — the
spec's figure predates task 1. The other counts reproduce: 142 `pub` methods and associated consts
inside `impl` blocks (141 fn + 1 const) is exact, 30 `pub use`, 52 `pub mod`, 1 `pub(super) use`.
`pub(super)` items: 46 fn + 19 methods + 15 struct + 4 enum + 1 use = 85, matching the spec's 85.

### The three feature shapes, spelled out

There is no cargo shape that is "default features but not C++-linked": `peacockdb-ffi/build.rs`
runs cmake unless `rust-only` is on, so default *is* the C++-linked build. The three shapes the
spec means, and what each proves:

1. `cargo test --features rust-only -p peacockdb-core -p peacockdb --no-run` — the tier boundary.
2. `scripts/cargo-cudf.sh build -p peacockdb-core -p peacockdb` — default features, lib and bin.
3. `scripts/cargo-cudf.sh test -p peacockdb-core -p peacockdb --no-run` — default features, every
   test target, which is where the GPU test files and `GpuBatch` actually compile.

`CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2` (cuDF 25.02, gcc-12), target dir
`target-cudf-rapids-cuda-12.2`.

### Warning counts from clean builds

"Clean" is `cargo clean -p peacockdb-core -p peacockdb` — the crates this task edits. The FFI crate
is deliberately not cleaned in the cudf dir: its source does not change here and wiping its
`OUT_DIR` costs a full C++ rebuild for no extra coverage.

| Shape | Warnings |
|---|---|
| rust-only, all targets | 0 |
| default, lib+bin | 0 |
| default, all targets | 0 |

Zero in all three, so any warning at all in a later slice is a regression.

### The residue gate at baseline

`residue-gate.sh` lands on **seven** lines: the four `batch→partition` mapping sites
(`cpp/src/node_session.cpp:220`, `flatbuffers/gpu_plan.fbs:312,346`,
`peacockdb-core/src/gpu_rowgroup_prune.rs:151`), `scripts/exec_model/README.md:371` and
`scripts/exec_model/operators/source.py:3` naming `ParquetBatchPartitioner`, and
`peacockdb-core/tests/test_ci_coverage.rs:431`, which names the module in an assert message and is
this task's residue. Expected at the finish: **six**, that last one gone. Strip-and-rematch over
the excluded lines is empty at baseline; both `bp` gates are empty.

## Run log

### Round 1 — developer dispatched
Dispatched the developer with the spec as its working document.

#### Baselines taken
All four, above. Two findings worth carrying: the `--lib` half of the case-inventory baseline
cannot be byte-identical across this task, and the spec's 170-item count is 174 on this head.

### Slice 2 — the `GpuHashJoin` rename, quarantined

Ready to commit alone. It is the only slice whose diff touches `testdata/goldens/`.

**Two `GpuJoin` types, and a blind sed breaks one.** The plan node is
`nodes::join::GpuJoin`; the GPU *executor* is `gpu_backend::join::GpuJoin`, and the spec keeps the
executor category names. So the rename skipped `gpu_backend/join.rs` and `gpu_backend/backend.rs`
whole, and in `tests/test_gpu_executors/join.rs` — the one file naming both — restored line 12's
`gpu_backend::join::GpuJoin as GpuJoinExec` after the sweep. Twelve `GpuJoin` sites survive on
purpose; they are all the executor.

**`testdata/cost_model.conf` is a rename site the spec does not list.** Its taxonomy line
`cuda_hash_join_bytes 1.0 GpuJoin` is matched against node names at runtime, so without it
`test_cost_model` fails both cases and 35 of `test_cpu_corpus`'s 448 fail with `node type
'GpuHashJoin' is not in the cost taxonomy`. Found by running the corpus, not by grep — it is
outside `testdata/goldens/`, which is where the spec's attention is.

Four import blocks needed re-sorting after the sed (`GpuHashJoin` sorts before `GpuInterleave`,
where `GpuJoin` sorted after): `cpu_backend/tests/backend.rs`, `driver/plans.rs`,
`translate/mod.rs`, `tests/common/rebuild.rs`. rustfmt cannot be run on a `mod.rs` here — it
follows `mod` declarations and would reformat the whole component — so they were rewrapped by hand
at rustfmt's 100-column fill.

**Wiki carried in the same commit**: `architecture.md` (6 refs), `tickets.md` (3 — #152 and #136
headers, anchors untouched), `scripts/exec_model/README.md` (1). `llm-wiki/archive/` deliberately
left: it records what things were called at the time. `tasks/test-layout.md`'s `GpuJoin` is the
executor and is correct as it stands.

#### The parquet is not in this worktree

`testdata/tpch.sf1` and `testdata/tpcds.sf1` are untracked in the primary checkout and a worktree
does not carry untracked files, so every golden-driven test needs a root that has both. Rather than
create untracked directories in the worktree, where a `git add` could sweep them into a commit,
point `PEACOCK_TESTDATA_DIR` at a scratch directory of symlinks — one per entry of this worktree's
`testdata/`, plus the two parquet directories from `/media/data/peacockdb/testdata/`. Golden writes
follow the `goldens` symlink back into the worktree, so a regeneration lands where `git diff` reads
it.

#### Evidence

| Check | Result |
|---|---|
| `--lib` | 437 passed |
| `test_plan_goldens`, verify | 19 passed |
| `test_plan_goldens`, `UPDATE_CANONICAL=1` (never `PEACOCK_REWRITE_RECIPE_BYTES`) | 19 passed, and the 170 golden digests are unchanged by the regeneration — the sed is confirmed, not authored |
| `test_cpu_corpus` | 448 passed, no golden moved |
| `test_cost_model` / `test_corpus_goldens` / `test_golden_format` / `test_ci_coverage` | 3 / 20 / 24 / 7 passed |
| three builds | 0 warnings in each, against a baseline of 0 |
| case inventory, both shapes | identical |
| visibility items | the intended three-line delta and nothing else: `top struct GpuJoin` → `top struct GpuHashJoin`, and the `nodes/mod.rs` re-export |
| residue gate | seven lines, the same seven as at baseline |

Post-rename comparands for later slices: `goldens-after-rename.sha256` (digest
`e071580a018c62145a19ac258b6b883758f00e2edc5cf589f87e10941c235d21`),
`visibility-after-rename.txt`, `visibility-items-after-rename.txt`.

### Rebase, at the human's word through the control file

`ENS-drop-mode-name` rebased onto `origin/master` (25 commits, no conflict), then
`ENS-module-layout` onto it (3 commits, no conflict). Both force-pushed.

What master carried across: `llm-wiki/prompts.md`, five `.claude/agents/*.md`, and a new
`scripts/start_helper.sh` that nothing references. Nothing a build or a test reads moved, so
by the documentation-only rule the rebase re-verifies nothing — task 1 stays `done`, task 2
stays `building`, and no proving command is re-run for the rebase itself.

Instruction-set changes this brought that affect the run: the coordinator now reads
`build-test.md` and `architecture.md` at startup; `architecture.md`'s falsified sentences are
named by the analyst at the completeness pass rather than corrected as the code changes;
`.github/workflows/*.yml` is the developer's now, not the coordinator's; and a verda check
belongs before each dispatch.

### verda is not usable this run

`ssh verda` failed on a changed host key. `build-test.md`'s documented remedy —
`ssh-keygen -R` plus a re-keyscan — got past that and then hit
`Permission denied (publickey)`: the box was reprovisioned and our key is not on it. Falling
back to local CPU runs, which that page says is fine. The human has to re-key verda before
any dispatch can use it.

### Slice 3 — `plan_text` and `executor` (with `driver` inside it)

Ready to commit. No golden moved, the case inventory is identical in both shapes, and all three
builds are clean at zero warnings.

#### What the slice contains, and why it is bigger than "move `driver/`"

`driver` cannot become `executor/driver` without an `executor/mod.rs` for it to hang from, and
what has to be declared there pulls the rest of the component in with it. So the slice moves the
whole of `executor/` except `cpu_backend/` and `gpu_backend/`, which are the backends slice.
`batch_partitioned/{executor,backend,batch,cpu_batch,gpu_batch,forwarder}.rs` are gone: their
declarations are in `executor/mod.rs` and their trait impls in implementation modules beside it.
`error.rs` is split as the spec's table says — `RunError` and `When` to `executor/mod.rs`,
`PlanError` left behind until the `plan` slice.

#### `PlanIndex` being component API drags three more types up with it

The spec puts `RunReport`, `PlanIndex` and `ROOT` in `executor/mod.rs` because `plan_text/run_text.rs`
walks the report through the driver's own index. `PlanIndex` has `pub` fields typed `IndexedNode`
and `PlanShape`, and `PlanShape` has a `Vec<JoinShape>`, so all three have to be declared in
`executor/mod.rs` too — a parent cannot name a type that lives inside a private child module.
`JoinShape` and `PlanShape` are the scheduler's vocabulary and read oddly at the component's
surface. The alternative is to stop `run_text` using `PlanIndex`, which is an API redesign this
task is not allowed to make. Recorded rather than fixed.

#### The one-expression rule costs four delegations

`mod.rs` bodies are one expression, so the four inherent methods whose bodies are two statements
delegate to a free function in an implementation module: `RowRange::clamp` → `row_range::clamp`,
`GpuBatch::consume` → `gpu_batch::consume`, `PlanIndex::build` → `driver::build_index` →
`index::build`, `PlanIndex::slot` → `driver::slot_of` → `index::slot`. The last two are two hops
because `driver` is a subcomponent: `executor/mod.rs` may only reach it through `driver/mod.rs`.

#### Visibility levels are preserved, and narrowing is a separate pass

Every item keeps the level it had, except `pub(super)` → `pub(crate)` (45 sites in
`driver/{mock,plans}.rs`) and the 30 `pub use` that are inlined. The spec's "54 items lose `pub`"
is deliberately **not** done per slice: `Forwarder` is one of the 54 and appears in `pub enum
NodeExecutors`, so narrowing it alone raises `private_interfaces` against a zero-warning baseline.
It wants one pass over the whole crate once every component has moved. Left undone at the end of
the task this would be a gap, so it is listed here as owed work.

#### rustfmt on a `mod.rs` reformats the whole component

`coding-style.md` already says a `mod.rs` is not one file for formatting. Method used instead: copy
`src/` to a scratch tree, run rustfmt there, and take the result wholesale only for the files this
slice authored, and only the leading `use` run for files where nothing but imports changed. The
alternative — formatting the tree — reformats `fb_text.rs` and `recipes.rs` bodies that predate the
installed rustfmt, which would both bury the diff and break rename detection.

#### Rename detection has to be done by hand here

`git diff -M --summary` cannot report a rename whose new path is untracked, and staging to make it
visible would mutate the index. A similarity pass over (deleted, untracked) pairs stands in: every
moved file scores 0.65 or better against its new path. The three that pair with nothing —
`backend.rs`, `batch.rs`, `executor.rs` — are the files whose declarations folded into
`executor/mod.rs`, which is the intended shape and not a rewrite.

#### `ResidentAccountant` has its name back

"the enforcer" and "resident enforcer" are gone from `architecture.md` (6), `tickets.md` (2) and
five code comments. `llm-wiki/archive/` keeps them: it records what things were called at the time.

#### The trap the rust-only build cannot see

`test_gpu_abi.rs` named `batch_partitioned::GpuBatch` and `executor/mod.rs` kept a
`ManuallyDrop` import that only the cudf shape compiles. Both were invisible to a green rust-only
build and both were caught by shape 3 — which is the spec's reason for insisting on three builds.

#### Evidence

| Check | Result |
|---|---|
| `--lib` | 437 passed |
| `test_plan_goldens` | 19 passed |
| `test_cpu_corpus` | 448 passed |
| `test_cpu_end_to_end` | 24 passed, 2 ignored |
| `test_ci_coverage` / `test_corpus_goldens` / `test_cost_model` / `test_golden_format` | 7 / 20 / 3 / 24 passed |
| `test_layout_injection` / `test_null_analysis` / `test_planner_join_{capability,refusals}` / `test_cpu_executors` | 4 / 8 / 13 / 10 / 1 passed |
| three builds | 0 warnings each |
| goldens | byte-identical to `goldens-after-rename.sha256` |
| case inventory, both shapes | identical |
| residue gate | the same seven lines |
| `pub use` / `pub(super)` in `executor/` and `plan_text/` | none; 18 and 35 remain, all in components not yet moved |

Visibility snapshot for the next slice: `visibility-after-slice3.txt`,
`visibility-items-after-slice3.txt`.

#### The parquet root, again

Every golden-driven run needs `PEACOCK_TESTDATA_DIR` pointed at the scratch symlink root described
under slice 2 — this worktree has no `testdata/tpch.sf1`. verda is unreachable this run (its host
key changed and the box no longer takes our key), so all of the above ran locally.

### Slice 4 — `wire`, and `generated` behind the wall

Ready to commit. No golden moved, the case inventory is identical in both shapes, all three
builds are clean at zero warnings, and the flatc surface is now unreachable from outside the
component — proven by the compiler, not asserted:

```
$ rustc --edition 2024 --crate-type lib --extern peacockdb_core=<rlib> probe.rs
error[E0603]: module `generated` is private
```

where `probe.rs` names `peacockdb_core::wire::generated::peacock::plan::PlanNodeKind`.

#### What moved

All eleven files that name flatc's output, exactly as the spec predicted: nine from `recipe/`
plus `plan_text/{fb_text,recipes}.rs`, which therefore move a second time — slice 3 carried them
up with `plan_text` and this slice puts them where they belong. `lib.rs`'s `pub mod generated`
becomes `wire/generated.rs`, declared `mod generated;`.

Two files the spec's tree does not list, both forced:

- **`wire/attach.rs`.** `recipe/mod.rs` held `attach_recipes` plus `walk`, `emit` and fifteen
  per-node arms, none of them one-expression bodies. `wire/mod.rs` is declarations and one-line
  delegations, so the walk needs an implementation module and `recipes.rs` is taken by the
  renderer.
- **`wire/serialize.rs`**, which is `recipe/wire.rs` renamed. Left alone it would be
  `wire::wire`, and its three functions serialize scalars, types and schemas.

`recipe/types.rs` is gone: its vocabulary is `wire/mod.rs` and its two `Display` impls are in
`recipes.rs`, which is the renderer they feed. `wire/mod.rs` is 354 lines against the spec's
estimate of about 300.

#### `generated.rs` flattens one level

The include goes straight into `wire/generated.rs` rather than into a nested
`gpu_plan_generated` module, so the path is `generated::peacock::plan` and not
`generated::gpu_plan_generated::peacock::plan`. All eleven import lines were being rewritten
anyway, and the extra level bought nothing once the module was private. The
`#[allow(unused_imports, dead_code, clippy::all)]` is kept as an inner attribute with a comment
saying why it stops being cosmetic: while the module was `pub` in `lib.rs` everything was
externally reachable and `dead_code` could not fire; private to one component, every generated
type the crate does not name is dead code.

#### `node_at` and `payload_text` stay inside the wall, against the spec's list

The spec lists both among what `wire/mod.rs` exposes. Both return or take flatc types
(`fb::PlanNode`), so declaring them `pub` in `mod.rs` would put a type from the private module
into the component's public signature — which is the one thing making `generated` private is
for. Nothing outside `wire` names either: their only caller is `recipes.rs`, which is now
inside. So both are `pub(crate)` in their implementation modules. Recorded as a deliberate
departure.

`FbKind::wire_kind` is the one item declared in `wire/mod.rs` whose signature names a type from
the private module. It is narrowed from `pub` to `pub(crate)`; its two callers are both inside
`wire`. Note that rustc does **not** warn here — `private_interfaces` reads the type's nominal
visibility, and flatc emits `pub`, so an unreachable-but-nominally-public type passes silently.
The check has to be made by reading, which is why the layout test in the next slice should carry
it.

#### The rust-only build cannot see a broken `gpu_backend`

A path rewrite turned `use super::super::recipe::…` into `use super::crate::wire::…` in five
`gpu_backend` files. Shape 1 was green — those files are `#[cfg(not(feature = "rust-only"))]` —
and shape 2 failed with `E0433: crate in paths can only be used in start position`. Second time
this slice sequence that a cudf-only break got through a green rust-only build.

#### A slice-3 defect fixed here

`plan_text/{expr_text,run_text}.rs` shipped in slice 3 with a mis-ordered `use` block: the
helper that reordered them took a file's *first* contiguous `use` run rather than the one that
changed, and those two files have three runs. Both are rustfmt-ordered now, and the helper used
in this slice takes the whole span from the first `use` to the last.

#### Evidence

| Check | Result |
|---|---|
| `--lib` | 437 passed |
| `test_plan_goldens` | 19 passed |
| `test_cpu_corpus` | 448 passed |
| `test_cpu_end_to_end` | 24 passed, 2 ignored |
| `test_ci_coverage` / `test_corpus_goldens` / `test_cost_model` / `test_golden_format` | 7 / 20 / 3 / 24 passed |
| `test_layout_injection` / `test_null_analysis` / `test_planner_join_{capability,refusals}` / `test_cpu_executors` | 4 / 8 / 13 / 10 / 1 passed |
| three builds | 0 warnings each |
| goldens | byte-identical to `goldens-after-rename.sha256` |
| case inventory, both shapes | identical |
| residue gate | the same seven lines |
| `generated` reachability | `E0603` from outside the crate |
| `pub use` / `pub(super)` | 15 and 8, all in `batch_partitioned/{nodes,translate,cpu_backend}` and the crate root |
| rename detection | every moved file scores 0.89 or better against its new path |

Visibility snapshot: `visibility-after-slice4.txt`, `visibility-items-after-slice4.txt`.

#### Owed work, carried forward

The whole-crate `pub` narrowing (the spec's "54 items lose `pub`") is still owed, and belongs
after the backends slice and before the layout test. `private_interfaces` is why it cannot be
done per slice against a zero-warning baseline.
