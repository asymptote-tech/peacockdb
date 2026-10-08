# Every tier passes on cuDF 26.02, and the newly enabled cells are benchmarked

Kind: production

**This task closes no ticket.** Last of the join-rewrite chain. Development and CI run on cuDF
25.02 (shad-gpu) and 25.10a (the CI's "26.02" leg, [#129](../tickets/testinfra.md#t129)); the
chain's C++ was written to compile on both and on 26.02, but nothing has run it on 26.02. This task
does, fixes what fails, and records the benchmark of every query-mode cell the chain turned on.
Its record is the evidence [#244](../tickets/system-hardening.md#t244) (whether to drop 25.02)
waits on.

It starts from a partial run. On 2026-10-08, with shad-gpu down, the device tiers ran on 26.02 on
an L40S and an RTX PRO 6000 Blackwell. That run is
[#260](../tickets/system-hardening.md#t260): its environment, its counts, a cuDF 26.02 fault in
the shared-memory groupby for a multi-batch decimal avg, and the 25.02 control that clears the
engine's code. Begin from it rather than rediscovering it.

## Where it runs

On shad-gpu, against its existing 26.02 environment `~/miniforge3/envs/rapids-2602` (libcudf
26.02, nvcc 12.2; a standalone libcudf program ran there on driver 535.247.01 on 2026-10-07):
- the binaries are built **on the workstation**, as every shad-gpu run's are — shad-gpu has no
  cargo, and `~/build-2602` (with `~/build2602.sh`) is a C++-only tree built from a separate source
  copy (`~/peacockdb-src`), not reusable. The C++ builds in `cpp/build26` against the workstation's
  26.02 environment (libcudf 26.02.01, the host's build), never `cpp/build`; cargo uses its own
  target directory (`CARGO_TARGET_DIR`), never the 25.02 one;
- `build-test-shadgpu.sh` gains a 26.02 mode (`PEACOCK_CUDF=26.02`) that pushes to its own remote
  directory, `/home/info/peacockdb-2602`, never the 25.02 one;
- the workstation's environment runs CUDA 12.9, the host's 12.2 (driver 535). Whether the shipped
  binaries load there is unproven, so the first cycle climbs a ladder and records which rung held:
  the host environment's cudart; the workstation's 12.9 cudart shipped beside the binaries; a local
  replica of the host's 12.2 environment to build against; and only then the fallback below;
- the same H200 and the same sf40 data, so the benchmark numbers compare with 25.02's directly.

Only if every rung fails does it fall back to a GPU host provisioned with 26.02 for this task
alone, released when it ends.

**Without the H200 it proceeds on nebius-gpu.** While shad-gpu is down, the task runs there under
chain J's board note: an L40S with 46 GB, modern glibc, and both cuDF environments already in
place (26.02 at `~/miniforge3/envs/rapids-26.02`, 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2`).
- Every step that needs no H200 completes there. That covers the build, every tier that fits the
  card, the fixes, the `gpu-result-26.02.txt` comparison, the report and #260.
- The H200-only parts are deferred, not blockers, and the report names each one: the sf40 tiers
  and the step-4 benchmark against 25.02's H200 numbers.
- The cudart ladder above is shad-gpu's driver-535 question and does not arise there: the driver
  is 580.

## The work

1. Build every target against 26.02 as above; record the configure and build lines in the detail
   file.
2. Run every tier `build-test.md` lists that touches the device or links cuDF — the C++ gtests
   (`test_join_session.cpp` included), the gpu tier, `test_gpu_corpus`, the benchmarks — and the
   cpu tiers once, for the record. Write each tier's counts beside the 25.02 run's.
3. Fix what fails, each with its own red case first; a failure that is a 26.02 behaviour the
   engine should follow, not a defect, is recorded with its reason.
4. The corpus benchmark on 26.02 and on 25.02, for the cells the chain turned on that the
   benchmark times: it times only tpch sf40 cases (`corpus_benchmark_cases.inc`), and tpcds and
   pbench have no sf40 data, so the comparison is the tpch sf40 cases among the flipped cells
   (`join-rewrite-cell-estimate.md`'s comparison). Each run block names its cuDF (`cudf=`), and the
   results go under `testdata/benchmark-results/cudf-<version>/`, so a 26.02 run never overwrites
   25.02's.
4b. The device's answers on 26.02: a corpus cycle with `PCK_WRITE_GPU_RESULT=26.02` writes
   `gpu-result-26.02.txt` (gitignored, never over the committed 25.02 file), compared with DuckDB
   by the same `duckdb_gpu_*` cases pointed at it; every divergence from DuckDB that 25.02 does
   not show is a finding.
5. Add to #244 what the record shows: every tier's outcome on 26.02, and the benchmark against 25.02.
6. Write the report, `llm-wiki/reports/cudf-26.02-verification.md`. It is the record a reader
   needs without the detail file:
   - each host and environment, as exactly as #260 states its own;
   - every tier's counts on 26.02 beside 25.02's;
   - each failure, with its cause and its fix or reason;
   - the benchmark comparison and the `gpu-result-26.02.txt` divergences;
   - what it means for #244.
7. Update #260 with what this task settles: whether the fault reproduces on the H200, its minimal
   case and the upstream report, and the fix or workaround. Close it if the fix lands here;
   otherwise leave it open and point at the report.

## Scope

| path | change |
|---|---|
| `scripts/build-test-shadgpu.sh` | `PEACOCK_CUDF=26.02`: the env, `cpp/build26`, the target dir, the remote dir, the cudart rung |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`, `peacockdb-ffi` | `peacock_cudf_version()`, a new symbol naming the linked cuDF — `peacock_gpu_version()` is unchanged, since `cpp/tests/cpu/test_executor.cpp:9` pins it at `"0.1.0"` |
| benchmark writers | `cudf=` per run block; the versioned tree |
| whatever the failures need | fixes, each with its case |
| `testdata/benchmark-results/` | the numbers |
| `scripts/lib/shadgpu-env.sh`, `scripts/build.sh` | the 26.02 environment and build dir |
| `peacockdb-core/tests/common/corpus_benchmark_cases.inc` | the tpch sf40 cases the comparison times |
| `cpp/tests/cpu/test_executor.cpp` | `peacock_cudf_version()` asserted beside `peacock_gpu_version()`'s `"0.1.0"` |
| `llm-wiki/build-test.md`, `tickets/system-hardening.md` | how to run on 26.02; #244's evidence; #260's update |
| `llm-wiki/reports/cudf-26.02-verification.md` | new: the verification report |

Component-level API: one additive C ABI symbol, `peacock_cudf_version()`. Any fix that needs more
says so in its PR.

## Restriction

26.02 verification and the fixes it needs. No 26.02-only fast path (#242), no removal of 25.02
(#244 decides that later).

## Verification bar

- device, 26.02: every tier green or each failure fixed or recorded.
- device, 25.02: the same tiers still green after the fixes.

## Device workflow

shad-gpu, the 26.02 environment, foreground cycles; the 25.02 rerun last.
