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

Two items, both tracked by the coordinator and both to be closed before the task ends.

1. **The whole-crate `pub` narrowing** — the spec's "54 items lose `pub`". It belongs after the
   backends slice and before the layout test. `private_interfaces` is why it cannot be done per
   slice against a zero-warning baseline: narrowing `Forwarder` alone, while `pub enum
   NodeExecutors` still names it, raises the lint.
2. **Two cases the layout test owes**, both discovered here rather than designed:
   - **The `E0603` probe**, as a case rather than a command someone once ran. A component's
     implementation module must be unreachable from outside the crate, and `wire::generated` is
     the strongest instance: the whole point of the slice is that 7,336 lines are private.
   - **The nominal-visibility hole.** `private_interfaces` compares against a type's *nominal*
     visibility, not its reachable one. flatc emits `pub struct` / `pub enum`, so a type that
     no path outside `wire` can name is still nominally public, and a `pub fn` in `wire/mod.rs`
     returning one compiles silently — `FbKind::wire_kind` was exactly that. rustc cannot be
     asked this question, so the test has to ask it, and the test's own comment has to say why
     it is not redundant with the compiler. Without that sentence the next reader deletes it.

### Slice 5 — `plan`, the facade

Ready to commit. No golden moved, the case inventory is identical in both shapes, all three
builds are clean at zero warnings, and `pub use` is now **zero** crate-wide.

#### This slice is a consolidation, not a move, and the rename check will say so

Every one of the eighteen nodes loses its struct declaration and its constructor body to
`plan/mod.rs` and keeps only its `impl GpuNode` and the constructor's real body. So a per-file
similarity against the old path is low by construction — `nodes/unload.rs` retains 17% of its
text, `nodes/mod.rs` 4%, `exec_ops.rs` 25% — and `git diff -M` will report most of these as
delete-plus-add rather than as renames. That is the design and not a rewrite; the files that
did move whole score as moves (`validate.rs` 0.99, the three `tests/` files 0.98–1.00).

The aggregate check that replaces per-file similarity: over the twelve old files and the new
`plan/`, 4,447 body lines before and 4,634 after, with **50** lines present before and absent
after. Every one of the 50 is a known rewrite — `Self {` becoming `GpuX {` in a hoisted
constructor, `join::JoinFilterColumn` losing its now-redundant module prefix, `self.` becoming
`interval.`/`node.` in the two delegated `&self` bodies, and the module-path rewrites.

#### The hoist dropped 35 doc lines, and only a second check found them

The tool that moved inherent `impl` blocks into the facade started each method at its `fn`
line, so the `///` block and any `#[…]` above it were left behind — twelve doc comments and two
`#[allow(clippy::too_many_arguments)]`. Nothing goes red for that: the build is clean, the
tests pass, and a line-level comparison that filters comments out (which the first one did)
reports full conservation. It was caught by comparing doc and attribute lines specifically,
old against new. All 35 are restored; the one that is deliberately gone is
`GpuNode::as_any`'s link to `nodes::as_node_ref`, which now reads `[`as_node_ref`]`.

**Worth carrying into the remaining slices:** a body-line comparison is not enough to prove a
hoist lost nothing. Compare doc and attribute lines as their own set.

#### What the one-expression rule cost

Seventeen inherent methods have bodies of more than one statement, so their bodies became free
functions the facade delegates to in one line: fifteen constructors, `RowInterval::range_of`
(now `plan/interval.rs`) and `GpuLoadParquet::largest_batch_bytes`. The other 28 inherent
methods are one expression and keep their bodies in the facade — which is why `plan/mod.rs` is
1,301 lines rather than the spec's estimated 980. `mod.rs` has no length limit in this task, and
a check confirms the rule holds: no inherent method and no free function in `plan/mod.rs` has a
statement-level `;` at depth zero.

#### Files the spec's tree does not list

- **`plan/interval.rs`** — `RowInterval::range_of`'s three statements.
- **`plan/error.rs`** — `PlanError`'s two trait impls. A trait impl belongs in an
  implementation module and `PlanError` had no other home.
- `plan/expr.rs` and `plan/schema.rs` are **gone**: everything in them was a declaration, so
  nothing was left behind. Their module docs are section comments in the facade, above the
  declarations they describe.
- **`plan/common.rs`** is exactly the spec's five functions plus `direction`, the private
  helper one of them uses.

#### The executor-to-planner edge is gone

`validate.rs` moving into `plan/` removes the edge the spec names at what was
`driver/partitioned.rs:28`: the driver now calls `crate::plan::check_canonical_form`, a
component API, rather than reaching into the planner's validation pass.

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
| `pub use` / `pub(super)` | 0 and 7, the seven all in `batch_partitioned/{cpu_backend,gpu_backend,translate}` |
| bare `pub` outside a `mod.rs` | none in `plan/`, `wire/`, `executor/` or `plan_text/` |
| doc and attribute lines | conserved, after the repair above |

