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
