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
