# sink-divergence-survey — run detail

Spec: [`sink-divergence-survey.md`](sink-divergence-survey.md). Plan:
[`sink-divergence-survey-impl.md`](sink-divergence-survey-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 7, **a prototype**: branch `ENS-sink-divergence-survey`, forked at
`2cb92f63`, the tip of task 6's branch. No PR, no reviewer, never merged; `done` means the branch
is pushed with the report at `llm-wiki/reports/sink-divergence.md` and the signoff on the spec.
`pipeline.yml` runs on pushes to master and on PRs, so CI does not run here.

## How this task is dispatched

Plan task 1 (the message, with a CPU unit test) as one dispatch; plan tasks 2 and 3 (the rollout,
then the report from its evidence) as the next, since the report is the rollout's product and the
developer who collected the evidence writes it. The coordinator commits after each.

## Hosts at dispatch (2026-09-12)

Ubuntu 24.04 / glibc 2.39 box; verda's hostname does not resolve; shad-gpu up with a neighbour
at 37 GiB of 143.7; `cpp/build`, `cpp/install` and `target-cudf-rapids-cuda-12.2` warm from task
6's cycle; `target/` warm for `rust-only`; `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.

## What the plan leaves open, settled here

- **The error site** is `executor/gpu_backend/mod.rs:239` on this tree (`concat_batches`), not the
  plan's 176-184; task 6 moved lines. `GpuBackend`, `GpuBatch` and `GpuContext` are `pub(crate)`
  behind `cfg_attr(not(feature = "test-support"), allow(dead_code))` since task 6; nothing here
  changes that. The surface guard `SURFACE` refuses any new bare `pub`, and a prototype is held to
  the layout test like any branch — the one structural change the plan allows (a free function
  taking two schemas, for the unit test) is `pub(crate)` or private.
- **A disabled device cell is no test.** `corpus_query!` with `gpu_modes = none` expands to
  nothing in `test_gpu_corpus`, so the survey cannot filter its way to a disabled cell. On this
  throw-away branch the schema-disabled queries are enabled at **one mode, `tp1_single`**, in
  `tests/common/corpus_cases.inc`, in a commit of its own that says it is the survey's enablement
  and not a change to the corpus. `testdata/cost-registry.csv` is untouched — the spec's "no
  registry edit" — and the registry assertion is filtered out of every survey run
  (`PCK_TEST_FILTER` names queries, and it is not a query). One mode, because a divergence class is
  a property of types, not of lanes or batching; the report says so and names any query whose
  sink message would need another mode to reach.
- **The cell set** comes from the registry's `tickets` column: queries naming #183, #187, #191 or
  #163 — 94 rows, most of them `152 183`, where #152's refusal comes first and the sink is never
  reached; those are the "disagrees with its ticket" rows the spec asks for, not failures of the
  survey. The developer writes the exact list here before running anything.

## Run log

### 2026-09-12 — plan task 1 dispatched: the message names every diverging column

Board set to `building`.

### 2026-09-12 — plan task 1 done: the message names every diverging column

**Shape.** `try_new`'s sentence stays as the prefix; the appendix is ` (declared vs exported:
{index} {name}: {declared} vs {exported}; …)`, types only, and it is omitted when no zipped
column differs by type (a count-only refusal keeps the bare prefix). From the test:
`0 l_comment: Utf8View vs Utf8; 2 l_extendedprice: Decimal128(15, 2) vs Decimal128(38, 2)` —
a matching column between two diverging ones is skipped, its index is not renumbered. A
nullability-only difference yields the empty string.

**Where.** The comparison is `schema_divergence(&Schema, &Schema) -> String`, `pub(crate)` in
`peacockdb-core/src/executor/errors.rs`, with `#[cfg_attr(feature = "rust-only", allow(dead_code))]`
on the `wire_nodes` precedent: its one production caller is the sink in `gpu_backend/mod.rs`,
which `rust-only` compiles out. It sits there rather than beside the site because
`gpu_backend` is `cfg(not(feature = "rust-only"))`: a `tests` module inside it would be named
for the floor rung and never run on it, and would only ever run under `--lib` of an FFI-linked
build, which the rung path filters do not select. The tests are
`peacockdb-core/src/executor/errors/tests.rs` (`#[cfg(test)] mod tests;`, three cases: one
column with both types, two clauses across a matching column, nullability alone is none), red
first against a stub returning `""` (two assertion failures, the nullability case vacuously
green), then green. The sink's wrapping — prefix, conditional parenthetical — is three lines
without a unit test; the rollout is what exercises it.

**Results.** `cargo build --features rust-only -p peacockdb-core` 0 warnings;
`cargo-cudf.sh build -p peacockdb-core --features gpu` 0 warnings;
`--test test_module_layout` 17 passed; `--features rust-only --lib` 517 passed, 2 ignored
(514 + 3); `cargo-cudf.sh test --lib --features gpu --no-run` 0 warnings. Running the three
cases from that gpu-shape binary needs `LD_LIBRARY_PATH=$CUDF_ROOT/lib` locally (it fails at
load on `libcudf.so` otherwise — the loader path `build-test-shadgpu.sh` supplies as
`PATCHED_LD`); with it, 3 passed. rustfmt-check clean on the three touched files with
`--edition 2024 --style-edition 2024` (the crate's; the default 2021 sort reorders every
import in the tree). Not committed.
