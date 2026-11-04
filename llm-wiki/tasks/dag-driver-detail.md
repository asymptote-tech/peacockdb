# dag-driver detail

Branch `ENS-dag-driver`, forked from `ENS-sf40-baseline` (`080b1bac`) on 2026-11-04.
Plan: `dag-driver-impl.md`, Tasks 1–11; each *Commit* step is a checkpoint the coordinator commits and pushes.

## Dispatches

- 2026-11-04: developer dispatched at Task 1 Step 1. Verda unreachable (no VERDA_CLIENT_ID); CPU tests run locally. Device steps on nebius-gpu (L40S).

## Task 1 notes

- `duplicate_handle` copies the original's `produced_by` entry instead of calling `producer_of`, as the plan's Step 3 put it. `producer_of` throws for a handle that is adopted or older than timing. A duplicate opens no region, so the refusal stays at the export of either handle, as before. `HandleDuplicate.IsChargedToTheOriginalsProducer` checks the duplicate opens no region: 6 regions, the export's at seq 1, call 1.
- An unknown handle resets the session, as `slice_handle` does (Interfaces). `AnUnknownHandleIsRefused` checks the next call sees "no plan loaded".
- The plan's file:line references are stale: `peacock_handle_release` is at `peacock_gpu.h:389`, not 299. The ABI count in architecture.md was already twenty-nine, which matches the header, and is now thirty.
- Counts after Task 1: `--lib` 987 (985 passed + 2 ignored), C++ CPU 64, `peacock_plan_tests` 135, gpu lib `gpu_tests::` 743. build-test.md's tier headers and rows are updated to match.
- The Step 4 run on the L40S: `ALL rc=0`. plan 135, join session 56, gpu 5, cpu 64, core gpu lib 743, test_gpu_corpus 989, benchmarks 9 (`--skip bench_`), node timing 1. The five new device cases are OK in `.gpu-test/` logs.

## Golden baseline (Task 1 Step 1, parent state `f52c11b6`, local)

- `--lib`: 984 passed, 2 ignored (986 cases).
- `--test test_corpus_goldens`: **53 passed** (master's 28 plus the chain's additions). Later golden checks compare with 53 plus each task's additions.
- `--lib -- planner::tests::plan_goldens`: 25 passed.
- `git status --short testdata/`: empty.

## VM time

| VM | start | stop | hours |
|---|---|---|---|
| nebius-gpu (Task 1 Step 4) | 2026-11-04 14:24 UTC | 2026-11-04 14:57 UTC | 0.55 |