`test_cost_model`'s `node_kind_names` reads `node_name`'s source text; its `include_str!` now
points at `plan/mod.rs` and its panic message names that file.

Visibility snapshot: `visibility-after-slice5.txt`, `visibility-items-after-slice5.txt`.

## The defect class: documentation a hoist leaves behind

Recorded as a class, not as an incident, because every check this task runs was green while it
was happening.

**What it is.** Moving an item into a facade means moving its declaration. A tool that starts
at the `fn` or `struct` line takes the declaration and leaves the `///` block and any `#[…]`
above it in the file it came from — which is then deleted. Thirty-five lines went that way in
the `plan` slice: twelve doc comments and two `#[allow(clippy::too_many_arguments)]`.

**Why nothing went red.** The build was clean, 437 lib tests passed, the goldens were
byte-identical, the case inventory matched, the visibility sweep was clean, and a body-line
conservation check reported full conservation — because it filtered comments out, which is what
a body-line check is for. A dropped doc comment changes no behaviour and no artifact. There is
nothing in the suite that can see it.

**The check that does see it.** `module-layout-baselines/doc-attr-check.py` emits every
sentence of documentation and every attribute in `peacockdb-core/src`, for a revision or for
the working tree, and a slice is compared with `comm -23`. Three things about its shape were
learned by getting them wrong first:

- **The unit is the sentence, not the line.** Prose that moves into a narrower indent gets
  rewrapped, so a line-level comparison reports the whole paragraph as deleted. At line level
  the `plan` slice showed 23 differences, of which the 12 real ones were indistinguishable
  from the noise.
- **The marker is stripped.** A module header (`//!`) that becomes an item doc (`///`) or a
  section comment (`//`) is the same prose in a better place, and a comparison that keeps the
  marker calls it a loss.
- **The file list for the working tree is the tree, not the index.** Half of what this task
  moves is untracked and half of what the index still lists is gone from disk, so
  `git ls-files --cached` reads neither state.

**Run it on every slice.** It is a per-slice check beside the goldens and the inventory. Its
output is a list of candidates a person reads, not a pass/fail: a deliberate rewording is
indistinguishable from a loss to any tool, and this task does a lot of deliberate rewording.

### The retroactive pass over slices 3, 4 and 5

Run against `b14757f9`, the commit before slice 3. **2,616 sentences then, 2,687 now, 30
absent** — and all thirty are accounted for:

| Count | Kind |
|---|---|
| 8 | `enforcer` → `accountant` / `ResidentAccountant`, which the spec asks for |
| 4 | doc links to paths that no longer exist (`super::nodes::as_node_ref`, two `super::super::nodes::aggregate::…`, `see backend.rs`) |
| 3 | `this mode's tree` / `the mode's expression IR` / `this mode decides` — task 1's wording, dropped on the way past |
| 1 | `#[allow(unused_imports, dead_code, clippy::all)]`, now the inner `#![allow(…)]` in `wire/generated.rs` |
| 14 | module headers that became item docs or section comments, differing only in a `[`Type`]:` prefix or a shortened clause |

Two were genuine drops and are restored: `Backend`'s "Backend choice is a turbofish at the
entry point, not a selector consulted per node", and `GpuNode`'s "what a plan node offers the
driver and the validator". Nine more sentences of real prose were restored in the same pass —
`Batch`'s ownership-by-move rationale, `GpuBatch`'s handle-and-`Drop` rationale, `CpuBatch`'s
one-liner, the executor contracts' typestate paragraph, `PlanError`'s plan-time rationale, the
recipe vocabulary's one-liner and the node-family section comment — all of which slices 3, 4
and 5 had folded away when they replaced seven module headers with one component header.

### Slice 6 — `planner`, and the three-deep nesting

Ready to commit. No golden moved, the case inventory is identical in both shapes, all three
builds are clean at zero warnings, and the doc-and-attribute check finds exactly one loss,
which is deliberate (below).

#### The subcomponent walls hold, and the sweep says so

`grep` for each subcomponent from outside its parent returns nothing:

- `scan_mapping` is named only from `planner/translator/**`. It is the three-deep nesting the
  spec allows, and the spec's reason survives contact: all three entry points have exactly one
  caller, `Translator::source`, so as a peer of `translator` it would be the design's only
  subcomponent-to-subcomponent edge.
- `translator`, `memory_estimation`, `nulls` and `pipeline` are named only from
  `planner/**`.

Two edges had to be redirected to make that true, both in test code and both one line:

- `plan_text`'s tests built a tree with `Translator::new(…).translate(…)`. A test in another
  component cannot reach a subcomponent, so `planner/mod.rs` gains `translate()` and the test
  calls that.
