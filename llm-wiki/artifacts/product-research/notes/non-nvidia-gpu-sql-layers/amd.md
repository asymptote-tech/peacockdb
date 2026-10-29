# cuDF-like GPU dataframe/SQL kernel libraries on AMD ROCm/HIP (as of 2026-10-29)

All URLs accessed 2026-10-29 unless noted. GitHub facts come from `gh api` or from a sparse clone of hipDF under scratchpad/ext/hipdf.

## Q1. hipDF / ROCm-DS: base version, C++ libcudf availability, modules, GPUs, cadence, licence, dependencies (hipMM, hipCollections, hipCOMP)

### Takeaway
hipDF 3.0.0 (ROCm-DS 26.03, ROCm 7.2.3) is a production (GA since 25.10), Apache-2.0 port of **cuDF 25.10**. It keeps the `cudf::` namespace, the `<cudf/...>` header paths, `rmm::cuda_stream_view` and the `cudf::cudf` / `rmm::rmm` CMake targets. Its C++ library builds from source (`./build.sh libcudf`) and also ships as a binary wheel (`amd-libhipdf`), so an executor calling about 75 libcudf entry points should mostly recompile against it. The real porting risks are:
- the API gap between cuDF 25.10 and whatever cuDF version the executor targets;
- 64-wide wavefronts;
- hipCOMP (a fork of nvCOMP 2.2) in place of nvCOMP;
- no GPUDirect Storage, no zstd compression and no per-thread default stream (PTDS);
- a release cadence that trails RAPIDS by roughly two releases.

