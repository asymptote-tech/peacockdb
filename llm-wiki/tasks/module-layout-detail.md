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
  `scan-batch→partition` mapping site.

**The gate must be compared by composition, never by count.** The spec predicts it finishes on
six: four `batch→partition` mapping sites plus `README.md` and `source.py`. It is on six now,
and they are not those six. Name them:

| Line | What it is |
|---|---|
| `cpp/src/node_session.cpp:220` | mapping site, deliberate |
| `flatbuffers/gpu_plan.fbs:312` | mapping site, deliberate |
| `flatbuffers/gpu_plan.fbs:346` | mapping site, deliberate |
| `scripts/exec_model/README.md:371` | `ParquetBatchPartitioner`, the structure and not the mode |
| `scripts/exec_model/operators/source.py:3` | the same |
| `peacockdb-core/tests/test_ci_coverage.rs:431` | **task 2 residue**, and it goes when `batch_partitioned` does |

The fourth mapping site is gone because it was the doc of the dead function this slice deleted,
and the seventh line is still present. Two changes cancelling to the predicted number is the
shape a reader who counts would call confirmation.

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

### Slice 7 — the backends, and `batch_partitioned/` is gone

Ready to commit. This is the last move. `peacockdb-core/src` is now `common.rs`, `lib.rs` and
six component directories, and the name `batch_partitioned` appears nowhere in the tree.

#### The gate finally reads the whole crate

Task 1's residue gate has never once read `peacockdb-core/src`: the `':!peacockdb-core/src'`
exclusion existed only to spare the directory this slice deletes. Run without it, and with the
four survivor spellings now absent so the whole-line exclusion matches nothing, it lands on
**five**, and the composition is what matters:

| Line | What it is |
|---|---|
| `cpp/src/node_session.cpp:220` | mapping site, deliberate |
| `flatbuffers/gpu_plan.fbs:312` | mapping site, deliberate |
| `flatbuffers/gpu_plan.fbs:346` | mapping site, deliberate |
| `scripts/exec_model/README.md:371` | `ParquetBatchPartitioner`, the structure and not the mode |
| `scripts/exec_model/operators/source.py:3` | the same |

`test_ci_coverage.rs:431` is gone: its assert message named "the inline `#[cfg(test)]` modules
(batch_partitioned, config)", and neither module exists. It now names the count instead.

**The general rule, because task 3 inherits this gate.** *An exclusion that outlives its
reason is a hole shaped like the project's history.* A gate is written with exclusions for the
spellings that are deliberate **at the time it is written**. Every one of those has an expiry —
the change that retires the spelling — and nothing reminds anyone. Until then the gate reads
green over exactly the text it was told to ignore, which is indistinguishable from a clean
tree. This is the same shape as the doc-comment class above: **a check that is green for a
structural reason rather than because the tree is clean.** The two together are what this task
found that no test could.

So: when a spelling is retired, delete it from the exclusion list in the same commit. And when
inheriting a gate, read its exclusions before its output.

**Three residues the gate could not see, and why.** `driver/partitioned.rs`'s module doc said
`batch_partitioned_driver` and two comments in `schema_tests.rs` said `plan_batch_partitioned`.
Both spellings are in the gate's *survivor* list — deliberate for task 1, residue for this one —
and the strip-and-rematch found nothing else on those lines, so it stayed quiet. Found by
grepping for `batch_partitioned` directly once the directory was gone. **The survivor list is
what has to shrink as each spelling is retired; a gate whose exclusions outlive their reason is
a gate with a hole in the shape of its own history.** The list now matches nothing, and is kept
rather than deleted so the next reader can see it never fires.

#### The exception this slice cannot avoid: `pub mod cpu_backend` / `pub mod gpu_backend`

`test_cpu_executors` and `test_gpu_executors` construct backend executors directly — 14 types
across `cpu_backend::{accumulate,backend,emit,join,source}` and
`gpu_backend::{accumulate,backend,emit,join}` — and a separate crate cannot reach a private
subcomponent. The rules leave three ways out and each breaks something:

| | Cost |
|---|---|
| `pub mod cpu_backend;` | breaks "a subcomponent is declared `mod`, not `pub mod`"; no code moves |
| declare the 14 types in `executor/mod.rs`, impls stay | breaks "a struct keeps its inherent `impl` in `mod.rs`"; drags `Calls`, `JoinCall`, `Stage` and `HeldBytes` up with them, as `PlanIndex` dragged `PlanShape` |
| the full hoist | breaks nothing; converts 55 multi-statement inherent methods, **23 of them `&mut self` executor state machines on the per-batch path**, and the GPU half has no test this host can run |

Taken the first. It is one line each, reversible, and its expiry is named in the spec:
`test-layout.md` moves both targets into `src/` and the exemption goes with them. The comment
in `executor/mod.rs` says so, and the layout test must list both by name rather than tolerate a
pattern. The third option is the one the rules ask for and it can be done later at leisure;
undoing a bad mechanical rewrite of the executors cannot.

Everything inside those two directories that is genuinely unreachable was still narrowed:
`spark_partitioning::rows_per_lane`, `merge_m2::{NAME, udaf}` — all in `mod`-private modules.

#### The one inventory change in the whole task

The spec says the case inventory comes back byte-identical, and it has for six slices. It does
not here, and the spec is what changes it: `config.rs` is dismantled, so its two unit tests go
with it. Measured rather than assumed:

| | Baseline | Now |
|---|---|---|
| `--lib` | 437 | 435 |
| `test_golden_format` | 24 | 26 |

