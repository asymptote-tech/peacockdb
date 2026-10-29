# Sirius as a DuckDB core extension; Sirius on desk/workstation GPUs (state as of 2026-10-29)

Method: GitHub API via `gh` (PR/issue/release metadata, diffs), a blob-less clone of sirius-db/sirius (main = `b9e05ffc653c8ed031c789e5c1415ac1186e0619`, 2026-10-28), and duckdb/duckdb-web sources. Direct `curl` to duckdb.org failed from the sandbox, so the docs page was read from its source file in duckdb/duckdb-web. All accessed 2026-10-29.

## Q1. DuckDB PR #28022 "Add Sirius as a core extension": what it adds, state, author, files, platforms, CUDA, archs, distribution, DuckDB version

### Takeaway
PR #28022 is open, has no reviews, and is small (+170/−18, 8 files). It adds only a CUDA-free `sirius` *loader* to DuckDB's core-extension build on Linux glibc (amd64/arm64). When you run `LOAD sirius`, the loader checks the NVIDIA driver's CUDA version and runs `INSTALL AND LOAD sirius_cuda12|sirius_cuda13 FROM community`. The heavy GPU backends, which bundle cuDF and friends statically, will ship as **community** extensions, not core. The PR is labelled for backport to v2.0 ("cyanoptera"), and the backends are built against DuckDB v2.0.0. An NVIDIA engineer who is a DuckDB-org member is driving the whole effort.

