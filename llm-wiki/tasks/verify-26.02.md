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
| `llm-wiki/build-test.md`, `tickets/system-hardening.md` | how to run on 26.02; #244's evidence |

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