The two lib cases were `config::tests::{labels_round_trip, tiers_are_strictly_increasing}`, and
both covered `MemoryLimit` as much as the deleted `TargetPartitions`. `MemoryLimit` survives, so
its coverage had to: the two cases are now `test_golden_format::{every_tier_label_round_trips,
tiers_are_strictly_increasing}`. Not beside the type in `tests/common/memory_limit.rs`, because
`common` is compiled into every integration target and the cases would multiply by eighteen.
`TargetPartitions`' own label round-trip is the only coverage genuinely gone, with the type.

Net zero cases. `inv-{rust-only,cudf}-final.txt` are the new baseline.

**A trap in reading that diff.** A `--lib` case line begins with `--lib`, so in `diff` output a
removed one reads `---lib` and an added one `+--lib` — and a `grep '^[+-][^+-]'` filter drops
every one of them. The first summary this produced showed only the `test_golden_format`
additions and looked like a pure gain. Compare with `comm` on sorted files, not by eye over a
diff.

#### `config.rs`, `memory.rs`, `spark_partitioning.rs`

- `config.rs` is gone. `MemoryLimit` is `tests/common/memory_limit.rs`, beside `mode.rs` —
  **not `bp_mode.rs`, which the spec names; task 1 renamed it.** The first draft of that file's
  header copied the spec's stale name and the `\bbp[-_]` gate caught it, which is the one time
  in this task that gate has fired.
- `memory.rs` is `src/common.rs`, the row-byte formula four components price by. It names no
  FFI type, which is what lets it compile in every shape — the breach the spec warns about
  would show as a rust-only link failure and does not.
- `spark_partitioning.rs` is `executor/cpu_backend/spark_partitioning.rs` and is now private:
  its only caller is `cpu_backend/emit.rs`.

#### `lib.rs` gains the engine's module doc

`batch_partitioned/mod.rs` carried "The engine: a lane holds a stream of batches rather than one
resident table" and the sentence naming the vocabulary. Deleting the file would have deleted
both — the doc-and-attribute check is what noticed — so `lib.rs` now opens with them, rewritten
around the six components.

#### The fourth cudf-only break did not happen

Three of the previous four slices had one. This one built clean in all three shapes first time,
which is worth recording as the thing that makes the three-build rule cheap: it is not that the
rule never pays, it is that it costs nothing when it does not.

#### Evidence