### Cited Findings
- **State and metadata:** title "Add Sirius as a core extension". State OPEN, not merged. Created 2026-10-28T13:20:47Z. Base `main`, head branch `sirius-core-extension` @ `4c8b4c22512c822ed812a77ca239a5de6371c1bf`. Labels: "Member" and "backport v2.0-cyanoptera". No reviews and no review requests. 0 comments. Timeline shows 9 commits and a ready_for_review event. +170/−18. — [duckdb/duckdb#28022](https://github.com/duckdb/duckdb/pull/28022) (accessed 2026-10-29)
- **Author:** `smvv` (Sander Mathijs van Veen). He is a public member of the `duckdb` GitHub org (`GET orgs/duckdb/members/smvv` → 204). His GitHub profile still says "Compiler Engineer @LeaningTech". The profile looks stale relative to the org membership, so affiliation is uncertain from public data. — [github.com/smvv](https://github.com/smvv) (accessed 2026-10-29)
- **Description, verbatim intent:** "add the lightweight Sirius loader to DuckDB's existing core extension group on supported Linux targets"; "preserve extension submodule lists and locate vcpkg manifests below extension source directories"; "preserve vcpkg configuration fields, overlays, and dependency overrides when merging manifests"; "keep `sirius_cuda12` and `sirius_cuda13` out of the core build; they will be distributed as community extensions". It says "Sirius is currently pinned to `5350b8f951c32cb79f4b1971a48c5915f0a6908f`." — [PR #28022](https://github.com/duckdb/duckdb/pull/28022) (accessed 2026-10-29)
- **Files touched:**
  - `.github/config/core.cmake` (+1, includes `sirius.cmake`)
  - `.github/config/extensions/sirius.cmake` (+11, new)
  - `extension/extension_build_tools.cmake` (switches to `cmake_parse_arguments(PARSE_ARGV 1 …)` so empty or list `SUBMODULES` survive)
  - `scripts/ci/image_version.txt` (CI image `20260923-ade72a46` → `20261019-8302ae16`)
  - `scripts/ci/test_sync_out_of_tree_extensions.py` (+99)
  - `scripts/extension-upload-single.sh` (+1)
  - `scripts/merge_vcpkg_deps.py` (+22/−8, merges vcpkg `overrides`, e.g. Sirius's dlpack 0.8 and flatbuffers 24.3.25)
  - `scripts/sync_out_of_tree_extensions.py` (+34/−8, adds `source_dir` so the vcpkg manifest is found under `sirius-duckdb/`)

  — [PR #28022 files](https://github.com/duckdb/duckdb/pull/28022/files) (accessed 2026-10-29)
- **Platform gate and pin:** the new `sirius.cmake` returns early unless `CMAKE_SYSTEM_NAME STREQUAL "Linux"` and not `MUSL_ENABLED`. That means no Windows, no macOS, no musl and no wasm. It loads `sirius` from `GIT_URL https://github.com/smvv/sirius`, `GIT_TAG bb247c9dbb67739d125fc65149085bf0701d4852`, `SOURCE_DIR sirius-duckdb/loader`, `SUBMODULES ""`.
  - Discrepancy: the cmake pins the author's fork at `bb247c9`, which is the head of sirius-db/sirius#2398. The PR text says `5350b8f`. — [PR #28022 diff](https://github.com/duckdb/duckdb/pull/28022/files) (accessed 2026-10-29)
- **Related PRs:**
  - duckdb/extension-ci-tools#458 "Support Sirius CUDA extension builds". MERGED 2026-10-28T12:52:19Z, head `ca06b42b58633fee9e880cfec0276eacdad55e71`. It forwards `CI_CUDA_VERSION` → `VCPKG_CUDA_VERSION` and `CI_CUDA_ARCHS` → `CUDAARCHS`/`VCPKG_CUDA_ARCHITECTURES`. — [extension-ci-tools#458](https://github.com/duckdb/extension-ci-tools/pull/458) (accessed 2026-10-29)
  - duckdb/duckdb-ci#86 "Improve CUDA extension build images and settings". MERGED 2026-10-19T14:22:57Z, head `6ad16adb1ce41fdf84c036e9ab51cc01e1642f0c`. It adds `manylinux_2_28` amd64 and aarch64 CUDA Docker images and pins "CUDA 12.9 and 13.4 component package streams". The images install cuda-nvcc, cudart, driver-devel, nvml, curand, nvjitlink and nvrtc. The build matrix gains `cuda_version` ("12"/"13") and an optional `cuda_archs`. — [duckdb-ci#86](https://github.com/duckdb/duckdb-ci/pull/86) (accessed 2026-10-29)
  - sirius-db/sirius#2398 "fix(loader): install CUDA backends from community". OPEN, created 2026-10-29, head `bb247c9dbb67739d125fc65149085bf0701d4852`. It redirects the default core auto-install repository to DuckDB's community repository for the backends and uses normal signature resolution so community-signed backends verify. — [sirius#2398](https://github.com/sirius-db/sirius/pull/2398) (accessed 2026-10-29)
  - duckdb/community-extensions#3447 "Add Sirius CUDA 12 community extension". OPEN, body says "*DO NOT MERGE*", head `3fa6aa59618e6031271e7a1ef6e43e286f51ccd8`. It adds `extensions/sirius_cuda12/description.yml`:
    - "Linux community extension for DuckDB v2.0.0", maintainer `mbrobbel`, `requires_toolchains: cuda`
    - `excluded_platforms` lists every musl, osx, windows and wasm target, so only linux_amd64 and linux_arm64 remain
    - `cuda_version: "12"`, `cuda_archs: "80-real"` (only sm_80 in this draft)
    - `SIRIUS_DUCKDB_LINKAGE static`, built from sirius-db/sirius @ `5350b8f951c32cb79f4b1971a48c5915f0a6908f` with submodules `duckdb;cucascade;substrait`
    - hardware_requirements: "Linux on x86-64 or arm64 with an NVIDIA GPU and a driver supporting CUDA 12"
    - The workflow defaults `cuda_version` to '13'. No `sirius_cuda13` description.yml was in this PR.

    — [community-extensions#3447](https://github.com/duckdb/community-extensions/pull/3447) (accessed 2026-10-29)
- **Loader logic** (sirius `sirius-duckdb/loader/src/sirius_loader.cpp` @ `bb247c9`, 92 lines):
  - It `dlopen("libcuda.so.1")` and calls `cuDriverGetVersion`.
  - If the reported version is below 12000, it errors: "Sirius requires an NVIDIA driver supporting CUDA 12 or newer".
  - Versions below 13000 select `sirius_cuda12`; otherwise `sirius_cuda13`.
  - It refuses to load if the other backend is already loaded, then runs `INSTALL AND LOAD <backend> FROM community`.
  - It does not check GPU compute capability.

  — [sirius_loader.cpp @ bb247c9](https://github.com/sirius-db/sirius/blob/bb247c9dbb67739d125fc65149085bf0701d4852/sirius-duckdb/loader/src/sirius_loader.cpp) (accessed 2026-10-29). The loader was introduced by sirius#2178 (commit `1493e00`). — [sirius commit 1493e00](https://github.com/sirius-db/sirius/commit/1493e00) (accessed 2026-10-29)
- **sirius-duckdb README on main** (`b9e05ff`):
  - Three artifacts: "`sirius`: a CUDA-free loader … with no Sirius package or CUDA toolkit dependency", "`sirius_cuda12`", "`sirius_cuda13`".
  - "These are Linux glibc amd64/arm64 SBSA builds; Jetson/Tegra is not supported. The loader does not detect or reject unsupported GPU architectures."
  - "`LOAD sirius` reuses an installed backend; it does not update it or require its Sirius version to match the loader."
  - "Existing direct-extension installations must use `FORCE INSTALL sirius FROM core` when switching."
  - The README on main still says "Community signatures are not supported". PR #2398 changes that.

  — [sirius-duckdb/README.md @ b9e05ff](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md) (accessed 2026-10-29)
- **Static distribution build:** "The final extension link bundles those libraries; the post-link check rejects unexpected runtime dependencies. NVIDIA driver libraries and standard Linux libraries remain external." vcpkg overlay ports include cudf, cuvs, cuco, cccl, cublas, curand, cusolver, cusparse and culibos. In other words, the backend bundles libcudf and needs no conda env at runtime. — [sirius-duckdb/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md); [vcpkg_ports](https://github.com/sirius-db/sirius/tree/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/vcpkg_ports) (accessed 2026-10-29)
- **GPU architectures in Sirius's own distribution CI** (`.github/workflows/distribution.yml` @ `b9e05ff`):
  - CUDA 13: `75-real;80-real;86-real;89-real;90a-real;100f-real;120a-real;120`
  - CUDA 12: `75-real;80-real;86-real;89-real;90a-real;100-real;120-real`
  - The pixi.toml comment says "Turing through Blackwell (requires CUDA 12.8 or newer)" for cuda12. Pixi pins `cuda-version = "13.4.*"` and `"12.*"`, `libcudf ==26.10.02`, `libcuvs ==26.10.00`.

  — [distribution.yml](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.github/workflows/distribution.yml); [pixi.toml](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/pixi.toml) (accessed 2026-10-29)

### Inferences
- "Core extension" here means only the ~100-line dispatcher. The GPU engine itself (cuDF and friends) goes through the community repository, maintained by NVIDIA's `mbrobbel`. DuckDB-signed core status does not cover the engine binary.
- The target is DuckDB 2.0: the backport label, the community description says "DuckDB v2.0.0", and the Sirius `latest` release requires DuckDB 2.0.0. DuckDB 1.5 is not targeted by the core loader.
- For consumer cards the relevant SASS targets are sm_75 (RTX 20), sm_86 (RTX 30), sm_89 (RTX 40) and sm_120 (RTX 50), and all of them are in Sirius's own release builds. The draft community description builds only `80-real`. If it ships unchanged, Turing/Ada/Blackwell consumer GPUs would get no matching cubin, and sm_75 is below 80, so it would fail outright. This is a draft and is likely to change.

### Gaps
- There is no reviewer activity yet, so there is no DuckDB Labs position on merge timing.
- No `sirius_cuda13` community description exists yet, and the final arch list is unknown.
- Download size through the community repository is not stated anywhere. See Q3 for the GitHub release sizes.

## Q2. DuckDB docs page /docs/2.0/core_extensions/sirius

### Takeaway
The page is a placeholder: "The `sirius` extension is not yet available. It is coming soon." It has no install instructions or requirements.

### Cited Findings
- The full body: a callout "The `sirius` extension is not yet available. It is coming soon." and one sentence: "The `sirius` extension adds GPU-accelerated query execution to DuckDB. Sirius is a GPU-native SQL engine that runs DuckDB queries on NVIDIA GPUs." Redirects exist from /docs/current, /lts, /preview and /stable. — [duckdb-web source docs/2.0/core_extensions/sirius.md](https://github.com/duckdb/duckdb-web/blob/main/docs/2.0/core_extensions/sirius.md), rendered at [duckdb.org/docs/2.0/core_extensions/sirius](https://duckdb.org/docs/2.0/core_extensions/sirius) (accessed 2026-10-29)
- The core-extensions overview table lists `sirius` as "GPU-accelerated query execution (coming soon)", maintainer "Third party", with no support tier. — [docs/2.0/core_extensions/overview.md](https://github.com/duckdb/duckdb-web/blob/main/docs/2.0/core_extensions/overview.md) (accessed 2026-10-29)
- The latest commits touching the file are `aba78b1aee37c713175e17949515ca3b9aa2b95c` (2026-10-20, "Move docs/preview to docs/2.0") and `8d67e5bd3c4881d1217d124673c251b4e1d75503` (2026-10-29, redirects), both by Gabor Szarnyas. — [duckdb-web commits](https://github.com/duckdb/duckdb-web/commits/main/docs/2.0/core_extensions/sirius.md) (accessed 2026-10-29)

### Inferences
- DuckDB Labs has publicly reserved the core slot (a "Third party" core extension). The install surface is not live yet.

### Gaps
- I could not fetch the rendered duckdb.org page directly (sandbox network). The content above is from the Markdown source.

## Q3. Sirius issue #609 (performance on GB10, RTX 5090, A6000, A100; the RMM bug)

### Takeaway
The reporter found Sirius slower than CPU DuckDB on all 22 TPC-H queries at SF10/SF100, on GB10, RTX 5090 and A6000. That report gave no per-query numbers. A second user on an A100 40GB traced two problems: a cold-CPU-baseline methodology error (1.45× → 9.27×), and a cuDF 26.04 bug that bypassed the RMM pool. Patching the bug raised speedup to about 11.3–11.7×. The cuDF/RMM bug is fixed upstream, and current Sirius main pins cuDF 26.10.02. The issue was closed "completed" by an NVIDIA maintainer without a written reply.

### Cited Findings
- **Opening report** by jingnanquan, 2026-04-10: "on the Nvidia PGX (GB10) platform … for all 22 TPC-H experiments, Sirius was slower than the CPU implementation on DuckDB". They used `gpu_execution`, one warmup and an average of 5 runs, and "also conducted experiments on the 5090 and A6000, and Sirius performed even slower". "Due to the limitation of the GPU's memory, I only attempted sf10 and sf100." No numeric per-GPU timings were given. — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- **A100 40GB SXM4, SF100** (liuweibupt, 2026-05-21): Sirius was "consistently slower than DuckDB running on a 32-core CPU across 8/22 TPC-H queries, with an average speedup of only 0.83×". Nsight showed many bare `cudaMalloc` calls. They noted that GPUBufferManager (legacy) and cuCascade coexist with independent accounting, and argued this hurts on 40 GB but not on GH200/H200. — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- **With the official `run_tpch_parquet.sh` and `SIRIUS_DISABLE=1`** (12-core CPU): geomean 1.45×, with 18/22 queries faster on GPU. Q1, Q2, Q6 and Q11 were still slower. `cudaMallocFromPoolAsync` made 1,719 calls versus 949 synchronous `cudaMalloc` calls. — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- **CPU warm-up methodology:** the official script runs CPU queries once (cold). Q1 CPU was cold about 2.1 s versus warm about 0.4 s. A warm-vs-warm comparison moved the speedup from about 1.45× to about 9.27×. — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- **RMM bug:** `cudf::get_current_device_resource_ref()` in `cpp/include/cudf/utilities/memory_resource.hpp` (cuDF v26.04.00) returned `rmm::mr::get_current_device_resource()`, which reads RMM's pointer map. Sirius sets the ref map. The result was that `device_buffer` allocations fell back to bare `cudaMalloc`, at about 2,363 µs/call versus about 67 µs for pool allocations. They applied a 1-line fix.
  - TPC-H SF100 22-query total, cold: 25.428 → 18.682 s. Warm: 22.525 → 18.021 s.
  - Speedup versus CPU: cold 8.27× → 11.26×, warm 9.34× → 11.67×.
  - Q1 cold: 2.871 → 1.461 s. Q21 cold: 4.679 → 1.812 s.

  — [sirius#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- **Q1 alone, after the fix:** Sirius 1.461 s cold / 1.429 s warm versus DuckDB CPU 2.383 s / 0.634 s, i.e. 1.63× / 2.26× faster. Before the fix it was 0.83× / 0.28× (slower). The user attributes the bug to cuDF, not Sirius. cuDF v26.08 headers give equivalent totals (cold 18.767 s, warm 18.651 s). The fix is cuDF PR #20688 "Remove cudf::get_current_device_resource". — [sirius#609](https://github.com/sirius-db/sirius/issues/609); [rapidsai/cudf#20688](https://github.com/rapidsai/cudf/pull/20688) (accessed 2026-10-29)
- **Responses:** bdice (2026-05-26, RAPIDS developer, author of cudf#20688, no repo association) said: "RMM's pointer-based map is removed in RMM 26.06 so there is only one source of truth … You should be fine to upgrade to the final 26.06". No Sirius maintainer commented. The issue was closed as "completed" 2026-06-23 by `wmalpica`, whose GitHub company is "Nvidia". — [sirius#609](https://github.com/sirius-db/sirius/issues/609); [github.com/wmalpica](https://github.com/wmalpica) (accessed 2026-10-29)
- **Current engine:** Sirius main pins `libcudf ==26.10.02` (commit `b9e05ffc653c8ed031c789e5c1415ac1186e0619` "chore: bump cudf to 26.10.02", #2359). That is past the 26.06/26.08 fix. — [pixi.toml @ b9e05ff](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/pixi.toml) (accessed 2026-10-29)

### Inferences
- The specific RMM bypass should not affect current builds (cuDF 26.10). The dual-allocator concern on small-VRAM GPUs was never answered by a maintainer.
- The issue implies the "about 10×" headline depends on a warm GPU (data resident) and an A100-class GPU. Consumer and workstation results (5090, A6000, GB10) were reported only qualitatively, as "slower", on an older dev build, and were never re-measured in the thread.

### Gaps
- There are no published numbers for GB10, RTX 5090 or A6000, and no rerun on the current engine.
- There is no maintainer statement on whether Super Sirius (cuCascade, out-of-core) has fixed small-VRAM behaviour.

## Q4. Sirius #2174 (silent CPU fallback), install requirements, binary size, pip/conda; #1728 and #1734

### Takeaway
Today Sirius installs as a manually downloaded, unsigned, gzipped GitHub-release `.duckdb_extension`. The archive is about 0.9–1.5 GB compressed, built for one exact DuckDB version (`latest` = DuckDB 2.0.0, `stable` = DuckDB 1.5.6), and loaded with `-unsigned`. It needs Linux glibc ≥2.28, sm_75+, CUDA 12 or 13 drivers, and io_uring, and no conda at runtime. With the official DuckDB CLI on `latest`, every Parquet scan silently falls back to CPU. The fix PR is still open. Packaging Sirius as a standalone library or conda package is tracked but open.

### Cited Findings
- **#2174** "Official DuckDB CLI: every Parquet scan falls back to the CPU (no trusted reference definition)". OPEN, by knkarthik01, 2026-10-14. Setup: EMR g7.4xlarge with an NVIDIA RTX PRO 4500, TPC-DS/TPC-H SF100.
  - On `stable`, some decimal sums were wrong (fixed by #1943). On `latest`, answers were correct but "every query quietly ran on the CPU".
  - Cause: since #1956, Sirius `dlsym`s `TableScanFunction::GetFunction` / `ParquetScanFunction::GetFunctionSet` from the host. The official CLI exports no DuckDB symbols (its dynsym has 450 entries, all libc/libstdc++).
  - With `SET enable_duckdb_fallback=false` the query errors with "Table function 'read_parquet' is not supported in Sirius (unverified callbacks…)". The Python package works because it exports symbols, and pixi builds use `EXPORT_DYNAMIC_SYMBOLS=ON`. A v2 CLI would hit the same problem.
  - `latest` was 0ef5a30 (2026-10-10). `stable` 8d17afe predates #1956.
  - Fix PR #2175 "fix(planner): resolve host scan references from the static symbol table" is OPEN, not merged. There is no maintainer comment on the issue.

  — [sirius#2174](https://github.com/sirius-db/sirius/issues/2174); [sirius#2175](https://github.com/sirius-db/sirius/pull/2175) (accessed 2026-10-29)
- **Requirements** (docs/README.md @ `b9e05ff`):
  - Linux on amd64/x86_64 or arm64/aarch64 with `glibc >= 2.28`
  - NVIDIA Turing or newer, compute capability 7.5+
  - CUDA 13.x (driver 580.65.06 or newer) or CUDA 12.x (driver 525.60.13 or newer)
  - `io_uring` enabled at runtime (`kernel.io_uring_disabled=0` recommended; containers must allow `io_uring_setup`, `io_uring_enter`, `io_uring_register`)
  - Local Parquet files on a filesystem/block device that supports `O_DIRECT`
  - Build needs Git and Pixi
  - Defaults: "95% GPU memory, 8 GiB pinned host memory per NUMA node"
  - "Queries with unsupported operators fall back silently to CPU."

  — [docs/README.md @ b9e05ff](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-29)
- **Python:** "The v2 extension requires the exact DuckDB revision pinned in this repository … The default environment's DuckDB 1.5 Python package cannot load this extension; Python use requires a package built from the matching revision." In practice `pip install duckdb` plus `LOAD sirius` does not work with `latest`, and there is no pip wheel for Sirius. — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-29)
- **Release assets** (rolling tags):
  - `latest` (published 2026-10-02T21:20:13Z, target `84fbc1f569eea9166b9942bbf13a451fbb3c3527`; the release name says 73ed20b) needs DuckDB v2.0.0:
    - amd64-cuda12 .gz: 1,394,139,296 B (≈1.39 GB)
    - amd64-cuda13: 949,635,245 B
    - arm64-cuda12: 1,381,847,496 B
    - arm64-cuda13: 1,052,135,174 B
  - `stable` (8d17afe) needs DuckDB v1.5.6:
    - amd64-cuda12: 1,540,150,020 B
    - amd64-cuda13: 921,613,272 B
    - arm64-cuda12: 1,528,461,003 B
    - arm64-cuda13: 1,025,122,511 B
  - The notes say: "Downloads are gzip-compressed to fit under GitHub's release-asset size limit"; "Sirius isn't published through DuckDB's official signed extension repository yet … `duckdb -unsigned`"; "Sirius extensions must match the exact DuckDB version they were built against". The requirements line matches the README (sm_75+, CUDA 12 driver ≥525.60.13 / CUDA 13 driver ≥580.65.06).

  — [sirius releases](https://github.com/sirius-db/sirius/releases) (accessed 2026-10-29)
- Gzip compression was introduced in #1973 "fix: gzip-compress release assets to fit GitHub's 2 GiB asset limit". — [sirius#1973](https://github.com/sirius-db/sirius/pull/1973) (accessed 2026-10-29)
- **Bundling:** the static distribution build statically bundles cuDF, cuVS, cuCascade (submodule) and other CUDA libraries into the extension; only NVIDIA driver and standard Linux libraries stay external. The shared build is for development via Conda/Pixi. — [sirius-duckdb/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md) (accessed 2026-10-29)
- **#1728** "Sirius as a standalone library". OPEN tracking issue by mbrobbel (company "@NVIDIA"), 2026-09-08: "support building Sirius as a standalone library, enabling anyone to use it directly as a composable analytics engine." — [sirius#1728](https://github.com/sirius-db/sirius/issues/1728); [github.com/mbrobbel](https://github.com/mbrobbel) (accessed 2026-10-29)
- **#1734** "Package Sirius libraries and the DuckDB extensions separately". OPEN by mbrobbel, 2026-09-09: "We can use rattler to build conda packages initially (for both shared and static `libsirius`)." — [sirius#1734](https://github.com/sirius-db/sirius/issues/1734) (accessed 2026-10-29)

### Inferences
- Uncompressed `.duckdb_extension` size is not published. Compression was needed to get under the 2 GiB asset limit, so the uncompressed binaries are presumably larger than 2 GiB (at least for cuda12, where the .gz is about 1.4–1.5 GB). The CUDA 12 builds are about 40–60% larger than the CUDA 13 builds, possibly because of fatbin or compression differences.
- Through the planned core loader, `INSTALL sirius; LOAD sirius;` would auto-download a backend of this size class from the community repository. That is roughly 1 GB+ on first load.
- Until #2175 merges, a desk user following the release notes with the official CLI on `latest` gets CPU execution with no warning on Parquet.

### Gaps
- No published uncompressed `.so` size, and no community-repository artifact size.
- No statement on whether the community-repo backend will be built with the fix for #2174.

## Q5. Sirius on Windows / WSL

### Takeaway
Native Windows is unsupported: Linux-only in docs, cmake gate and excluded platforms. WSL2 works in at least one user report on an RTX 4060, but needed a manual host-memory config workaround and hit a (since fixed) teardown panic.

### Cited Findings
- Native Windows is excluded: `sirius.cmake` returns on non-Linux, and community `excluded_platforms` includes `windows_amd64;windows_arm64;windows_amd64_mingw;windows_amd64_rtools`. — [PR #28022](https://github.com/duckdb/duckdb/pull/28022/files); [community-extensions#3447](https://github.com/duckdb/community-extensions/pull/3447) (accessed 2026-10-29)
- **#1873** "Quent/NVTX injection panics during teardown with Rust TLS AccessError after successful GPU query". OPEN, 2026-09-23, by bluelightspirit.
  - Environment: Windows 10 Home host, WSL 2.7.14.0, kernel 6.18.33.2-microsoft-standard-WSL2, i5-12600K, **NVIDIA GeForce RTX 4060**, nvidia-smi 610.53.
  - "On this WSL2 system, Sirius requires an absolute host-memory capacity because GPU topology discovery reports the GPU NUMA node as `-1`". Workaround: `memory.host.capacity_bytes: 4GB` in a YAML config.
  - Collaborator 9prady9 (2026-10-13): "Retested the original reproduction on Sirius 7e7e29ee … The shutdown panic no longer reproduces."

  — [sirius#1873](https://github.com/sirius-db/sirius/issues/1873); [commit 7e7e29ee](https://github.com/sirius-db/sirius/commit/7e7e29eefeaeb4760c268ce6f1fbe8c519f585f3) (accessed 2026-10-29)

### Inferences
- WSL2 is a de facto, user-tested path, not documented support. The `io_uring` and `O_DIRECT` requirements may behave differently under WSL2's 9p/drvfs mounts; that is not verified.

### Gaps
- No official maintainer statement on WSL2 support.

## Q6. GPU support list for consumer GPUs (RTX 20/30/40/50) and VRAM minimums

### Takeaway
No GPU model list exists. Support is defined only as "Turing or newer, compute capability 7.5+", which covers RTX 20 through RTX 50 in principle. Release builds include sm_75/86/89/120 SASS. No minimum VRAM is documented. Consumer GPUs appear only incidentally in issues.

### Cited Findings
- "NVIDIA Turing or newer, with compute capability 7.5+" is the only GPU requirement. — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-29)
- Release archs: CUDA 13 `75;80;86;89;90a;100f;120a;120(PTX)`, CUDA 12 `75;80;86;89;90a;100;120`. The pixi comment reads "Turing through Blackwell". — [distribution.yml](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.github/workflows/distribution.yml) (accessed 2026-10-29)
- CI GPU testing uses T4 (sm_75) and L4 (sm_89): `CUDAARCHS: "75" # 75 for T4 capability; 89 for L4 capability`, and #813 "CI: Switch GPU testing to L4 GPUs". — [test.yml](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/.github/workflows/test.yml); [sirius#813](https://github.com/sirius-db/sirius/issues/813) (accessed 2026-10-29)
- Consumer and workstation GPUs seen in issues:
  - RTX 5090 (32607 MiB) in #1667, a GPU_SCAN "validity row_offset not byte-aligned" bug, OPEN
  - RTX 4060 under WSL2 in #1873
  - RTX PRO 4500 in #2174
  - RTX 5090, A6000 and GB10 in #609

  — [sirius#1667](https://github.com/sirius-db/sirius/issues/1667); [#1873](https://github.com/sirius-db/sirius/issues/1873); [#2174](https://github.com/sirius-db/sirius/issues/2174); [#609](https://github.com/sirius-db/sirius/issues/609) (accessed 2026-10-29)
- The loader "does not detect or reject unsupported GPU architectures". — [sirius-duckdb/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/sirius-duckdb/README.md) (accessed 2026-10-29)
- Execution is "out-of-core with tiered memory management (GPU/host/disk) … and spilling", and `pin_table(tier='host')` is offered "for tables larger than GPU memory". — [docs/README.md](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md) (accessed 2026-10-29)

### Inferences
- The CUDA 12 path needs CUDA ≥12.8 for sm_120, so RTX 50 on an older CUDA 12 driver may fail. The README's driver floor of 525.60.13 is the generic CUDA 12.0 floor and is inconsistent with sm_120 needs.
- Out-of-core and spilling imply no hard VRAM floor, but there is no documented minimum and no small-VRAM benchmark.

### Gaps
- No official VRAM minimum, no tested-GPU matrix, and no consumer-GPU benchmarks from maintainers.
