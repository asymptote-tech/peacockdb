# Every tier passes on cuDF 26.02, and the newly enabled cells are benchmarked

Kind: production

**This task closes no ticket.** Last of the join-rewrite chain. Development and CI run on cuDF
25.02 (shad-gpu) and 25.10a (the CI's "26.02" leg, [#129](../tickets/testinfra.md#t129)); the
chain's C++ was written to compile on both and on 26.02, but nothing has run it on 26.02. This task
does, fixes what fails, and records the benchmark of every query-mode cell the chain turned on.
Its record is the evidence [#244](../tickets/system-hardening.md#t244) (whether to drop 25.02)
waits on.

## Where it runs

On shad-gpu, against its existing 26.02 environment `~/miniforge3/envs/rapids-2602` (libcudf
26.02, nvcc 12.2; a standalone libcudf program ran there on driver 535.247.01 on 2026-10-07):
- the C++ build reuses `~/build-2602` (a CMake tree of two C++ targets from 2026-08-13,
  `~/build2602.sh`) if it configures for the repo's full targets, and otherwise uses a new build
  directory beside it; never `cpp/build`, which the 25.02 runs use;
- the cargo build uses its own target directory (`CARGO_TARGET_DIR`), never the 25.02 one;
- the same H200 and the same sf40 data, so the benchmark numbers compare with 25.02's directly.

Only if the build or the driver fails there does it fall back to a GPU host provisioned with
26.02 for this task alone, released when it ends.

## The work

1. Build every target against 26.02 as above; record the configure and build lines in the detail
   file.
2. Run every tier `build-test.md` lists that touches the device or links cuDF — the C++ gtests
   (`test_join_session.cpp` included), the gpu tier, `test_gpu_corpus`, the benchmarks — and the
   cpu tiers once, for the record. Write each tier's counts beside the 25.02 run's.
3. Fix what fails, each with its own red case first; a failure that is a 26.02 behaviour the
   engine should follow, not a defect, is recorded with its reason.
4. The corpus benchmark for every query-mode cell the chain turned on (the cells
   `join-rewrite-cell-estimate.md`'s comparison lists as flipped), on 26.02 and on 25.02, written
   to `testdata/benchmark-results/` as the benchmark tree is.
5. Add to #244 what the record shows: every tier's outcome on 26.02, and the benchmark against 25.02.

## Scope

| path | change |
|---|---|
| `scripts/build-test-shadgpu.sh` (or a sibling) | a 26.02 mode: the env, the build dir, the target dir |
| whatever the failures need | fixes, each with its case |
| `testdata/benchmark-results/` | the numbers |
| `llm-wiki/build-test.md`, `tickets/system-hardening.md` | how to run on 26.02; #244's evidence |

Component-level API: none expected; any fix that needs one says so in its PR.

## Restriction

26.02 verification and the fixes it needs. No 26.02-only fast path (#242), no removal of 25.02
(#244 decides that later).

## Verification bar

- device, 26.02: every tier green or each failure fixed or recorded.
- device, 25.02: the same tiers still green after the fixes.

## Device workflow

shad-gpu, the 26.02 environment, foreground cycles; the 25.02 rerun last.