| Check | Result |
|---|---|
| `--lib` | 435 passed |
| `test_plan_goldens` | 19 passed |
| `test_cpu_corpus` | 448 passed |
| `test_cpu_end_to_end` | 24 passed, 2 ignored |
| `test_ci_coverage` / `test_corpus_goldens` / `test_cost_model` / `test_golden_format` | 7 / 20 / 3 / 26 passed |
| `test_layout_injection` / `test_null_analysis` / `test_planner_join_{capability,refusals}` / `test_cpu_executors` | 4 / 8 / 13 / 10 / 1 passed |
| three builds | 0 warnings each, first attempt |
| goldens | byte-identical to `goldens-after-rename.sha256` |
| case inventory | the one authorized change above, net zero cases |
| doc and attribute sentences | 15 absent, all accounted: 11 from the deleted `config.rs`, 3 reworded stale names, 1 carried into `lib.rs` |
| residue gate, unexcluded, whole crate | five lines, named above |
| rename detection | every moved file 0.98 or better; `config.rs` 0.49 because it was dismantled |
| `pub use` / `pub(super)` | 0 and 6, the six all inside the two exempt backend directories |
| bare `pub` outside a `mod.rs` | `lib.rs` (8, the crate's own surface), `common.rs` (2), and 25 inside the two exempt directories — nowhere else |

Snapshots: the `*-final.txt` files in the baselines directory.

#### One drift found in `build-test.md`, not introduced here

Its header claims 1,562 cases and says it is the sum of the N column. The column sums to
**1,558**, both at `accf25f0` and now — my two edits to it cancel. Pre-existing, and the spec
gives that table to `test-layout.md`, so it is reported rather than fixed.

## The `pub` narrowing

Its own commit, after every component landed and before the layout test. `pub(super)` is now
zero crate-wide and so is `pub use`.

### What left, and why

**43 items narrowed `pub` → `pub(crate)`, 6 `pub(super)` → `pub(crate)`, 4 deleted.** The full
list is the diff of `visibility-dump.py`'s output across `59b973ac`; the four deletions are
the part to read:

| Deleted | Why |
|---|---|
| `Schema::position_of` | no caller anywhere, in any shape |
| `AggregateBatches::compactions` (GPU) | no caller; the CPU twin is used by a test and kept under `#[cfg(test)]` |
| `plan::sql_name` | a facade delegation nothing called; `aggregate::sql_name` is the used one and keeps the doc |
| `plan_text::expr_text` | the same shape; `expr_text::expr_text` is the used one, and now carries the doc the facade had |

Three more items had their only callers behind a `cfg`, so they carry the same `cfg` rather
than a wider `pub` that hides the fact: `JoinCapability::makes_a_finish_pass` and
`Schema::state_for` are `#[cfg(test)]`, and `Input::is_build_side` is
`#[cfg(not(feature = "rust-only"))]` — its one caller is the GPU backend.

### Counting imports is the wrong instrument, and that is the correction to carry

**The general fact, which outlives the number: an import-based sweep cannot see a type that is
reachable through a `pub` field or a `pub` signature — and those are the types that are most
publicly reachable, not least.** `Expr::Column(ColumnRef)` is the worked example. No file
outside the crate writes `ColumnRef`; every file that matches on an `Expr` obtains one. A sweep
over imports reports it unused and is exactly wrong.

So the spec's "54 lose `pub`" is not a target and 43 is not a better one. What holds is the
method: narrow what the outside does not import, then let `private_interfaces` re-widen what it
names, and iterate to a fixpoint. The count falls out; it is not the check. A spec that gives a
number and twelve examples invites the next reader to treat the number as the check, which is
the one reading that cannot work.

The spec names twelve of the 54 explicitly. Six of those twelve **cannot** be narrowed:
`ColumnRef`, `SortOrder`, `UnaryOp`, `MemoryModel`, `JoinCapability` and `Forwarder` are all
reachable through a `pub` field or a `pub` signature — `Expr::Column(ColumnRef)`,
`PartitionLayout::sort_order`, `planner::plan`'s return, `NodeExecutors::BatchForwarder`. An
external caller obtains a value of each without any file naming the type, so an
import-based sweep says "unused" and is wrong.

`private_interfaces` is what sees this, and the method that works is a **fixpoint**: narrow
everything the outside does not import, then re-widen whatever the lint names, and repeat,
because widening one type exposes the next. It converged in two rounds and re-widened
eighteen types.

`narrow.py` and `external-names.py` in this directory are the two halves, and both are
deliberately conservative: a method name is kept if the identifier appears *anywhere* outside
the crate, because `.bytes()` at a call site carries no path to match on. Over-keeping is a
`pub` that should have narrowed; under-keeping is a broken build, and the three shapes report
that.

### The fourth cudf-only break, and it was the fixpoint itself

This is one of four instances of the same thing, and the family is worth more to the next
reader than the instances. **A check can be green because of its own structure rather than
because the tree is clean.** Four in one task, none of them found by a test:

| The check | Why it was green |
|---|---|
| proving a guard red, by grepping the run for `panicked at` | the violation did not build, so nothing panicked and nothing ran |
| body-line conservation over a hoist | it filters comments out, so 35 dropped doc lines conserved perfectly |
| task 1's residue gate | its exclusion list still held the four spellings this task retires |
| the `private_interfaces` fixpoint | run under `rust-only`, which does not compile the GPU backend at all |

The first is the sharpest, and it was found while proving the other guards: a `pub use`
violation that collided with an existing name failed to *build*, so the test never ran, and a
grep for the expected failure signature found nothing — which reads exactly like "the rule did
not fire". A check of a check, green because the run never happened.

The shape is the same each time: the check's own structure — what it filters, what it excludes,
what shape it runs in, whether it ran at all — decides its answer before the tree does. None of
the four could go red. The counter is the same too: **read the check's structure before its
output**; where the structure has a scope, run it in every scope the tree has; and read the
exit status rather than grepping for the signature you expect.


The first fixpoint ran under `--features rust-only`, where the GPU backend does not exist, so
the lint could not see `Collapse`, `GpuSource` or `GpuProbingJoin`. The cudf build then failed
with `E0446: crate-private type GpuSource in public interface`. **A lint-driven fixpoint is only
as complete as the shapes it is run in** — it has to converge in the shape that compiles the
conditional half, and `E0446` is an error there rather than a warning, which is the only reason
it could not have shipped.

The same shape produced the fourth dead item: the GPU `AggregateBatches::compactions` is dead
only in the cudf build, and `--features rust-only` is silent about it.

### A splitter artefact in the doc check, worth knowing before it is mistaken for a loss

`doc-attr-check.py` cuts on `.` or `:` followed by a capital or a backtick. Deleting the
`sql_name` facade left its doc reported as two absent sentences even though the identical prose
sits on `aggregate::sql_name` — the two copies were wrapped at different columns, so the colon
fell inside a line in one and at a line end in the other, and one copy split where the other did
not. **Compare the text, not the count.**

### Evidence

| Check | Result |
|---|---|
| `--lib` | 435 passed |
| `test_plan_goldens` | 19 passed |
| `test_cpu_corpus` | 448 passed |
| `test_cpu_end_to_end` | 24 passed, 2 ignored |
| the seven cheap tiers | 7 / 20 / 3 / 26 / 4 / 8 / 13 / 10 / 1 passed |
| three builds | 0 warnings each |
| goldens | byte-identical |
| case inventory, both shapes | identical — nothing moved, nothing was added |
| doc and attribute sentences | one splitter artefact, no prose lost |
| residue gate | the same five lines |
| `pub use` / `pub(super)` | 0 and 0 |

Snapshots: the `*-final.txt` files in the baselines directory. The full before/after for this
commit is the diff between the branch's `59b973ac^` and `59b973ac` trees.

## The layout test

`peacockdb-core/tests/test_module_layout.rs`, its own commit, wired into pipeline.yml's CPU
tier beside `test_ci_coverage`. Ten cases. It reads the committed tree and compiles one probe
against the built library; no dataset, no device.

### Every rule was watched red

The spec asks for this and it is the only way to know a guard is wired up. Each violation was
constructed in the tree, the test run, the message read, and the tree restored.

| Rule | The violation | What it said |
|---|---|---|
| `pub_mod_declares_a_component_and_nothing_else` | `pub mod translator;` in `planner/mod.rs` | "`pub mod` outside lib.rs makes a subcomponent nameable crate-wide, and the wall it was given then exists only on paper: planner/mod.rs declares `pub mod translator;`" |
| `a_components_api_is_declared_in_its_mod_rs` | `pub fn new_project` in `plan/exec_ops.rs` | "these items are `pub` outside a mod.rs, so they are component API nothing declared: plan/exec_ops.rs:110" |
| `nothing_re_exports_with_pub_use` | `pub use std::fmt::Debug;` in `wire/mod.rs` | "`pub use` is not allowed — inline the declaration into mod.rs … wire/mod.rs:19" |
| `nothing_is_pub_super` | `pub(super) fn input_layout` in `plan/common.rs` | "`pub(super)` is not a level this layout uses … plan/common.rs:9" |
| `no_subcomponent_reaches_a_sibling` | `use super::translator::Translator` in `planner/memory_estimation/mod.rs` | "a subcomponent reaches a sibling, which nothing in rustc refuses: planner/memory_estimation/mod.rs names `super::translator::`" |
| `no_super_path_climbs_out_of_its_component` | `use super::super::plan_text::…` in `plan/exec_ops.rs` | "these `super::` chains leave their own component: plan/exec_ops.rs:7 … climbs 2 from depth 1" |
| `no_public_signature_names_a_type_from_a_private_module` | `pub fn wire_kind` in `wire/mod.rs` | "a `pub` item names a type from a module private to its own component, so the type escapes by inference even though no path can reach it: wire/mod.rs:117" |
| `a_private_module_is_unreachable_from_outside_the_crate` | `pub mod generated;` in `wire/mod.rs` | "`wire::generated` compiled from outside the crate. flatc's 7,336 lines are supposed to be private to one component" |
| `every_pub_mod_exemption_still_has_the_target_that_forces_it` | renamed a target in the exemption | "these `pub mod` exemptions have outlived the targets that forced them … The subcomponent can be `mod` again" |
| `each_reader_sees_the_violation_and_not_its_near_miss` | made `pub_mod_declarations` a naive `contains` | "assertion failed: pub_mod_declarations(\"    pub modelled: usize,\").is_empty()" |

**Two of those violations compiled with zero warnings**, which is the case for the test
existing at all:

- `use super::translator::Translator` from a sibling subcomponent. `pub(super)`, `pub(crate)`
  and `pub(in path)` all give a subcomponent's siblings the same access as its parent, and
  there is no level meaning "my parent but not my siblings". rustc accepted it; only this test
  refused it.
- `pub fn wire_kind(&self) -> fb::PlanNodeKind`. Confirmed by building it deliberately:
  `cargo build` said **nothing**. `private_interfaces` compares against a type's *nominal*
  visibility, and flatc emits `pub`, so a type no path outside `wire` can name is nominally
  public. The test's comment says this, because a reader who assumes the compiler covers it
  will delete the test as redundant.

### One near-miss the readers had to be built around

`executor/mod.rs` has `pub modelled: usize` — a field of `Underestimate`. A
`contains("pub mod")` reader counts it as a subcomponent declaration. The reader matches at a
word boundary and requires the `;`, and `each_reader_sees_the_violation_and_not_its_near_miss`
pins that with the real line.

### The `E0603` probe, and why it needs a control

It compiles a two-line snippet against the `peacockdb_core` rlib beside the running test
binary — `current_exe().parent()`, not a hardcoded path, because this crate is built into
`target/` and `target-cudf-*` and a guess would read the wrong build or none.

A probe that fails for the wrong reason — wrong rlib, missing `-L` — is indistinguishable from
a probe that proved something. So the negative case is preceded by a positive control that
must compile (`wire::Recipe`), and the failure is matched on `E0603` and `generated` rather
than on "it failed".

### A hazard met while proving the rules red

The first `pub use` violation was `pub use attach::attach_recipes;`, which collides with the
`pub fn attach_recipes` in the same file: the **build** failed with `E0255`, the test never
ran, and grepping the output for `panicked at` found nothing — which read as "the rule did not
fire". It is the family again, one level further in: *a check of a check can be green because
the run never happened*. Read the exit status, not the expected signature. The violation that
proves the rule is `pub use std::fmt::Debug;`, which compiles.

### The exemption has a mechanical expiry

`PUB_SUBCOMPONENTS` lists `executor/cpu_backend` and `executor/gpu_backend` with the targets
that force them. The field is not decoration: `every_pub_mod_exemption_still_has_the_target_that
_forces_it` asserts each named target exists, so when `test-layout.md` moves them into `src/`
the test goes red and says the wall can go up. An exemption without a verified claim is the
hole this whole family of findings is about.

### Evidence

| Check | Result |
|---|---|
| `test_module_layout` | 10 passed |
| every rule | seen red, table above |
| `test_ci_coverage` | 7 passed — it demanded the new target be wired, and accepts it now |
| pipeline.yml | parses as YAML; the rendered `run:` block passes `bash -n` |
| `--lib` / `test_plan_goldens` | 435 / 19 passed |
| three builds | 0 warnings each |
| goldens | byte-identical |
| case inventory | the ten new cases and nothing else |
| doc and attribute sentences | nothing absent |
| residue gate | the same five lines |

## The full suite, once, at the end

Local. verda still refuses our key (reprovisioned without it), and `build-test.md` says a local
CPU run is fine.

**What a package-wide command actually sweeps.** `cargo test --features rust-only -p
peacockdb-core -p peacockdb` builds and runs **21 binaries**, not a tier — the whole CPU
execution suite, the meta tier and the golden tier together. **1,031 cases passed, 0 failed, 2
ignored** (the two `#[ignore]`d against #182, both pre-existing). `cargo test -p cost-report`
adds 37.

**Six of the 21 ran nothing, and that is not coverage:**

| Binary | Why it ran zero cases |
|---|---|
| `unittests src/main.rs` | the CLI has no tests; `test_ci_coverage` asserts CI at least builds it |
| `test_gpu_abi`, `test_gpu_batch`, `test_gpu_corpus`, `test_gpu_executors`, `test_gpu_recipe_walk` | file-gated on `not(rust-only)`, so under this build they compile to empty binaries and pass |

`test_inc2_conformance` ran 3 of its 10 for the same reason. A binary that runs zero tests
reports `ok`, which is why the count per binary is in the table above rather than a single
total.

**So the GPU half of this branch is proved by three clean builds and by nothing else.** No host
this run could reach has a device. What that does and does not cover:

- Covered: every GPU path compiles in the shape that links cuDF, including the conditionally
  present half of `executor/`'s API, and every GPU test target still compiles against the moved
  types. Four cudf-only breaks were caught this way across the task and none reached a commit.
- Not covered: that the GPU backend still *behaves* the same. Nothing in this task rewrote an
  executor body — the backends slice is a directory move, and every file in it is a `git`
  rename at 93% or better — but the assertion rests on that fact, not on a run.

### The whole task, measured

| | |
|---|---|
| commits | 10, from `1676822d` to `11b17f83` |
| files changed | 216 |
| renames git detects | 85 |
| goldens | 31 files touched, all in the quarantined `GpuHashJoin` commit; byte-identical since |
| case inventory | one authorized change: `config.rs`'s two lib cases became two in `test_golden_format`, net zero |
| documentation sentences | 4,071 before slice 3, 4,268 now; 48 absent, every one classified below |
| `pub use` | 30 → 0 |
| `pub(super)` | 85 → 0 |
| `pub mod` outside `lib.rs` | 52 → 13, all inside the two exempt backend directories |

The 48 absent sentences, by cause: 10 the deleted `config.rs`; 9 the `enforcer` → `accountant`
rename the spec asks for; 13 module headers that became item docs or a component header,
differing by a prefix or a clause; 5 doc links to paths that no longer exist; 4 renamed
identifiers in prose; 3 task 1's `this mode` wording; 3 the dead `all_row_groups`; and the
`#[allow]` that became an inner attribute. None is prose that was meant to survive: the nine
that were are in the retroactive pass above, restored.

## Review round 1

The reviewer verified the mechanical core itself rather than reading the evidence table: it
re-derived the golden substitution as a sorted-multiset identity, ran its own static `#[test]`
census over both trees, and confirmed that no FFI type is reachable from a rust-only path. That
is the reading the evidence table cannot substitute for, and it is why the table is not enough.

Two of its findings were the layout test failing its own rule, and both had been watched red on
a shape that could not exercise them. The super-climb reader took `rel.components().count() - 1`
as depth, so a component facade got one free climb — the climb that leaves the component — and
the red-watch had used a leaf file. The private-type reader matched an item's first line, so a
wrapped signature hid its types; `executor::run` and `planner::plan` are both wrapped.

### The family has five members and a shape

The four findings this task recorded as one thing now take a sentence rather than a count. Every
edit whose correctness depends on a type's visibility or its path is a cudf-only-break candidate
wherever a `cfg(not(feature = "rust-only"))` impl names that type, and a green rust-only build is
evidence about neither. The five are that sentence with different nouns: a name (`GpuBatch`), a
path (`super::crate::`), a shadow (`plan(&plan, …)`), a visibility on a struct (`GpuSource` under
the fixpoint), and a visibility on a `pub mod` (`GpuSource` again, through `Backend::Source`).

The fifth arrived while fixing the third finding, which is the argument for building all three
shapes after every edit rather than at slice boundaries.

### A subtree exemption is the shape that grows

Exempting `executor/cpu_backend` and `executor/gpu_backend` wholesale put 91 `pub` items across
13 files behind an exception granted for fourteen types — over half the crate's remaining `pub`
surface — and two of the eleven inner `pub mod` declarations were forced by nothing outside the
crate at all. It is now one entry per module path, nine of them, each naming the files that
force it.

### Half a claim expires wrong

A `forced_by` naming one of four forcing files goes green the day that file is fixed, and
announces that the wall can go up while three files still need it. Both directions are checked
now — a named file that no longer forces, and a forcing file the list omits — and the reverse
direction caught the list's own first version.

### A green control is half of watching a guard red

Narrowing a reader and seeing the new shape fail proves the reader got sharper. Only a control
proves it did not simply stop exempting: the same `pub` inside a path that is still exempt must
still pass. Both belong in every red-watch, and the five mutations run for this round each had
one.

### Logged, not filed

Neither is production behaviour, so neither gets a ticket.

`cargo fmt --all -- --check` is not clean repo-wide — `cost-report/src/main.rs` has hand-aligned
tables. There is no `rustfmt.toml` and no fmt or clippy step in `pipeline.yml`, so neither is a
gate today, and introducing one is not this task's.

The word "mode" survives as a common noun in about twenty comments, referring to a thing task 1
retired. It is a task-1 follow-up; rewriting twenty comment lines here would churn the
doc-and-attribute baseline for no layout reason.

## A dispatch died at 03:59, and left its work uncommitted

A coordinator run ended between `9d92c159` (03:56) and the working tree it left behind (03:58,
03:59). Five files were modified and never committed, and nothing in this file recorded them.
The successor found them by mtime and reflog, not by a note, which is the failure mode the
detail-file rule exists to prevent.

What the residue does, from the diff:

- `test_module_layout.rs` — the reverse half of the `forced_by` check swept `peacockdb-core/tests`
  only, and matched an uppercase letter after the module path. Both are holes. `peacockdb/src/main.rs`
  names `executor::cpu_backend` and no test-crate sweep could see it, and a free function is
  lowercase, so `cpu_backend::physical_expr` — the exact shape the exemption exists for — was
  dropped as a near-miss. The residue walks every `[workspace]` member's `src` and `tests`,
  matches "not another `::`", excludes the guard file itself through `file!()`, and turns
  `forced_by` entries into repo-root relative paths.
- `executor/mod.rs` — the `physical_expr` facade hop is deleted; `plan/tests/aggregate.rs` now
  names `executor::cpu_backend::physical_expr` directly, and the doc comment on the real item
  says why a facade hop for one import is the wrong trade.
- `gpu_backend/mod.rs` — a doc comment that had drifted onto the wrong struct is put back on
  `GpuExec`, and `GpuSource`'s fields use the file's existing imports.

None of it is proven: no build, no test run, no evidence anywhere. It is dispatched as work to
finish, not as work to trust.

## Finishing the residue

Two of the five modified files survive. The layout test's sharpened readers and the
`gpu_backend` doc-block repair were right and are kept, with three comments corrected and one
guard added. The `physical_expr` facade deletion was a mistake and is reverted.

### What the residue got wrong

- `files_naming`'s doc claimed `peacockdb/src/main.rs` names `executor::cpu_backend`. It does
  not — it names `executor::CpuBackend` and `executor::run`, and nothing outside
  `peacockdb-core/tests` names any of the nine exempt paths today. The workspace-wide sweep is
  still the right reader, for the reason the comment now gives: what forces an exemption is any
  code outside the crate, not one crate's tests. Same correction on `names_the_module`, which
  claimed the uppercase reader had dropped a file that forces an exemption.
- The `file!()` exclusion was asserted only to resolve, not to work. A `file!()` whose form
  drifts from what the walk yields would leave the guard reporting itself, and the assert would
  still pass. `each_reader_sees_the_violation_and_not_its_near_miss` now pins both halves: this
  file is a match for the needle, and `files_naming` does not return it.
- `executor/mod.rs` lost a blank line and was not rustfmt-clean. The whole layout test was not
  either, at head as well as in the residue, so it was run through rustfmt as a leaf — it
  declares no `mod`, so nothing below it moved.

### The facade hop is the wall, not a hop

The residue deleted `executor::physical_expr` and pointed `plan/tests/aggregate.rs` at
`executor::cpu_backend::physical_expr`. The spec's own rule refuses that: only the parent
component's own code may use a subcomponent, and `plan` is not `executor`. It compiles only
because `cpu_backend` is an exempt `pub mod`, and the exemption was granted for two external
test files — so the change leans on the exemption for a reason `forced_by` cannot record,
since `files_naming` skips `peacockdb-core/src` by design. The exemption's expiry would then
announce that the wall can go up while an in-crate caller still needs it down, which is the
defect "half a claim expires wrong" is about. Reverted, with `cpu_backend/mod.rs`'s doc, which
described the deleted hop.

`peacockdb-core/src/wire/tests.rs:829` already does the same thing — `use
crate::executor::cpu_backend::join::CpuJoin` from the `wire` component. It predates this task.
A reader for it belongs in the exemption test, but `CpuJoin` is a type and cannot be delegated
through `executor/mod.rs`, so closing it means moving that test or declaring the type in
`executor`. Left as a finding; not production behaviour, so no ticket.

### Every changed reader was watched red, each with a control

| Mutation | Result | Control |
|---|---|---|
| drop `test_cpu_executors.rs` from `executor/cpu_backend` | red: "is forced by peacockdb-core/tests/test_cpu_executors.rs, which forced_by does not name" | the other nine cases pass |
| `forced_by` back to the tests-relative `test_gpu_executors.rs` | red on both halves: names a file that no longer exists, and is forced by one it does not name | the repo-root form is green |
| a line naming `cpu_backend::CpuExec` added to `peacockdb/src/main.rs` | red naming `peacockdb/src/main.rs` | listing `peacockdb/src/main.rs` in `forced_by` turns it green, so the forward lookup resolves a non-test crate too |
| the same line, with the sweep restricted to `peacockdb-core` | green — the hole the workspace sweep closes | |
| `names_the_module` back to uppercase-only | red on the `physical_expr` fixture | |
| `names_the_module` widened to `contains` | red on the `join::CpuJoin` fixture, so the reader got sharper rather than permissive | |
| `out.retain` dropped from `files_naming` | red twice: the new fixture, and the exemption test reporting the guard file itself | with the retain, green |

### Evidence

| Check | Result |
|---|---|
| rust-only, all targets | 0 warnings |
| default, lib+bin | 0 warnings |
| default, all targets | 0 warnings |
| `test_module_layout` | 10 passed |
| `--lib` | 435 passed |
| `test_plan_goldens` | 19 passed |
| `test_ci_coverage` | 7 passed |
| goldens | byte-identical to `goldens-after-rename.sha256`, digest `e071580a…` |
| case inventory, both shapes | byte-identical to the two `-final` baselines |
| residue gate | the same five lines |
| doc sentences | three absent, all reworded doc comments on the two readers |
| rustfmt | both touched files clean |

The dispatch named `27eac51d…` as the golden digest. That is `goldens.sha256`, taken before the
first move; the quarantined `GpuHashJoin` rename superseded it, and the bar the spec sets is
`goldens-after-rename.sha256`. No golden is modified in the working tree at all.

### The parquet is still not in this worktree

`test_plan_goldens` fails 13 of 19 with `register the tables: IoError NotFound` unless
`PEACOCK_TESTDATA_DIR` points at a scratch root of symlinks, as slice 2 describes. It is an
environment gap, not a regression, and it costs a debugging round every dispatch that meets it.

## CI answers the question the local run could not

"The full suite, once, at the end" says the GPU half of this branch is proved by three clean
builds and by nothing else, because no host this run could reach has a device. That was true of
the local run and is not true of the branch. CI run on `b0f84b5f` — the round-1 head, the last
commit before this one to touch code — is green on every job that runs: both cuDF legs, the
25.02 GPU build, the cost report, the S3 metadata check, and the remote GPU tier. The GPU tests
ran on a device.

Two heads above it are documentation only and get a changed-paths skip, which is not a gate and
not evidence. Read the green on the last code head, not on the tip.

## Review round 2

No blocking findings. The reviewer re-derived the mechanical core rather than reading the
evidence: goldens 170/170 against `goldens-after-rename.sha256` with an empty `testdata/` diff
since the quarantined rename; zero bare `pub` outside the exempt files; exactly nine `pub mod`
outside `lib.rs`, all nine registered; zero `pub use` and zero `pub(super)`; zero super-climbs
leaving a component under the corrected depth, with 37 files sitting exactly at their cap; all
nine `forced_by` entries true in both directions by its own reimplementation; and no FFI type
reachable from a rust-only path.

Round 1 is closed. Both of round 2's open questions resolve for the branch: the facade revert is
right, because the deleted call compiled only through an exemption granted for two external test
files and `files_naming` skips `peacockdb-core/src` by design, so nothing could record an in-crate
caller; and the widened `names_the_module` is sharper rather than weaker, since no code shape was
found that matches without forcing the `pub mod`.

### The one site where the tree breaks the rule it ships

A repo-wide sweep for a cross-component subcomponent reach inside `src` returns exactly one hit:

    peacockdb-core/src/wire/tests.rs:829  use crate::executor::cpu_backend::join::CpuJoin;

It predates the task — `787c1e5c:peacockdb-core/src/batch_partitioned/recipe/tests.rs:824` is the
same line. Nothing else does it in any form.

Its expiry is the problem, not its existence. The layout test has no reader for this shape;
`no_subcomponent_reaches_a_sibling` compares siblings under one parent. So when `test-layout.md`
moves `test_cpu_executors.rs` and `injection.rs` into `src/`, the `forced_by` expiry will say the
`cpu_backend` wall can go up, and taking it up is an `E0603` on this line. That is the same
"half a claim expires wrong" shape the revert cited as its own reason. Loud rather than silent,
which is why it is important and not blocking.

### coding-style.md states three rules the tree deliberately breaks

Line 117 says the components in `lib.rs` are the only `pub mod` in the crate; lines 106-108 say
nothing else in a component carries `pub` or `pub mod`; line 144 says the subcomponent rule is
enforced across components. Nine `pub mod` are exempt, 60 bare `pub` items sit behind that
exemption, and the cross-component rule has one live violation. The page does not mention the
exemption at all, so a developer who greps `pub mod` cannot tell a violation from a sanctioned
exception.

### Nits worth taking while the developer is in these files

- `test_module_layout.rs:378-380` — the stated reason for matching the path exactly is wrong. A
  file naming `gpu_backend::accumulate::GpuAccumulator` does force `gpu_backend` to be `pub mod`,
  because the path traverses it. The consequence is stuck-red rather than a hole: if
  `test_gpu_executors.rs` alone stops naming `executor/gpu_backend`, the forward half goes red and
  the three child-naming files cannot re-justify the entry.
- `test_module_layout.rs:467` — `names_the_module` matches comments and string literals, so both
  halves of the expiry can be satisfied by prose. No live instance; every match outside the guard
  file is a `use` line.
- Comment caps: `test_module_layout.rs:673` is 13 lines against a cap of 10, `:22` is 11, and
  `:511` is a 5-line in-body comment against a cap of 4.
- Four files that were rustfmt-clean at the branch base are not after `b0f84b5f`, and the dirt is
  the line it added: `gpu_backend/backend.rs:8-9,13,15`, `gpu_backend/source.rs:13`,
  `cpu_backend/tests/backend.rs:12`.
- `14eb0308` also stopped `pub_declarations` counting `<`/`>` as nesting. It is a correct fix —
  a `pub const X = 1 << 20;` would have swallowed every declaration below it — and it is pinned by
  a fixture, but neither the commit message nor "Finishing the residue" says it happened.
- `no_public_signature_names_a_type_from_a_private_module` matches `alias::` and `module::`
  prefixes only, so a bare type imported out of a private module and named in a `pub fn` signature
  would pass. Not live: the only such import is `Trip` into `executor/driver/mod.rs:25`, which
  declares no `pub` items.
- `test_module_layout.rs:604-620` advances seven bytes after a hit, so one `super::super::x` at
  depth 0 is reported twice. Message noise.

### A count the round-1 note reads too small

The exemption still covers 60 bare `pub` items across seven implementation modules, which
`a_components_api_is_declared_in_its_mod_rs` does not read at all. The narrowing moved it from
91 items across 13 files to 82 across 9, or 60 once the two subcomponent `mod.rs` facades that
test skips anyway are excluded. The same fourteen types force it, so the shape is right.

## Round 2, closed

### The reader, and where the register lives

`only_the_parent_component_names_a_subcomponent` in `peacockdb-core/tests/test_module_layout.rs`,
the eleventh case. Two helpers behind it. `subcomponent_paths` derives the set from the tree
rather than listing it — a directory with a `mod.rs` that is neither a component nor a test
module, at any depth — and returns the five there are today: `executor/{cpu_backend,driver,
gpu_backend}` and `planner/{translator,translator/scan_mapping}`. `cross_component_reaches` then
reads every file under `src/`, skips the subcomponents of its own component, and looks for
`crate::<parent>::<sub>::`, reporting the deepest path only so one line is not named twice.

The register is `CROSS_COMPONENT_REACHES`, a `&[CrossComponentReach { file, path, why }]` beside
`PUB_MODULES`. One entry: `wire/tests.rs` reaching `executor/cpu_backend`. It is checked in both
directions, like `forced_by` — an entry whose line is gone is reported, and so is a reach the
register does not name. `wire/tests.rs:829` now carries two lines saying why it is there and that
it dies with the `cpu_backend` exemption.

What the reader buys `coding-style.md:144`: the claim is not "enforced across components" flat.
rustc enforces it wherever a subcomponent is declared `mod`, and enforces nothing for the two
declared `pub mod` — which is where the test takes over, with one registered exception. The
honest wording is "the compiler enforces it except behind the `pub mod` exemption, where the
layout test checks it and the exceptions are named".

### Prose no longer satisfies an expiry

`code_only` drops line comments, and both expiry readers run through it — `names_the_module` for
the `forced_by` halves and `cross_component_reaches` for the register. A commented-out `use` held
an exemption open from one side and a sentence about one satisfied it from the other. Line
comments only: this crate writes no block comments, and a `//` inside a string can at worst hide
a later match on that line, which under-reports rather than over-reports. String literals are
still matched, which is why `files_naming` needs its `file!()` exclusion at all.

### Every new and changed reader was watched red, with a control

| Mutation | Result | Control |
|---|---|---|
| register entry pointed at `wire/mod.rs` | red both ways: the entry no longer names it, and `wire/tests.rs` names it unregistered | |
| a second, bogus entry (`plan/mod.rs` → `executor/gpu_backend`) | red on the expiry half alone | the real entry stays silent |
| a real `use crate::executor::cpu_backend::CpuExec;` added to `plan/mod.rs` | red naming `plan/mod.rs` | the same `use` inside `executor/mod.rs` is green — its own component |
| the same line, commented out, in `plan/mod.rs` | green | |
| `code_only` made the identity | red on the commented-out fixture | |
| `code_only` made to drop the whole line | red on the `use …; // why` fixture, so it strips the comment and not the code | the nine `forced_by` entries still pass, so nothing live was stripped away |

One mutation failed to build rather than to run, and the exit status caught it — the hazard this
file already records under "A hazard met while proving the rules red".

### Nits taken

`pub_declarations`' angle-bracket change, which round 1 made and did not record: a `<` counted as
an open leaves `1 << 20` two deep, so the `;` ending a `pub const` is missed and every declaration
below it is swallowed — the guard reporting nothing while reading nothing. Only `(` and `[` nest
now, pinned by a fixture.

The `files_naming` doc claimed that counting `gpu_backend::accumulate::GpuAccumulator` for the
parent would make the parent look forced by files that force only the child. It does traverse
`gpu_backend` and does force that wall down. The behaviour stands and the trade is now stated:
attributing a child's callers to the parent would let one file justify an entry it never names,
and the cost is a stuck red when `test_gpu_executors.rs` stops naming `executor/gpu_backend`.

Three comments were over their caps and are trimmed: the `PubModule` doc at 11 lines, the
`pub_declarations` doc at 13, and a 5-line in-body comment in the sibling reader.

`gpu_backend/backend.rs`, `gpu_backend/source.rs` and `cpu_backend/tests/backend.rs` were clean
at `b0f84b5f^` and are clean again. All three are leaves, so rustfmt moved nothing below them.

### Not taken, with reasons

- `tests/test_cpu_end_to_end.rs` carries nine hunks of its own, but it had four at `787c1e5c` and
  nine at `b0f84b5f^`, so the review rounds did not add them. It declares `mod common;`, so
  formatting it reformats thirty hunks across `tests/common/`. Same shape for
  `cpu_backend/expr_physical.rs`, which declares `mod tests;`.
- `tests/common/corpus_gpu.rs` gained one import-order hunk from `b0f84b5f` on top of two it
  already had, and formatting it would take all three.
- The super-climb reader's duplicate report. The fix is one token — advance `7 * climbs` rather
  than `7` — but it changes a reader that was watched red on a particular shape, and what is wrong
  is a repeated line in a failure message. Not worth re-proving the reader for.

### The case inventory gains exactly one line

The eleventh case is a deliberate addition, so both `-final` inventories differ by its name and
the count line above it, and by nothing else. The baselines are left as they are rather than
re-taken, so the coordinator's reference for the next round is still the one it wrote.

### Evidence

| Check | Result |
|---|---|
| rust-only, all targets | 0 warnings |
| default, lib+bin | 0 warnings |
| default, all targets | 0 warnings |
| `test_module_layout` | 11 passed |
| `--lib` | 435 passed |
| `test_plan_goldens` | 19 passed |
| `test_ci_coverage` | 7 passed |
| goldens | byte-identical, `e071580a…` |
| case inventory, both shapes | the one new case and nothing else |
| residue gate | the same five lines |
| doc sentences | eleven absent, every one a comment rewritten above |
| rustfmt | the five files this round touched are clean |
