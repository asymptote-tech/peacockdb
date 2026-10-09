# limits — working record

## Standing facts

- Third of chain K, branch `ENS-limits` off `ENS-distinct-companions`, PR will target that
  branch. Closes [#186](../tickets/corpus-coverage.md#t186) and
  [#234](../tickets/corpus-coverage.md#t234).
- **Chain K runs without a GPU.** No device build, no device run, no GPU cycle. Every build and
  run is local — verda does not resolve from this host (checked again at this dispatch). The
  `scan.cpp` edit and the four `gpu_tests/source_cases.rs` cases are built, not run; #281 holds
  them. The task reaches `done` when every CI job but the GPU tests is green.
- **The cost gate's rises are pre-accepted by the human** (2026-10-08, on the board): this task
  is `done` when the cost-report job's only regressions are the sections this file lists from a
  local `--cost-diff` run — expected `nested-limits` at tp1-rowgroup and tp4-rowgroup, +228
  bytes. Any other regression is an ordinary finding. So the local `--cost-diff` output has to
  land in this file before CI is read, or there is nothing to compare the job against.
- The chain's base is master `31c56bea`. Both tasks below this one were rebased onto it and
  re-proved today; `guard-checks-detail.md` records what master brought across. Nothing in this
  task's scope overlaps master's `64ced62e`, which touched `plan_text` and the join-projection
  rendering.

## Dispatch log

### 2026-10-09 — round 1, developer dispatched

Branch cut from `ENS-distinct-companions` at `aa0335bf`, board moved
`approved to build` → `building` in the same commit.