- `memory_estimation`'s own tests did the same, which would have been a *sibling*-subcomponent
  edge — the one the spec claims does not exist. Same fix, same entry point.
- `expr_physical`'s tests named `expr_translate::translate_expr`; `planner/mod.rs` gains
  `translate_expr()`.

All three facade entry points are `#[cfg(test)]`, and say so in a comment: their only callers
outside the translator are tests, and without the cfg a plain `cargo build` reports them dead.

#### `Translator`'s methods become free functions

`translator/mod.rs` is a subcomponent facade, so its bodies are one expression. `Translator`
carried 23 inherent methods, 18 of them the translation itself. Five are the subcomponent's API
(`new`, `with_source_targets`, `with_small_table_bytes`, `sources_reached`, `translate`) and
stay; the other 18 became free functions taking `t: &Translator`, in `translator/nodes.rs`
(the per-kind arms), `translator/common.rs` (the shared ordinal helpers) and
`translator/aggregate.rs`. `translator/mod.rs` is 118 lines against the spec's "nothing else
passes 400".

The conversion has one hazard worth writing down: **a parameter or a `let` binding can shadow
the function it now calls.** `node()` has `if let Some(sort) = …` and then calls `sort(t, sort)`;
`aggregate()`'s parameter was named `aggregate`. rustc catches every instance as
`E0618: expected function, found &SortExec`, so none can survive a build — but the error points
at the *definition* of the shadowed function, not at the call, which makes it read like a
different bug than it is. Two were fixed by qualifying the call (`self::sort(t, sort)`) and one
by renaming the parameter to `exec`.

#### `all_row_groups` was dead, and only the wall revealed it

It had no caller at `HEAD` either. It was `pub` in a crate-root module, so `dead_code` could
not fire; private inside `scan_mapping` it goes red immediately. Deleted rather than given an
`#[allow]` that would claim it is used. Two consequences the next reader should know:

- It is the one entry in this slice's doc-and-attribute check, and the only prose lost.
- It carried one of the residue gate's deliberate survivors — `gpu_rowgroup_prune.rs:151`'s
  `scan-batch→partition` mapping site. **The gate now lands on six rather than seven, and the
  composition is not the one the spec predicted**: three mapping sites rather than four, plus
  `README.md`, `source.py` and `test_ci_coverage.rs:431`. The count matching the spec's expected
  six is a coincidence of two changes, not the finish state.

#### `gpu_rowgroup_prune` loses its `gpu_`

`src/gpu_rowgroup_prune.rs` is `planner/translator/scan_mapping/rowgroup_prune.rs`. It runs on
the CPU and serves both backends, as the spec says. `partitioner.rs` becomes `partition.rs` in
the same directory, since `scan_mapping::partitioner::partition` reads worse than
`scan_mapping::partition::partition`.

#### `plan_batch_partitioned` → `planner::plan`, and what that shadows

The rename collides with the local binding `plan` that eight call sites hold for the DataFusion
physical plan: `plan(&plan, knobs)` resolves to the binding. Every instance is an `E0618` and
so cannot ship, but **two of the eight were in files only the cudf build compiles**
(`test_gpu_recipe_walk.rs`, `test_gpu_abi.rs`) — the third time in this task that a green
rust-only build hid a cudf-only break. The call sites are now `planner::plan(&plan, …)`.

#### The comparison scripts both went red on something real

- `compare-inventory.sh` reported DRIFTED. The cause was the normalizer, not a test: it dropped
  the module path down to the last `::tests::`, and `schema_tests` does not match that. Every
  case name and count was unchanged. Fixed to take any segment ending in `tests`, and the
  suffix is still unique across all 437.
- `doc-attr-check.py` found the one deletion and nothing else, which is what it is for.

#### Body-line conservation

Over the eleven old files and the new `planner/`: 3,255 body lines before, 3,302 after, **118**
absent. All 118 are known rewrites — `&self,` becoming `t: &Translator,` and `self.x(` becoming
`x(t, ` across the 18 converted methods, `pub fn` narrowing to `pub(crate) fn`, the three test
helpers moving to `translate()`, the `plan_batch_partitioned` rename, and the eight lines of
`all_row_groups`.

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
| doc and attribute sentences | one absent, the deleted dead function's |
| residue gate | six lines — see above, the composition changed |
| `pub use` / `pub(super)` | 0 and 6, the six all in `batch_partitioned/{cpu,gpu}_backend` |
| bare `pub` outside a `mod.rs` | none in any moved component |
| subcomponent reach | nothing outside `translator` names `scan_mapping`; nothing outside `planner` names its subcomponents |

Visibility snapshot: `visibility-after-slice6.txt`, `visibility-items-after-slice6.txt`.
