
# Pre-production system hardening tasks

## Contents

- [#13 — Hermetic builds: system-library whitelist + CI audit](#t13)
- [#196 — the table registrar's non-parquet guard does nothing, so a stray file panics](#t196)
- [#169 — a recipe plan is a chain, so its depth is its length, and the verifier caps depth](#t169)
- [#128 — Doctests run nowhere, and the meta guard cannot see them](#t128)
- [#244 — cuDF 25.02 is kept for a host constraint that may no longer hold](#t244)
- [#260 — on cuDF 26.02 a multi-batch decimal avg faults in cuDF's shared-memory groupby](#t260)

<a id="t13"></a>
### #13 — Hermetic builds: system-library whitelist + CI audit
`ld` silently prefers system libs over the conda env (seen as
`libarrow.so.2300: undefined reference to curl_easy_getinfo@CURL_OPENSSL_4`). Whitelist
glibc/libgcc_s/libcuda only; everything else from `$CUDF_ROOT`. Enforce via CMake
find-root pinning, build.rs link-search order, and a post-link `ldd` audit that fails CI.

<a id="t196"></a>
### #196 — the table registrar's non-parquet guard does nothing, so a stray file panics
`read_table` in `lib.rs` opens with `if path.extension() != Some("parquet") { () }` — the
condition is computed and discarded, so a non-parquet entry falls through to
`ListingTableUrl::parse` and four `unwrap`s. The caller's `let Ok(..) else { continue }` says
the intent was an `Err` there.

Nothing in the tree provokes it: every dataset dir holds parquet and nothing else, and
`.duckdb_cache/` is a sibling rather than a child. The CLI is what makes it reachable by a
user, since it registers whatever directory it is pointed at. The fix is the `return Err(())`
the shape already asks for, with a case putting a non-parquet file in the dir.

<a id="t169"></a>
### #169 — a recipe plan is a chain, so its depth is its length, and the verifier caps depth

fb children are nested, so the recipe plan for a query is one deep chain rather than a broad
tree: depth equals the number of addressed nodes plus its stubs. The C++ verifier caps depth at
1024, and the Rust reader had to have the same limit raised to parse what it had just written.

Deepest today is tpcds at `tp4-rowgroup`, seq 382, so nothing is near it. What makes it worth
recording is the failure mode: a plan of roughly a thousand addressed nodes fails at
`begin_plan` — the whole query refused before a call is made — rather than degrading at the call
that overruns.

The fix belongs here rather than in the verifier. Raising a limit to fit a shape that grows
without bound only moves the number; splitting one recipe plan into several, loaded in turn, ends
it. Not urgent at a factor of two and a half of headroom, and it wants measuring before it wants
designing: nothing yet says a thousand-node plan is a shape this mode should produce.

<a id="t128"></a>
### #128 — Doctests run nowhere, and the meta guard cannot see them
No step in `pipeline.yml` passes `--doc`, and `test_ci_coverage.rs` enumerates `--test`
targets plus `--lib`, so a doctest is invisible to the guard whose whole job is finding
targets CI does not run. The crate has none today: the one it had documented an entry point
that no longer exists.

There is now one pipeline to document, and it is three calls in a fixed order —
`planner::plan` for the tree, `wire::attach_recipes` where a device is involved, and
`executor::run` over a backend. `peacockdb/src/main.rs` is the only place that
sequence is written down, and a reader of the crate meets the three functions separately. A
doctest on the entry it documents is the natural fix and the reason to close both halves at
once: write it, run `cargo test --features rust-only -p peacockdb-core --doc` in the
dataset-matrix tier, and teach the guard that `--doc` is a target class it must see named
(the `--lib` check at `line_runs_lib_tests` is the pattern).

<a id="t244"></a>
### #244 — cuDF 25.02 is kept for a host constraint that may no longer hold
**Priority: low** — a cost, not a defect; it waits on `verify-26.02`.

The engine compiles and is tested against two cuDF versions: 25.02 on shad-gpu
(`envs/rapids-cuda-12.2`) and 26.02 on verda (`envs/rapids`), with the CI's "26.02" leg really
25.10a ([#129](testinfra.md#t129)). 25.02 was kept only because shad-gpu's driver is 535
(CUDA 12.2) and the host gives no sudo to upgrade it.

That constraint may be gone. shad-gpu already holds a 26.02 environment,
`~/miniforge3/envs/rapids-2602` (libcudf 26.02, nvcc 12.2, created 2026-08-13), and a CMake build
of two C++ targets against it, `~/build-2602` (`~/build2602.sh`, the same day). On 2026-10-07 a
standalone libcudf program built against it ran on the H200 under driver 535.247.01, giving the
same join and group answers as 25.02 (#243's probe). The repo's own tiers have not run there:
`verify-26.02` (the join-rewrite chain's last task) runs them on shad-gpu's 26.02 environment.

What dropping 25.02 would remove: the `__has_include(<cudf/join/join.hpp>)` header split in
`join.cpp` and the gtests; the portable-constructor constraints on `hash_join` and
`distinct_hash_join`; the CI's 25.02 C++ build job; `rapids-cuda-12.2` in the shad-gpu scripts
and `build-test.md`. What it would allow: the 26.02-only joins of [#242](performance.md#t242),
one cuDF for every host, and a CI leg that is really 26.02 (#129). To evaluate: whether every tier
passes on shad-gpu's 26.02 (verify-26.02's record), whether sf40 benchmarks on the H200 hold or
improve, and whether anything else on shad-gpu (the actions runner, its jobs) depends on the
25.02 environment.


<a id="t260"></a>
### #260 — on cuDF 26.02 a multi-batch decimal avg faults in cuDF's shared-memory groupby
On cuDF 26.02, `CudfAggregate{Partial}` dies with `cudaErrorMisalignedAddress` for any query
that averages a decimal over more than one batch. The error is sticky, so every later device
call in the process fails with it too. On 2026-10-08 that turned 8 failing corpus cases into
17 failures in one `test_gpu_corpus` run.

**Failing on their own:** `gpu_tpch_q1` and `gpu_tpch_shuffle_additive_avg`, each at
`tp1_rowgroup`, `tp4_rowgroup`, `tp4_single` and `tp4_sized`. Both pass at `tp1_single`, the one
mode with a single batch, where no Partial runs. q6, which has no avg, passes in every mode.
`gpu_tpch_shuffle_stddev_tp1_single` also fails, but that is [#94](corpus-coverage.md#t94)'s `group_merge_m2`, not this.

**Where it faults.** compute-sanitizer memcheck reports `Invalid __shared__ write of size 16
bytes … Access to 0xaa8 is misaligned` in
`cudf::groupby::detail::hash::single_pass_shmem_aggs_kernel`. The neighbouring threads write
to 0xab8 and 0xac8: decimal128 slots that sit on 8-byte boundaries, not 16. The host stack
runs `compute_shared_memory_aggs` ← `compute_single_pass_aggs` ← `cudf::groupby::aggregate`
← `peacock::execute_aggregate` — the main grouped call in `aggregate.cpp`.

**The request at that call**, captured with a temporary dump for q1 at `tp1_rowgroup`:
- keys: `l_returnflag`, `l_linestatus` (STRING, no nulls), `null_policy::INCLUDE`;
- 11 requests of one aggregation each, 122880 rows per batch;
- requests 0–3: SUM over DECIMAL128 at scales −2, −2, −4, −6;
- requests 4–9: three pairs of SUM and `COUNT_ALL` over DECIMAL128 at scale −2;
- request 10: `COUNT_ALL` over INT64.

**Not reproduced outside the engine yet.** The same 11 requests, sent through pylibcudf to the
same `libcudf.so` over row group 0 of the same file, run clean under compute-sanitizer. So
something about the engine's call still differs: the memory resource, the stream, or whether
the shared-memory path is chosen at all.

**It is cuDF 26.02, not the engine's code.** On nebius-gpu the same commit (`38d5f2de`), built
against cuDF 25.02 with `scripts/build-test-shadgpu.sh --build`, passes all 28 `test_gpu_corpus`
cases in one process, the 8 above included. Same host, same data, same code; only cuDF
differs. The 25.02 env is `~/miniforge3/envs/rapids-cuda-12.2`, created from the build box's
explicit list: libcudf 25.02.02 `cuda12_250303_g8139f3c84f_0`, libarrow 19.0.1, built with
gcc-12 12.4.0 and nvcc 12.9.86.

**Not the cause:** the GPU (the same 17 fail on Ada and on Blackwell), the dataset (byte-identical
on both hosts), and cuDF's parquet reader (`cudf.read_parquet` reads all of `lineitem` cleanly).

**Environment**, where it reproduces:

| | nebius-gpu | dev |
|---|---|---|
| GPU | NVIDIA L40S, 46068 MiB, compute capability 8.9 | RTX PRO 6000 Blackwell Server Edition, 97887 MiB, compute capability 12.0 |
| Driver | 580.173.02 (CUDA 13.0) | 580.126.09 (CUDA 13.0) |
| OS | Ubuntu 24.04.5 LTS, glibc 2.39, 8 vCPU, 31 GiB | Ubuntu 24.04, glibc 2.39 |
| Commit | master `38d5f2de` | `ENS-repartition-keys` `9b52b6a0` |
| cuDF env | `~/miniforge3/envs/rapids-26.02` | `~/miniforge3/envs/rapids-26.02-local` |

Both cuDF environments were created from one `conda list --explicit` of the build box's
`envs/rapids`:
- libcudf 26.02.01 `cuda12_260205_5b9658c4`;
- librmm 26.02.00 `cuda12_260204_498dafcf`;
- libarrow 21.0.0 (cpu);
- cuda-version 12.9, cuda-cudart 12.9.79, nvcc 12.9.86 from that env.

The build is `scripts/build-test.sh --gpu --build` with gcc-14 14.2.0, cmake 4.2.3 or 4.3.4,
ninja 1.12.1 and rustc 1.99.0. `libpeacock_gpu.so` is built for sm_80 and sm_90, so both cards
JIT its PTX. That does not matter here: the faulting kernel is in libcudf, which ships SASS for
both cards.

The data is `testdata/generate_testdata.sh --bench tpch` with the DuckDB 1.5.4 release CLI and
synthetic embeddings. `lineitem.parquet` has sha256 `91e14da396295b2f…`, the same on both hosts.

**To reproduce** on either host, from the repo root, after the build:

    E=<the cuDF env>
    LD_LIBRARY_PATH=cpp/build26/install/lib:$E/lib PEACOCK_TESTDATA_DIR=$PWD/testdata \
      /usr/local/cuda/bin/compute-sanitizer --tool memcheck --show-backtrace host \
      cpp/build26/install/rust-tests/test_gpu_corpus gpu_tpch_q1_tp1_rowgroup --exact --test-threads=1

**Next.** Find what makes the engine's call differ from the pylibcudf replay, and file the
minimal case upstream. Then check whether reordering the requests (decimal128 sums before the
4-byte counts) avoids it, as a workaround until cuDF is fixed. This blocks a GPU run on 26.02:
[#244](#t244) and `verify-26.02` wait on it.
