# empty-sorts — working record

## Standing facts

- Fourth and last of chain K, branch `ENS-empty-sorts` off `ENS-limits`, PR will target that
  branch. Closes [#205](../tickets/corpus-coverage.md#t205).
- **Chain K runs without a GPU.** No device build, no device run, no GPU cycle. Every build and
  run is local — verda does not resolve from this host, checked again at this dispatch. The three
  `gpu_tests/accumulate_cases.rs` cases are built, not run; #281 holds them. The task reaches
  `done` when every CI job but the GPU tests is green.
- **The cost gate's rises are pre-accepted by the human** (2026-10-08, on the board): this task
  is `done` when the cost-report job's only regressions are the sections this file lists from a
  local `--cost-diff` run — expected q17 at tp1-single and tp1-rowgroup, +24 bytes. Any other
  regression is a finding. So the local `--cost-diff` output has to land in this file before CI
  is read, or there is nothing to compare the job against.
- The chain's base is master `31c56bea`. The three tasks below this one are `done`.

## The spec's DuckDB conditional resolves to "has not merged"

Checked on this branch, 2026-10-09. The two `duckdb-result.txt` goldens exist — they landed
2026-09-28 with #235 — but no `duckdb_oracle` module exists anywhere under
`peacockdb-core/src/`, and chain J's `duckdb-oracle` task is `blocked(completeness approved)`
with PR #167 unable to get a CI run. So:

- nothing in this task touches DuckDB;
- the list of things the helper must strike when `duckdb-oracle` is merged **after** this task
  goes into [#235](../tickets/corpus-coverage.md#t235)'s empty-answer bullet, since #205 is
  archived here and the references to it over there would outlive it. That list is q17's
  `duckdb_divergent(205)`, the tests asserting #205 open (`duckdb_oracle/tests.rs:157-158`,
  `:270-278`, `:322-323`) and the empty-answer branch in `duckdb_oracle.rs:113-115`.

The #235 edit is markdown, so the coordinator writes it rather than the developer.

## Dispatch log

### 2026-10-09 — round 1, developer dispatched

Branch cut from `ENS-limits` at `4581bca3`, board moved `approved to build` → `building` in the
same commit.