### Cited Findings
**Identity, version and status**
- The hipDF README calls it a "ROCm port derived from NVIDIA's RAPIDS cuDF, version 25.10" that keeps cuDF's directory structure and API naming. The default branch is `release/rocmds-26.03`. It has 19 stars and 3 forks, and the Releases section is empty — [GitHub AMD-Ecosystem/hipDF](https://github.com/AMD-Ecosystem/hipDF) (accessed 2026-10-29).
- The repo moved from `ROCm-DS/hipdf` to `AMD-Ecosystem/hipDF`, and the old URL redirects. Licence is Apache-2.0 and the repo is not archived. Last push was 2026-09-09T22:41:58Z. — `gh api repos/AMD-Ecosystem/hipDF` ([repo](https://github.com/AMD-Ecosystem/hipDF), accessed 2026-10-29)
- The last three commits on the default branch (repo-move housekeeping, nothing functional):
  - `f59924fde8165ef59588abe65b5965336e28f99a` (2026-07-29, "For AMD-Ecosystem move (#11)")
  - `8f04183440e71f95659ab19b73849717590651c1` (2026-07-14, "Update BUILD.rst")
  - `10f2afcdb5807b128ff4bc78b63285f5470cc0a1` (2026-07-07, "update ROCm 7.2.3 references for rocmds-26.03 (#465)")
  - Source: [commits](https://github.com/AMD-Ecosystem/hipDF/commits/release/rocmds-26.03) (accessed 2026-10-29)
- Branches: `release/1.0.x`, `release/rocmds-25.10`, `release/rocmds-26.03`, plus `docs/25.05`, `docs/25.10` and `docs/26.03`. There is no branch tracking cuDF 26.x. — [branches](https://github.com/AMD-Ecosystem/hipDF/branches) (accessed 2026-10-29)
- The docs at commit f59924fd say "hipDF is aligned with and is API-compatible with RAPIDS® cuDF 25.10, allowing for workloads to be transitioned to AMD devices without hipification. Support is limited to C++ and Python interfaces. A Java interface is not supported." They also say hipDF v3.0.0 supports CSV, Text, JSON, Parquet, ORC, HDF5, Feather and Avro. — [docs/what-is-hipdf.rst @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/what-is-hipdf.rst) / [rocm.docs What is hipDF](https://rocm.docs.amd.com/projects/hipDF/en/latest/what-is-hipdf.html) (accessed 2026-10-29)
- The same file lists what hipDF does **not** support (verbatim):
  - "zstd compression (zstd decompression is supported)"
  - "GPU direct storage (KvikIO, cuFile)"
  - "rocTX tracing"
  - "Per-thread default streams (PTDS)"
  - "Interoperability with Polars"
  - cuML/cuGraph/ibis/stumpy integration
  - the hipstreamz and hipdf_kafka packages
  - Source: [docs/what-is-hipdf.rst @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/what-is-hipdf.rst) (accessed 2026-10-29)

**C++ library**
- `docs/install/BUILD.rst` documents building the C++/HIP components with `./build.sh libcudf tests benchmarks`, running the tests with `ctest --test-dir cpp/build/`, and building Python with `bash build.sh libcudf pylibcudf cudf`. — [BUILD.rst @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/install/BUILD.rst) / [rocm docs BUILD](https://rocm.docs.amd.com/projects/hipDF/en/latest/install/BUILD.html) (accessed 2026-10-29)
- AMD's PyPI index for ROCm 7.2.3 ships `amd-libhipdf` (wheel `amd_libhipdf-3.0.0-py3-none-manylinux_2_28_x86_64.whl`), `amd-pylibhipdf`, `amd-hipdf`, `amd-libhipmm` and `amd-hipmm`. This is the same split as upstream's libcudf wheel, which carries `libcudf.so` plus headers (the contents of the AMD wheel are an estimate). — [pypi.amd.com/rocm-7.2.3/simple](https://pypi.amd.com/rocm-7.2.3/simple/) (accessed 2026-10-29)
- The public C++ headers keep `namespace cudf` and use `rmm::cuda_stream_view` (62 occurrences under `cpp/include/cudf/io`). The io headers include `parquet.hpp`, `parquet_metadata.hpp`, `nvcomp_adapter.hpp`, `orc.hpp`, `csv.hpp` and `json.hpp`. — sparse clone at f59924fd, [cpp/include/cudf/io](https://github.com/AMD-Ecosystem/hipDF/tree/f59924fde8165ef59588abe65b5965336e28f99a/cpp/include/cudf/io) (accessed 2026-10-29)
- A Sirius contributor reports that "hipdf itself keeps `cuda*` calls verbatim in its own source and relies on a compatibility layer at build time". — [sirius issue #1158](https://github.com/sirius-db/sirius/issues/1158) (accessed 2026-10-29)

**Build options**
- `cpp/CMakeLists.txt` @ f59924fd:
  - `CUDF_USE_WARPSIZE_32` is "Only to be used for gfx11*/gfx12* architectures (RDNA 3/4)", default OFF, so CDNA builds use wave64.
  - `CUDF_USE_PROPRIETARY_NVCOMP`, `GPU_STATIC_RUNTIME` and `GPU_ENABLE_LINEINFO` are flagged unsupported; the first two cause a fatal error.
  - `USE_NVTX` is unsupported.
  - Comments say rapids-cmake sets -O0, which "is presently not supported for hipDF".
  - Source: [cpp/CMakeLists.txt @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/cpp/CMakeLists.txt) (accessed 2026-10-29)

**Third-party dependencies (cpp/cmake/thirdparty @ f59924fd)**
- `get_nvcomp.cmake` actually calls `rapids_cpm_hipcomp`, i.e. hipCOMP.
- `get_cucollections.cmake` calls `rapids_cpm_cuco`. In AMD's rapids-cmake fork, `cuco.cmake` sets `HIP_AS_CUDA ON` and includes `hipco.cmake`, i.e. hipCollections.
- Other deps: `get_rocthrust.cmake`, `get_jitify.cmake` (`jitify-hip`), `get_kvikio.cmake` (upstream kvikio, though GDS is unsupported) and zstd 1.5.7.
- Sources: [cpp/cmake/thirdparty](https://github.com/AMD-Ecosystem/hipDF/tree/f59924fde8165ef59588abe65b5965336e28f99a/cpp/cmake/thirdparty); [ROCmDS-cmake cpm](https://github.com/AMD-Ecosystem/ROCmDS-cmake/tree/release/rocmds-26.03/rapids-cmake/cpm) (accessed 2026-10-29)
- Version pins in `versions.json` of AMD's rapids-cmake fork:
  - hipco 0.4.0 → `${rocm-org}/hipCollections` @ `release/rocmds-26.03`
  - hipmm 4.0.0 → `hipMM` @ `release/rocmds-26.03`
  - hipcomp 2.3.0 → `hipcomp-core` @ `release/rocmds-26.03`
  - libhipcxx 3.0.2, jitify-hip 0.2.0, rocthrust 4.0.0
  - Source: [ROCmDS-cmake versions.json](https://github.com/AMD-Ecosystem/ROCmDS-cmake/blob/release/rocmds-26.03/rapids-cmake/cpm/versions.json) (accessed 2026-10-29)

**Supported GPUs and OS**
- From `docs/install/hipDF-support.rst`:
  - ROCm 7.2.3; Ubuntu 24.04 and 22.04; Python 3.10–3.13.
  - Instinct MI355X (gfx950), MI325X and MI300X (gfx942), MI250 and MI210 (gfx90a).
  - Source builds need GCC 13 or later in this file; the README says GCC 14 or later.
  - Source: [hipDF-support.rst @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/install/hipDF-support.rst); [README](https://github.com/AMD-Ecosystem/hipDF) (accessed 2026-10-29)
- The ROCm-DS 26.03 release notes say hipDF has **experimental** Radeon support (gfx1100, gfx1101, gfx1200, gfx1201). — [ROCm-DS 26.03 release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/docs-26.03/about/release-notes.html) (accessed 2026-10-29)

**ROCm-DS 26.03 release notes**
- Component versions:
  - hipDF 2.0.0 → 3.0.0 ("API updates, a new pylibhipdf Python wrapper compatible with upstream pylibcudf, expanded GPU architecture support")
  - hipMM 3.0.0 → 4.0.0 (a precompiled `librmm.so`)
  - hipRAFT 1.0.0, hipVS 1.0.0, hipGRAPH 1.0.0b1 (still early access)
  - new: dask-hip 1.0.0 (a port of dask-cuda) and hip-ucxx 0.1.0 (early access)
- "hipcomp zstd GPU decompression for ORC and Parquet" was added.
- Known hipDF issues: -O0 DEBUG builds are unsupported, and some cudf.pandas configurations are unstable under heavy memory pressure (XNACK).
- Components are "aligned with upstream 25.10 APIs".
- Source: [ROCm-DS 26.03 release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/docs-26.03/about/release-notes.html) (accessed 2026-10-29)

**Release history**
- ROCm-DS was announced 2025-05-20 as an **early access** "software preview" with hipDF and hipGRAPH, "not optimized for production workloads", and with "Python and C++ APIs". — [AMD ROCm blog, Introducing ROCm-DS](https://rocm.blogs.amd.com/software-tools-optimization/introducing-rocm-ds-revolutionizing-data-processing-with-amd-instinct-gpus/README.html) (accessed 2026-10-29)
- In ROCm-DS 25.10, "hipDF and hipMM transition to production status" (GA), and multi-node / multi-GPU is experimental. — [ROCm-DS docs / 25.10 release notes (search snippet)](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/index.html) (accessed 2026-10-29)
- The current "latest" ROCm-DS release-notes page covers **26.07**, whose only new component is rocOpt 1.0.0 (a cuOpt port) on ROCm 7.2.3. It has no hipDF update. — [ROCm-DS latest release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/about/release-notes.html) (accessed 2026-10-29)

**hipMM, hipCollections, hipCOMP**
- hipMM: `AMD-Ecosystem/hipMM`, Apache-2.0, default branch `release/rocmds-26.03`, last commit `22732e49aa00ba179847e3b314371f2307f47a9a` (2026-07-29). — [hipMM](https://github.com/AMD-Ecosystem/hipMM) (accessed 2026-10-29)
- hipCollections (the cuco port): `ROCm/hipCollections`, Apache-2.0, default branch `amd-develop`, 12 stars, last commit `401e21a775bf9b4ef0bf0c8dd00b649d9dd3622f` (2026-10-22, security-scanning PR #13). — [ROCm/hipCollections](https://github.com/ROCm/hipCollections) (accessed 2026-10-29)
- hipCOMP: `ROCm/hipCOMP-core`, MIT, "based on nvCOMP release branch branch-2.2" (the last open-source nvCOMP), last commit `22cc762f54fba7cdfca74a4c50c00f2aac4ace7a` (2026-07-07).
  - Compress and decompress: Snappy, LZ4, Cascaded, RLE.
  - Experimental, decompress only: ZSTD, Deflate/GZIP.
  - Not supported (NVIDIA proprietary): Bitcomp, ANS, GDeflate.
  - Supports both HIP/AMD and HIP/CUDA backends.
  - Source: [ROCm/hipCOMP-core README](https://github.com/ROCm/hipCOMP-core) (accessed 2026-10-29)
- The `AMD-Ecosystem` org also hosts hipRaft, hipVS, rocGRAPH, hipGRAPH, dask-hip, hip-ucxx, rocmds-logger, ROCmDS-cmake, rocOpt and many ported third-party repos (xgboost, catboost, CV-CUDA, stdgpu, …). — `gh api orgs/AMD-Ecosystem/repos` ([org](https://github.com/AMD-Ecosystem)) (accessed 2026-10-29)

### Inferences
- An executor written against libcudf 25.10-era APIs should mostly compile unchanged against libhipdf: same namespace, headers, CMake targets, `rmm::` allocators and pool resources (via hipMM). The 75 listed entry points are core libcudf modules (binaryop, AST, groupby, hash/distinct/conditional/mixed joins, sort, merge, gather, stream compaction, rolling, reduce, strings, datetime, parquet, arrow interop, dictionary) that hipDF inherits from the 25.10 tree. Nothing in the docs says any of them is removed. (estimate: I did not verify each function, or its tests passing, on gfx942.)
- If the executor targets cuDF 26.x APIs, expect to backport or patch them. Sirius found at least two missing Parquet APIs (see Q2).
- Parquet compression codecs: Snappy, LZ4 and (experimental) ZSTD/GZIP decompression come from hipCOMP. Proprietary nvCOMP codecs are unavailable, which hardly matters for reading Parquet. Writing ZSTD is unsupported.
- The 5 custom CUDA kernels need hipify or a shim, and must handle wave64. Any `__shfl_*_sync`, 32-bit lane masks or `warpSize==32` assumptions break on CDNA. (estimate)
- Cadence: about two ROCm-DS releases a year (25.05, 25.10, 26.03, then 26.07 with no hipDF change). hipDF lags RAPIDS by about 2–3 releases (26.03 tracks cuDF 25.10). There have been no functional hipDF commits since July 2026. A team adopting it is pinned to an old cuDF for months at a time.

### Gaps
- I found no public per-function coverage matrix for libhipdf (which C++ tests pass or are skipped on gfx942), e.g. for conditional/mixed joins, `strings::like`, rolling windows or AST JIT paths. Checking would need the full test tree or CI logs.
- Release dates for ROCm-DS 25.10, 26.03 and 26.07 do not appear on the release-notes pages I fetched.
- I did not verify that `amd-libhipdf` actually contains `libcudf.so` plus headers.
- I found no announced hipDF release tracking cuDF 26.x.

## Q2. Sirius ROCm backend PR #1153 and issue #53

### Takeaway
PR #1153 was a single contributor's opt-in ROCm port of Sirius (a duckdb + libcudf GPU SQL engine). It used compatibility shims on top of hipDF and hipMM. It never got past "compiles on gfx942": no end-to-end build, no query run. It was moved to a standalone repo at the maintainers' request, converted to draft over conflicts, and closed by its author on 2026-09-28. Issue #53 (the Hot Aisle ask) is still open with a single skeptical reply.

### Cited Findings
**PR #1153**
- Metadata: "feat(rocm): AMD ROCm/HIP backend foundation (compat shims verified on hardware; cuCascade partial)" by `mycomputercrashsalot`. Opened 2026-07-11, closed (not merged) 2026-09-28T06:27:59Z. Head SHA at close was `ce937de9ae3a90dbec60537288b7d19fb7493e78`. The branch was force-pushed at close, and the final diff is only 2+/2− lines in 1 file. — [sirius PR #1153](https://github.com/sirius-db/sirius/pull/1153) (accessed 2026-10-29)
- Design: an `ENABLE_ROCM=ON` CMake option plus 77 shim headers that intercept CUDA headers at the preprocessor level, instead of hipifying each file:
  - `cuda_runtime.h` → hip macros
  - `cub` → hipcub with `warp_threads=64`
  - CCCL redirects
  - cooperative_groups fallback
  - nvtx3 → roctx
  - Source: [PR #1153 body](https://github.com/sirius-db/sirius/pull/1153); [RFC issue #1158](https://github.com/sirius-db/sirius/issues/1158) (accessed 2026-10-29)
- Reported status:
  - The shim layer compiles with 0 errors on gfx942 / ROCm 7.2.1.
  - The cuco `bloom_filter` and `static_set` ports compile and link.
  - An earlier claim of runtime passes on an MI300 was **retracted** as never reproduced.
  - "no full end-to-end sirius build", "no runtime query execution", and the RMM-coupled cuCascade methods still throw.
  - The CI full-build job is disabled.
  - "both amd test boxes went offline".
  - Source: [PR #1153](https://github.com/sirius-db/sirius/pull/1153) (accessed 2026-10-29)
- Version mismatch: hipDF 26.03 equals cuDF 25.10, but Sirius pins `libcudf=26.06.*` and uses `hybrid_scan_reader::all_column_chunks_byte_ranges()` and `cudf::io::parquet::fetch_footer_to_host`, both absent in hipDF. The contributor wrote `scripts/hipdf_26.06_api_patch.sh` to backport them. Maintainer `ran-yuan-rui` raised the issue and asked them to port the real cuCascade rather than stubs. — [issue #1158 comments](https://github.com/sirius-db/sirius/issues/1158) (accessed 2026-10-29)
- Timeline of the closure:
  - 2026-07-15: "per our slack discussion, moving the rocm/hip backend work to a standalone repo instead of merging into core sirius". The PR was reopened as a pointer. Per Xiangyao Yu, the work could be hosted under sirius-db "once it's benchmarked".
  - 2026-08-26: maintainer `mbrobbel` marked it draft ("There are some conflicts").
  - 2026-09-28: the author closed it.
  - No formal reviews were recorded.
  - Source: [PR #1153 comments/events](https://github.com/sirius-db/sirius/pull/1153) (accessed 2026-10-29)
- The standalone repos:
  - `mycomputercrashsalot/sirius-rocm` (formerly hamstergamerszhang-ops), last commit `a24f2c0f1aa4d692d8b50b64426e6928eaee6a84` (2026-10-26), 0 stars. Its README status: "Sirius end-to-end build — Not yet built (blocked on hipDF completion)", "TPC-H benchmark — Not yet run", and the blocker is "rented boxes dying mid-build". — [sirius-rocm](https://github.com/mycomputercrashsalot/sirius-rocm) (accessed 2026-10-29)
  - `mycomputercrashsalot/cuda2rocm` (the URL the brief guessed is correct), last commit `0b86950669129046d69c33e38a7ea7c96cdfee62` (2026-10-27), 0 stars. It holds the shim headers, a cuCollections port (bloom_filter, static_set) and a cuCascade port, and claims "compile-verified" only. — [cuda2rocm](https://github.com/mycomputercrashsalot/cuda2rocm) (accessed 2026-10-29)

**Issue #53**
- "[feature] Support for AMD GPUs" by `jon-hotaisle` (Hot Aisle), opened 2025-06-28, **still open**. It argues that MI300's 192GB+ eases Sirius's GPU-memory limit, and suggests HIPIFY or SCALE. — [sirius issue #53](https://github.com/sirius-db/sirius/issues/53) (accessed 2026-10-29)
- The only reply, from `datametrician` on 2025-07-01: "Is GPU memory really a limiting factor if they plan to go Multi-Node, Multi-GPU? From my experience at real workloads 1TB+ scale out out performs scale up when managing memory." — [sirius issue #53](https://github.com/sirius-db/sirius/issues/53) (accessed 2026-10-29)

### Inferences
- Even a motivated contributor with access to gfx942 could not finish a hipDF build plus link of a libcudf-based engine in about 3 months. The practical blockers were build infrastructure (unstable rented boxes, a large CUDA→HIP build) and the cuDF version gap, not missing kernels.
- Sirius maintainers are not hostile but are not investing in AMD. They want measured numbers before upstreaming.

### Gaps
- The Slack discussion behind "move to a standalone repo" is not public.
- There is no data on hipDF build time or memory on gfx942.

## Q3. Other AMD data-analytics efforts and CUDA-on-AMD tooling

### Takeaway
AMD's RAPIDS equivalent is ROCm-DS: hipDF, hipMM, hipRAFT, hipVS, hipGRAPH (early access), dask-hip, hip-ucxx and rocOpt. None of these is a SQL engine. I found no Spark-RAPIDS, HeavyDB, Kinetica, BlazingSQL or PG-Strom support for AMD GPUs. The transpile-or-emulate routes are:
- **HIPIFY**: active.
- **SCALE**: commercial; targets gfx942/950; its validation table shows thrust failing everywhere, and cudf has a test dir but no published status.
- **ZLUDA**: v7 previews; a hobby project mainly aimed at consumer GPUs.

### Cited Findings
**ROCm-DS components**
- ROCm-DS 26.03 lists hipDF 3.0.0, hipMM 4.0.0, hipGRAPH 1.0.0b1 (early access), hipRAFT 1.0.0, hipVS 1.0.0, dask-hip 1.0.0 and hip-ucxx 0.1.0. — [ROCm-DS 26.03 release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/docs-26.03/about/release-notes.html) (accessed 2026-10-29)
- ROCm-DS 26.07 adds rocOpt 1.0.0, a port of cuOpt whose barrier solver is missing because cuDSS has no ROCm equivalent. — [ROCm-DS latest release notes](https://rocm.docs.amd.com/projects/rocm-ds/en/latest/about/release-notes.html) (accessed 2026-10-29)
- Mixing RAPIDS and ROCm-DS packages "is not supported", and there is no hipDF Docker image. — [hipDF README](https://github.com/AMD-Ecosystem/hipDF) (accessed 2026-10-29)

**Spark and the GPU databases**
- I found no evidence of a Spark RAPIDS plugin for AMD. hipDF explicitly does not support Java, and Spark-RAPIDS depends on cuDF's JNI. — [what-is-hipdf.rst @ f59924fd](https://github.com/AMD-Ecosystem/hipDF/blob/f59924fde8165ef59588abe65b5965336e28f99a/docs/what-is-hipdf.rst) (accessed 2026-10-29)
- HeavyDB (local clone at `72c6df9f1bce2cd5b849670b38c7e7e4592c564b`, 2026-10-07): a grep for `hipMalloc` / `__HIP_PLATFORM` / ROCm found no HIP backend. My grep's single "hit" in DataMgr.cpp was a false positive. — [heavyai/heavydb](https://github.com/heavyai/heavydb) (accessed 2026-10-29)
- PG-Strom, Kinetica and BlazingSQL: searches surfaced no AMD/ROCm support (BlazingSQL has been archived for years; that is background knowledge, not checked this session).

**HIPIFY**
- `ROCm/HIPIFY` is active: last commit `b743f9062b5e051355b327054b61e97f829bcbe1` on 2026-10-29. — [ROCm/HIPIFY](https://github.com/ROCm/HIPIFY) (accessed 2026-10-29)

**SCALE (Spectral Compute)**
- An nvcc-compatible compiler plus a CUDA runtime/driver API implementation and "CUDA-X" wrapper libraries that delegate to ROCm. Its docs list CDNA gfx950, gfx942, gfx90a, gfx908, gfx906 and gfx900, plus RDNA gfx1010–gfx1201. — [SCALE docs](https://docs.scale-lang.com/stable/) (accessed 2026-10-29)
- Phoronix launch coverage — [Phoronix](https://www.phoronix.com/news/SCALE-CUDA-Apps-For-AMD-GPUs) (accessed 2026-10-29)
- A Spectral engineer said SCALE uses no ZLUDA code. — [HN](https://news.ycombinator.com/item?id=40971561) (accessed 2026-10-29)
- SCALE validation repo (last commit `a77f241b7f5bdc717bb49191db3db45499c9acaa`, 2026-09-15):
  - Has a `cudf/` test directory, but cudf has no row in the README status table.
  - `thrust` (756c5af) shows ❌ on every target (gfx90a, gfx900, gfx1201, gfx1100, gfx1030, sm_120).
  - `cuml` shows ✅ on all of them. `jitify` and `stdgpu` pass on RDNA.
  - The table has **no gfx942 column**.
  - Source: [spectral-compute/scale-validation](https://github.com/spectral-compute/scale-validation) (accessed 2026-10-29)
- A 2026 blog repeats SCALE's claim of up to about 6× over HIPIFY-converted code, and notes that no independent reproduction exists. — [rajatpandit.com](https://rajatpandit.com/ai-infrastructure/cuda-monopoly-breaks-scale-amd/) (accessed 2026-10-29; secondary, unverified)

**ZLUDA**
- `vosen/ZLUDA`: Apache-2.0, 15.1k stars, latest pre-releases `v7-preview.19` (2026-10-26) and `v7-preview.18` (2026-10-20). — [vosen/ZLUDA releases](https://github.com/vosen/ZLUDA/releases) (accessed 2026-10-29)
- Lost commercial funding with v6 and returned to "weekend hobby project" status. — [Tom's Hardware](https://www.tomshardware.com/pc-components/gpu-drivers/cuda-emulator-for-amd-gpus-zluda-loses-funding-with-v6-release-embattled-project-goes-back-to-hobby-status-but-now-includes-32-bit-physx-support) (accessed 2026-10-29)

### Inferences
- For a libcudf-based executor, hipDF is the only realistic AMD target.
- SCALE would have to compile all of libcudf, rmm, cuco and CCCL itself. Its own validation shows thrust failing, so treat it as not viable for libcudf today. (estimate)
- ZLUDA targets PTX-level binary compatibility on consumer GPUs and is irrelevant to an MI300X data-center deployment. (estimate)

### Gaps
- SCALE pricing and licence terms, and SCALE results on gfx942 for cudf.
- Any AMD "GPU SQL" or Spark-accelerator roadmap. I found none.
- I did not fetch Kinetica's docs.

## Q4. AMD GPU specs and cloud pricing vs H100

### Takeaway
Memory per GPU:

| GPU | HBM | Bandwidth |
|---|---|---|
| MI300X | 192 GB HBM3 | ~5.3 TB/s |
| MI325X | 256 GB HBM3E | ~6 TB/s |
| MI355X | 288 GB HBM3E | 8 TB/s |
| H100 SXM (background, not fetched) | 80 GB | ~3.35 TB/s |

MI300X rents for about $1.7–2/GPU-hr at neoclouds and about $6 normalized on Azure. That is roughly at parity with H100 per GPU-hour and much cheaper per GB of HBM.

### Cited Findings
**Specs**
- MI300X: 192 GB HBM3, about 5.3 TB/s, 750 W. MI325X: 256 GB HBM3E, about 6 TB/s, 1000 W (AMD first announced 288 GB). — [SemiAnalysis InferenceX MI325X vs MI300X](https://inferencex.semianalysis.com/chips/mi325x-vs-mi300x); [Glenn Lockwood MI325X](https://glennklockwood.com/garden/processors/MI325X) (accessed 2026-10-29)
- MI355X: 288 GB HBM3E, 8 TB/s, CDNA 4, adds FP4/FP6; MI350X is the air-cooled sibling. — [TechRadar](https://www.techradar.com/pro/amd-releases-details-of-288gb-mi355x-accelerator-80-percent-faster-than-mi325x-8tb-s-memory-bandwidth); [Tom's Hardware](https://www.tomshardware.com/pc-components/gpus/amd-announces-mi350x-and-mi355x-ai-gpus-claims-up-to-4x-generational-gain-up-to-35x-faster-inference-performance) (accessed 2026-10-29)

**MI300X pricing**
- MI300X on-demand across 5 providers ranges from $2.59 to $3.99/GPU-hr, median $3.45 (survey dated 2026-10-17). Azure ND MI300X v5 is about $6.00/GPU-hr normalized from its 8-GPU node. One-year reserved runs about 35% below the on-demand median. — [amcompute MI300X](https://www.amcompute.com/gpu-cloud-prices/mi300x) (accessed 2026-10-29; aggregator)
- Neocloud prices:
  - TensorWave MI300X about $1.71/GPU-hr (not all pages dated) — [gpus.io MI300](https://gpus.io/en/gpus/mi300); [Thunder Compute](https://www.thundercompute.com/blog/amd-mi300x-pricing)
  - Hot Aisle $1.99/hr for 1 GPU, updated mid-September 2026 — [cloudgpuprice](https://cloudgpuprice.com/gpu/amd-mi300x/)
  - Vultr spot $1.85/hr — [computeprices](https://computeprices.com/gpus/mi300x)
  - (all accessed 2026-10-29; aggregators, may be stale)

**H100 pricing**
- On-demand medians are about $2.94/GPU-hr (gpus.io, 49 configs) and $3.29 (Flopper, 33 providers). Cheapest offers are about $1.33–1.83. Hyperscaler VMs run $6.98–11.06 per GPU. Akash says 1-year contracts rose from $1.70 to $2.35 between October 2025 and March 2026. — [gpus.io H100](https://gpus.io/gpus/h100); [Flopper](https://flopper.io/pricing/nvidia-h100); [Beam](https://www.beam.cloud/blog/nvidia-h100-pcie-pricing); [Akash](https://akash.network/the-bid/h100-rental-price-2026-cost-per-hour/) (accessed 2026-10-29; aggregators, no source dated October 2026)

### Inferences
- MI300X costs about the same per GPU-hour as H100 but offers about 2.4× the HBM capacity and about 1.6× the bandwidth. That favors single-GPU scale-up for out-of-core SQL workloads, which is exactly Hot Aisle's argument in Sirius #53. (estimate, using H100 SXM 80 GB / 3.35 TB/s background figures)

### Gaps
- I fetched no source on Infinity Fabric. My background knowledge says MI300X has 7 xGMI links at about 128 GB/s each, 896 GB/s aggregate peer bandwidth, in a fully connected 8-GPU OAM board. Treat that as unverified.
- No official AMD spec-sheet page was fetched. The spec numbers come from secondary sources.

## Q5. Measured GPU SQL / dataframe benchmarks on MI300X

### Takeaway
I found no published GPU SQL or dataframe benchmark (TPC-H, TPC-DS, hipDF vs cuDF) on MI300X. AMD's own hipDF announcement contains no numbers, the Sirius ROCm port never ran a query, and the only hipDF measurement I located is an HPC trace-analysis paper on Frontier (MI250X).

### Cited Findings
- The ROCm-DS launch blog (2025-05-20) makes general performance claims but gives no hipDF vs pandas figures and no MI300X results. — [AMD ROCm blog](https://rocm.blogs.amd.com/software-tools-optimization/introducing-rocm-ds-revolutionizing-data-processing-with-amd-instinct-gpus/README.html) (accessed 2026-10-29)
- Sirius-rocm: "TPC-H benchmark — Not yet run". — [sirius-rocm](https://github.com/mycomputercrashsalot/sirius-rocm) (accessed 2026-10-29)
- arXiv 2605.03561 (Grbic et al., May 2026), "Enhancing Performance Insight at Scale…": per a search snippet it compares pandas and hipDF groupby/sort/filter/merge on Frontier, and cuDF on Polaris. The abstract claims up to 314× speedup over CPU for 100k traces. I could not extract the hipDF numbers. — [arXiv 2605.03561](https://arxiv.org/abs/2605.03561) (accessed 2026-10-29)
- A Voltron Data blog post (about 2025) called hipDF "still early with limited operator coverage", with no query-plan optimization or real-world validation. That predates the 25.10 GA release. — [Voltron Data blog](https://voltrondata.com/blog/the-race-to-build-a-distributed-gpu-runtime) (accessed 2026-10-29; search snippet only)
- An AMD blog shows CuPy and hipDF for portfolio optimization, with no SQL benchmarks. — [ROCm blog CuPy/hipDF](https://rocm.blogs.amd.com/artificial-intelligence/cupy_hipdf_portfolio_opt/README.html) (accessed 2026-10-29; not fetched in full)

### Inferences
- A team porting to hipDF would be producing the first public GPU-SQL numbers on MI300X. Performance of hash joins and groupby with hipCollections on wave64 is unknown. Budget for profiling and tuning, not just a recompile. (estimate)

### Gaps
- I found no hipDF C++ benchmark results (the repo has a `cpp/benchmarks` tree using hipbench).
- I found no MI300X vs H100 libcudf comparison.
- The hipDF table in arXiv 2605.03561 was not extracted.
