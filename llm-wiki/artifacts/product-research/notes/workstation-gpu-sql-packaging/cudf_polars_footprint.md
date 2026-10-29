# RAPIDS cuDF / cudf.pandas / cudf-polars: install footprint and platform support (as of 2026-10-29)

Method note: wheel sizes come from the PyPI JSON API (`https://pypi.org/pypi/<pkg>/<ver>/json`). Dependency closures were resolved with `uv pip compile` (uv 0.13.6, `--python-version 3.12 --python-platform x86_64-manylinux_2_28`) on 2026-10-29, then summed per wheel. libcudf wheels were downloaded to a scratch dir and inspected with `unzip -l`, `readelf` and `cuobjdump --list-elf/--list-ptx`. Nothing from them was executed. "MB" means 10⁶ bytes. All accessed 2026-10-29.

## 1. PyPI wheel sizes and total download for `pip install polars[gpu]`

### Takeaway
On x86_64, the cudf-polars GPU stack downloads about 2.69 GB for CUDA 12 and 1.73 GB for CUDA 13. About 85–90% of that is libcudf plus CUDA math libraries (cuBLAS, cuSPARSE, cuSOLVER, cuFFT). Those math libraries come in through CuPy's `[ctk]` extra, not through cuDF itself. Today a bare `pip install polars[gpu]` fails to resolve to 26.10. Polars on PyPI is now 2.1.0, but cudf-polars 26.10 pins `polars<1.45`, so the resolver backtracks to an ancient stub cudf-polars-cu12 24.8 and the build fails.

### Cited Findings
**Latest versions (all 26.10.x, Linux only, manylinux_2_24/2_28, x86_64 + aarch64, no Windows/macOS wheels):**
- cudf-cu12 26.10.2 (uploaded 2026-10-22), `requires_python >=3.11`, abi3 wheel: 2.8 MB (x86_64 and aarch64). Deps: libcudf-cu12, pylibcudf-cu12, rmm-cu12, cupy-cuda12x[ctk]>=14.0.1, numba-cuda[cu12], numba-cuda-mlir, pandas>=3.0.0,<3.1, pyarrow>=19, numpy>=2,<3, cuda-bindings>=12.9.2,<13 — [PyPI cudf-cu12](https://pypi.org/pypi/cudf-cu12/json) (accessed 2026-10-29)
- cudf-cu13 26.10.2: 2.2 MB — [PyPI cudf-cu13](https://pypi.org/pypi/cudf-cu13/json) (accessed 2026-10-29)
- **libcudf-cu12 26.10.2: 666.8 MB on x86_64, 662.3 MB on aarch64.** Deps: cuda-toolkit[nvrtc]==12.*, libkvikio, librmm, nvidia-libnvcomp-cu12==5.3.0.16, nvidia-nvjitlink-cu12>=12.9, rapids-logger — [PyPI libcudf-cu12](https://pypi.org/pypi/libcudf-cu12/json) (accessed 2026-10-29)
- **libcudf-cu13 26.10.2: 301.4 MB on x86_64, 297.4 MB on aarch64** — [PyPI libcudf-cu13](https://pypi.org/pypi/libcudf-cu13/json) (accessed 2026-10-29)
- pylibcudf-cu12/cu13 26.10.2: 8.2 MB (x86_64), 7.5 MB (aarch64) — [PyPI pylibcudf-cu12](https://pypi.org/pypi/pylibcudf-cu12/json); [PyPI pylibcudf-cu13](https://pypi.org/pypi/pylibcudf-cu13/json) (accessed 2026-10-29)
- cudf-polars-cu12/cu13 26.10.2: 0.7 MB pure-Python wheel (`py3-none-any`). Deps: cudf-streaming, kvikio, pylibcudf, rapidsmpf, nvidia-ml-py, **polars>=1.35,<1.45**, cuda-bindings. It does **not** depend on the `cudf` Python package or pandas — [PyPI cudf-polars-cu12](https://pypi.org/pypi/cudf-polars-cu12/json); [PyPI cudf-polars-cu13](https://pypi.org/pypi/cudf-polars-cu13/json) (accessed 2026-10-29)
- rmm-cu12/cu13 26.10.0: 0.9–1.0 MB. librmm-cu12/cu13 26.10.0: 4.8 MB — [PyPI rmm-cu12](https://pypi.org/pypi/rmm-cu12/json); [PyPI librmm-cu13](https://pypi.org/pypi/librmm-cu13/json) (accessed 2026-10-29)
- Other RAPIDS pieces pulled in by cudf-polars: cudf-streaming 1.0 MB, libcudf-streaming 0.5 MB, rapidsmpf 2.8 MB, librapidsmpf 2.1 MB, kvikio 0.7 MB, libkvikio 3.2 MB, ucxx 0.7 MB, libucxx 4.7 MB, libucx-cu12/cu13 1.21.0 42 MB — [PyPI rapidsmpf-cu12](https://pypi.org/pypi/rapidsmpf-cu12/json); [PyPI libkvikio-cu12](https://pypi.org/pypi/libkvikio-cu12/json) (accessed 2026-10-29)
- nvidia-libnvcomp: the version pinned by libcudf 26.10 is 5.3.0.16, at 41.5 MB (cu12, x86_64) and 28.5 MB (cu13, x86_64). The latest on PyPI is 6.0.0.12 (2026-10-13), at 43.5 MB (cu12) and 29.8 MB (cu13) on x86_64 — [PyPI nvidia-libnvcomp-cu12](https://pypi.org/pypi/nvidia-libnvcomp-cu12/json) (accessed 2026-10-29)
- cupy-cuda12x 14.2.0: 136 MB (x86_64 cp312). cupy-cuda13x 14.2.0: 69 MB — [PyPI cupy-cuda12x](https://pypi.org/pypi/cupy-cuda12x/json); [PyPI cupy-cuda13x](https://pypi.org/pypi/cupy-cuda13x/json) (accessed 2026-10-29)
- The `polars` package (2.1.0, 2026-10-25) declares `gpu` extra = `cudf-polars-cu12` only (CUDA 12). Polars docs give `pip install polars cudf-polars-cu13` for CUDA 13 — [PyPI polars](https://pypi.org/pypi/polars/json); [Polars GPU support docs](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- The cudf issue "Bump Polars minimum version to 2.0" (#24513) was closed 2026-10-08, so Polars 2 support is on the 26.12 development line, not in the 26.10 wheels — [NVIDIA/cudf#24513](https://github.com/NVIDIA/cudf/issues/24513) (accessed 2026-10-29)

**Resolved closures (x86_64, CPython 3.12, sum of wheel download sizes):**

| Install spec | Packages | Total download |
|---|---|---|
| `polars[gpu]<1.45` (resolves to cudf-polars-cu12 26.10.2 and polars 1.44.2) | 37 | **2,689.7 MB (2.57 GiB)** |
| `cudf-polars-cu13` (polars 1.44.2) | 37 | **1,734.3 MB (1.65 GiB)** |
| `cudf-cu12` (cudf.pandas) | 42 | **2,815.8 MB (2.69 GiB)** |
| `cudf-cu13` | 44 | **1,940.6 MB (1.85 GiB)** |

Source: dependency resolution as above, with sizes from the [PyPI JSON API](https://pypi.org/pypi/libcudf-cu12/26.10.2/json) (accessed 2026-10-29).

- **Largest items for `polars[gpu]` (CUDA 12):**

| Package | Size |
|---|---|
| libcudf-cu12 | 666.8 MB |
| nvidia-cublas-cu12 12.9.2.10 | 581.2 MB |
| nvidia-cusparse-cu12 | 366.5 MB |
| nvidia-cusolver-cu12 | 338.1 MB |
| nvidia-cufft-cu12 | 200.9 MB |
| cupy-cuda12x | 136.1 MB |
| nvidia-cuda-nvrtc-cu12 | 89.6 MB |
| nvidia-curand-cu12 | 68.3 MB |
| polars-runtime-32 1.44.2 | 48.3 MB |
| libucx-cu12 | 42.2 MB |
| nvidia-libnvcomp-cu12 | 41.5 MB |
| nvidia-nvjitlink-cu12 | 39.7 MB |
| numpy | 17.0 MB |
| nvidia-cuda-runtime-cu12 | 3.5 MB |
| nvidia-cufile-cu12 | 1.2 MB |

  All remaining packages are under 10 MB each.
- **Largest items for `cudf-polars-cu13`:**

| Package | Size |
|---|---|
| nvidia-cublas 13.8.0.4 | 439.3 MB |
| libcudf-cu13 | 301.4 MB |
| nvidia-cusolver | 246.4 MB |
| nvidia-cusparse | 170.5 MB |
| nvidia-cufft | 161.8 MB |
| cupy-cuda13x | 69.4 MB |
| nvidia-curand | 61.5 MB |
| nvidia-cuda-nvrtc | 53.3 MB |
| polars-runtime-32 | 48.3 MB |
| nvidia-nvjitlink | 42.5 MB |
| libucx-cu13 | 42.0 MB |
| nvidia-libnvcomp-cu13 | 28.5 MB |
| nvidia-cuda-runtime | 2.5 MB |
| nvidia-cufile | 1.5 MB |

- The cudf (pandas) closure adds, on top of what cudf-polars pulls: numba-cuda-mlir 71.2 MB, llvmlite 56.3 MB, pyarrow 26.0.0 38.7 MB, nvidia-cuda-nvcc 40.5 MB (cu12) or 47.7 MB (cu13), nvidia-nvvm 72.6 MB (cu13), pandas 3.0.6 10.4 MB.
- CPU-only Polars for comparison: polars 2.1.0 (0.9 MB) + polars-runtime-32 2.1.0 (53.9 MB, manylinux x86_64) = about 55 MB — [PyPI polars-runtime-32](https://pypi.org/pypi/polars-runtime-32/json) (accessed 2026-10-29)

### Inferences
- The GPU engine is about 49× (CUDA 12) or 32× (CUDA 13) the download of CPU Polars (2.69 GB or 1.73 GB vs about 55 MB).
- On cu12, about 1.49 GB of the 2.69 GB is cuBLAS/cuSPARSE/cuSOLVER/cuFFT/cuRAND. These come in transitively via `cupy-cuda12x[ctk]`, which kvikio requires. The `[ctk]` extra appears in the kvikio-cu12 deps; that it pulls these specific libraries is inferred from the resolver output. A libcudf-only deployment that skips CuPy would be roughly libcudf + nvcomp + nvrtc + nvjitlink + cudart + rmm/kvikio, about 850 MB (cu12) or 430 MB (cu13) of wheels. This is an estimate summed from the per-wheel sizes above.
- CUDA 13 wheels are about 35% smaller than CUDA 12. Likely reasons: libcudf-cu13 is 55% smaller because it drops sm_70 and its fatbin appears compressed (see section 5), and the CUDA 13 math libraries are smaller.

### Gaps
- Installed (unpacked) size of the whole closure was not measured; only libcudf was unpacked (section 5).
- I did not resolve against `--extra-index-url https://pypi.nvidia.com`. The resolver failure involves an old `nvidia-stub` sdist (cudf-cu12 24.8.3) on PyPI that tries to fetch from pypi.nvidia.com.

## 2. Docker image sizes (Docker Hub and NGC)

### Takeaway
RAPIDS 26.10 Docker images are about 4.1–4.5 GB (CUDA 13) and about 5.9 GB (CUDA 12) as compressed pull size. The notebooks variant adds about 0.15–0.2 GB.

### Cited Findings
- Sizes are Docker Hub `full_size`, which is the compressed layer total, for `rapidsai/base:26.10-*`, updated 2026-10-28 — [Docker Hub API rapidsai/base tags](https://hub.docker.com/v2/repositories/rapidsai/base/tags) (accessed 2026-10-29):

| Tag | amd64 | arm64 |
|---|---|---|
| 26.10-cuda13-py3.11 | 4.19 GB | 4.53 GB |
| 26.10-cuda13-py3.12 | 4.16 GB | 4.50 GB |
| 26.10-cuda13-py3.13 | 4.12 GB | 4.47 GB |
| 26.10-cuda13-py3.14 | 4.15 GB | 4.50 GB |
| 26.10-cuda12-py3.11 | 5.96 GB | 5.90 GB |
| 26.10-cuda12-py3.12 | 5.93 GB | 5.87 GB |
| 26.10-cuda12-py3.13 | 5.90 GB | 5.84 GB |
| 26.10-cuda12-py3.14 | 5.93 GB | 5.87 GB |

- `rapidsai/base:26.08` (2026-08-06): about 6.2–6.27 GB for cuda12 and about 4.07–4.11 GB amd64 / 4.43–4.46 GB arm64 for cuda13 — [Docker Hub API rapidsai/base tags](https://hub.docker.com/v2/repositories/rapidsai/base/tags) (accessed 2026-10-29)
- `rapidsai/notebooks:26.10`: cuda12-py3.12 is 6.10 GB (amd64) and 6.03 GB (arm64); cuda13-py3.12 is 4.32 GB (amd64) and 4.66 GB (arm64) — [Docker Hub API rapidsai/notebooks tags](https://hub.docker.com/v2/repositories/rapidsai/notebooks/tags) (accessed 2026-10-29)
- RSN 53: from 25.10, Docker images are consolidated to a single tag per CUDA major version — [RAPIDS notices](https://docs.nvidia.com/datascience/notices/) (accessed 2026-10-29)
- The install docs say "All images are multiarch (x86_64 and ARM)" — [RAPIDS install](https://docs.nvidia.com/datascience/install/) (accessed 2026-10-29)
- The NGC `nvidia/rapidsai/base` catalog lists latest tag `26.10-cuda13-py3.14`, updated 2026-10-09, with compressed size 4.18 GB and multi-arch amd64/arm64. It has 172 tags. 26.02 tags cover py3.10–3.13; 26.04 and later cover py3.11–3.14 — [NGC RAPIDS base](https://catalog.ngc.nvidia.com/orgs/nvidia/teams/rapidsai/containers/base) (accessed 2026-10-29)

### Inferences
- Uncompressed on-disk size is typically about 2× compressed for CUDA images. That is a rule of thumb, not measured, so expect roughly 8–12 GB on disk.

### Gaps
- Uncompressed image sizes were not measured (no pull).

## 3. Python and CUDA version support

### Takeaway
26.10 supports Python 3.11–3.14 with CUDA 12.2–12.9 (driver ≥535) or CUDA 13.0–13.3 (driver ≥580). CUDA 11 was dropped after 25.06, and Python 3.11 is dropped in 26.12.

### Cited Findings
- RAPIDS platform support table, by release (from the [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) page, accessed 2026-10-29):

| Release | Python | CUDA |
|---|---|---|
| 25.06 | 3.10–3.13 | 11.4–11.8 (driver 470.42.01+) and 12.0–12.9 (525.60.13+) |
| 25.08 | 3.10–3.13 | 12.0–12.9 (no CUDA 11) |
| 25.10 | 3.10–3.13 | 12.0–12.9; first release with CUDA 13.0 (driver 580+) |
| 25.12 | 3.10–3.13 | CUDA 12.2–12.9, driver 535+ |
| 26.02 | 3.10–3.13 | CUDA 13.0–13.1 |
| 26.04 | 3.11–3.14 | — |
| 26.06 | 3.11–3.14 | CUDA 13.0–13.2 |
| 26.08 / 26.10 | 3.11–3.14 | CUDA 12.2–12.9 / 13.0–13.3 |
| 26.12 (nightly) | 3.12–3.14 | — |

- RSN 40: drop Python 3.9 and add 3.12 in 24.10. RSN 58 (2026-04-01): drop 3.10 and add 3.14 in 26.04. RSN 63 (2026-10-27): drop 3.11 in 26.12. RSN 48: CUDA 11 deprecated / dropped in 25.08. RSN 54: CUDA minimum raised from 12.0 to 12.2 in 25.12 — [RAPIDS notices](https://docs.nvidia.com/datascience/notices/) (accessed 2026-10-29)
- Python 3.13 is first listed for 25.06 in the table above. A secondary news site claims 25.06 added 3.13 — [blockchain.news](https://blockchainnews.azurewebsites.net/news/rapids-introduces-gpu-polars-streaming-and-unified-gnn-api-enhancements) (accessed 2026-10-29). I found no official RSN for the 3.13 addition.
- Conflict: the install page still says "CUDA 12 with Driver 525.60.13 or newer", while the platform table says 535+ from 25.12 — [RAPIDS install](https://docs.nvidia.com/datascience/install/); [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) (accessed 2026-10-29)
- RSN 59: from 26.08, cuDF moved to pandas 3.0+. The 26.10 wheel pins `pandas>=3.0.0,<3.1.0`. RSN 61: 26.08 requires CuPy 14 and NumPy 2 — [RAPIDS notices](https://docs.nvidia.com/datascience/notices/); [PyPI cudf-cu12](https://pypi.org/pypi/cudf-cu12/json) (accessed 2026-10-29)
- Wheel tags: the 26.10 compiled wheels are `cp311-abi3`, so one wheel serves 3.11–3.14+, and `requires_python >=3.11` — [PyPI pylibcudf-cu12](https://pypi.org/pypi/pylibcudf-cu12/json) (accessed 2026-10-29)
- pip wheels require the matching CUDA major: "if you have the CUDA 12 toolkit, install the -cu12 wheels" — [RAPIDS install](https://docs.nvidia.com/datascience/install/) (accessed 2026-10-29)

### Inferences
- Because of abi3 wheels, adding a new CPython version no longer needs new RAPIDS binaries. Support windows are now set by policy, not by the build.

### Gaps
- No official dated notice for when Python 3.11 was first added. It is outside the 25.06–26.12 table, and from memory it was around 23.x, unverified.

## 4. GPU generations, minimum compute capability, built SASS

### Takeaway
Pascal was dropped in 24.02. Volta (sm_70) is still supported, but only in CUDA 12 builds. CUDA 13 builds require Turing (sm_75) or newer. There is no RAPIDS notice deprecating Volta as of 2026-10-29. The minimum is effectively CC 7.0 with cu12 and CC 7.5 with cu13.

### Cited Findings
- "NVIDIA Volta™ or higher with compute capability 7.0+". "Pascal™ GPU support was removed in 24.02. Compute capability 7.0+ is required for version 24.02 and later." — [RAPIDS install](https://docs.nvidia.com/datascience/install/) (accessed 2026-10-29)
- Platform table: CUDA 12 builds cover Volta (70), Turing (75), Ampere (80, 86), Ada (89), Hopper (90) and Blackwell (100, 120). CUDA 13 builds cover "Turing (75) and newer" (no Volta). "Newer GPUs are supported through forward-compatible PTX instructions built for the latest virtual architecture." — [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) (accessed 2026-10-29)
- The notices list (30 notices up to RSN 63) has no notice about Volta or Pascal — [RAPIDS notices](https://docs.nvidia.com/datascience/notices/) (accessed 2026-10-29)
- Polars docs: "NVIDIA Volta or higher, with compute capability 7.0+" — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- rapids-cmake `RAPIDS` architecture set (main branch), from [rapids-cmake set_architectures.cmake](https://github.com/rapidsai/rapids-cmake/blob/main/rapids-cmake/cuda/set_architectures.cmake) (accessed 2026-10-29):

| nvcc version | Architectures |
|---|---|
| ≥13.0 | `75-real 80-real 86-real 90a-real 100f-real 120a-real 120` (120 = SASS + PTX) |
| 12.9 | `70-real 75-real 80-real 86-real 90a-real 100f-real 120a-real 120` |
| 12.8 | `70 75 80 86 90a 100 120a-real + 120-virtual` |

- **Measured: libcudf-cu13 26.10.2 x86_64.** `cuobjdump --list-elf` shows 463 cubins each for sm_75, sm_80, sm_86, sm_90 and sm_100, and 926 for sm_120 (120a plus 120). `--list-ptx` shows 463 PTX entries for sm_120 only. There is no sm_70, sm_89 or sm_121 SASS. Measured by me on the wheel from [PyPI libcudf-cu13 26.10.2](https://pypi.org/pypi/libcudf-cu13/26.10.2/json) (accessed 2026-10-29)

### Inferences
- Ada (sm_89) runs the sm_86 SASS (binary compatible within major 8). GB10 / RTX 50 sm_12x run the sm_120 SASS or JIT the compute_120 PTX.
- Volta dropping out of the CUDA 13 toolkit means Volta is de facto on its way out. Once RAPIDS drops CUDA 12, Volta will go too, but no date has been announced.

### Gaps
- No official Volta deprecation date found.

## 5. libcudf binary size, static build, per-arch size

### Takeaway
libcudf.so in the cu13 wheel is 369.7 MB unpacked, of which 272.8 MB (74%) is `.nv_fatbin` (GPU code for 6 architectures) and 73.7 MB is host `.text`. A static libcudf (`BUILD_SHARED_LIBS=OFF`) is a supported build path used by cudf-java / Spark. Upstream measured a full-arch Spark build at about 1.5 GB, with fatbin at 70.5%.

### Cited Findings
- **libcudf-cu13 26.10.2 x86_64 wheel**, measured by me from [PyPI libcudf-cu13](https://pypi.org/pypi/libcudf-cu13/26.10.2/json) (accessed 2026-10-29):

| Item | Size |
|---|---|
| Wheel (compressed) | 301.4 MB |
| Unpacked total, 2,998 files | 392.8 MB |
| `libcudf/lib64/libcudf.so` | 369,662,288 B |
| `.nv_fatbin` section | 0x10421550 = 272.8 MB |
| `.text` section | 0x464f802 = 73.7 MB |

  The remaining files are about 23 MB of headers (vendored CCCL/libcudacxx etc.). Zip compression barely shrinks libcudf.so (370 → about 300 MB), which suggests the fatbins are already compressed.
- `DT_NEEDED` of libcudf.so (cu13): librmm.so, libkvikio.so, libnvcomp.so.5, librapids_logger.so, libnvrtc.so.13, libnvJitLink.so.13, libz.so.1, libstdc++, libm, libgcc_s, libc, libdl, libpthread, librt. CUDA runtime is linked statically (no libcudart in NEEDED). Same source as the table above.
- [NVIDIA/cudf#23419](https://github.com/NVIDIA/cudf/issues/23419) (2026-07-23, open) (accessed 2026-10-29) gives these Spark-JNI figures:
  - "libcudf.so is approximately 1.5 GiB in the current cudf-spark-jni package".
  - A full Release build: `libcudf.so` 1,504,051,352 B, `.nv_fatbin` 1,061,134,960 B (70.55%), 518 fatbin blobs "commonly containing eight SASS images plus PTX".
  - Single-arch (`sm_75`) prototype fatbin savings of about 33–50% per deduplicated kernel family. The estimated full-matrix saving is 39.6 MiB (3.91% of fatbin).
- Static build is supported. `java/ci/build_static_libcudf.sh` builds libcudf with `BUILD_SHARED_LIBS=OFF`, and the JAR uses `CUDF_JNI_LIBCUDF_STATIC=ON` to produce a fat `libcudf.so` inside cudf-java classifier JARs. A 2026-08 regression left a `libspdlog.so.1.17` dependency in that fat library — [NVIDIA/cudf#23575](https://github.com/NVIDIA/cudf/issues/23575) (2026-08-06, open) (accessed 2026-10-29)
- Projects that statically link libcudf into one native library: cudf-java / RAPIDS Accelerator for Apache Spark (spark-rapids-jni), per #23575 and #23419 above.
- Single-arch builds: `CMAKE_CUDA_ARCHITECTURES=NATIVE` (or a single value) is supported via `rapids_cuda_set_architectures(NATIVE|RAPIDS)` — [rapids-cmake set_architectures.cmake](https://github.com/rapidsai/rapids-cmake/blob/main/rapids-cmake/cuda/set_architectures.cmake) (accessed 2026-10-29)

- **libcudf-cu12 26.10.2 x86_64 wheel**, measured by me from [PyPI libcudf-cu12](https://pypi.org/pypi/libcudf-cu12/26.10.2/json) (accessed 2026-10-29):

| Item | Size |
|---|---|
| Wheel (compressed) | 666.8 MB |
| Unpacked total, 2,998 files | 1,021.7 MB |
| `libcudf.so` | 998,503,144 B |
| `.nv_fatbin` section | 0x3579f5a0 = 897.2 MB (90% of the .so) |
| `.text` section | 0x4b72282 = 79.1 MB |

  SASS present: 463 cubins each for sm_70, sm_75, sm_80, sm_86, sm_90a, sm_100, sm_120a and sm_120, 8 SASS targets in total. `--list-ptx` shows compute_120 PTX.

### Inferences
- cu12 and cu13 differ by more than the dropped sm_70. cu12 has 8 SASS targets plus PTX in an 897 MB fatbin; cu13 has 7 plus PTX in a 273 MB fatbin. Per target that is about 100 MB in cu12 vs about 34 MB in cu13. This strongly suggests the CUDA 13 build uses fatbin compression, or a more size-oriented compression mode, and the CUDA 12 build does not. The same compressed content explains why zip barely shrinks cu13 libcudf.so (370 → 301 MB) while cu12 shrinks 999 → 667 MB.
- Single-arch estimate:
  - **CUDA 13 build, one SASS arch, no PTX:** about 34–40 MB fatbin + 73.7 MB `.text` + about 20 MB other sections ≈ **125–135 MB libcudf.so**.
  - **CUDA 12 / uncompressed:** about 100 MB fatbin + 79 MB `.text` ≈ **200 MB**.

  These are extrapolations, not a measured build.

### Gaps
- No published measurement of a single-arch libcudf build size from upstream.

## 6. Native Windows

### Takeaway
There is still no native Windows support for cuDF, cudf.pandas or cudf-polars as of 2026-10-29. Windows is supported only through WSL2. There are no Windows wheels on PyPI.

### Cited Findings
- "Windows is supported only through WSL... Supported via WSL with a compatible Linux distribution." — [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) (accessed 2026-10-29)
- WSL2 requirements: Windows 11 with a WSL2 Ubuntu install, "WSL1 not supported", CC 7.0+ ("16GB+ GPU RAM is recommended"), "Only single GPU is supported", "GPU Direct Storage is not supported". Known error: "WSL2 Jitify fatal error: libcuda.so: cannot open shared object file" — [RAPIDS install](https://docs.nvidia.com/datascience/install/) (accessed 2026-10-29)
- Polars GPU engine: Linux or WSL2 only. Native Windows is not listed — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- PyPI 26.10.2 cudf, libcudf, pylibcudf and rmm wheels exist only for manylinux x86_64 and aarch64 — [PyPI libcudf-cu12](https://pypi.org/pypi/libcudf-cu12/json) (accessed 2026-10-29)
- Long-standing tracking issue "Support for windows?" — [rapidsai/cudf#28](https://github.com/rapidsai/cudf/issues/28) (accessed 2026-10-29, per search snippet). The issue thread (per search snippet) says there are no plans but contributions would be accepted. A maintainer said in 2020 that native Windows "is not a priority for us, especially given the push towards GPU support in WSL2" — [Stack Overflow answer (mirror)](https://kiwix.lema.org/stackoverflow.com_en_all_2023-11/a/63620909) (accessed 2026-10-29)

### Gaps
- I found no 2025–2026 official statement revisiting native Windows. The NVIDIA/cudf GitHub search for "windows" returned only rolling-window issues.

## 7. aarch64 / DGX Spark (GB10, sm_121)

### Takeaway
aarch64 manylinux wheels and arm64 images ship for every current release. GB10 support reportedly arrived in public packages with 25.10, according to a forum reply. There is no sm_121-specific SASS; GB10 relies on sm_120 SASS or compute_120 PTX.

### Cited Findings
- Linux glibc ≥2.28 on x86_64 and aarch64 — [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) (accessed 2026-10-29)
- aarch64 wheels: libcudf-cu13 26.10.2 aarch64 is 297.4 MB; libcudf-cu12 aarch64 is 662.3 MB — [PyPI libcudf-cu13](https://pypi.org/pypi/libcudf-cu13/json) (accessed 2026-10-29)
- Forum thread "RAPIDS/cuDF preview build request for Grace-Blackwell GB10 (DGX) — aarch64 kernels missing". A user reports the public 24.12 and nightly builds lacked GB10 kernels. A reply says GB10 support is available in public RAPIDS 25.10 packages — [NVIDIA forums](https://forums.developer.nvidia.com/t/rapids-cudf-preview-build-request-for-grace-blackwell-gb10-dgx-aarch64-kernels-missing/352629) (accessed 2026-10-29, per search snippet, not verified against release notes)
- A general complaint thread exists: "DGX Spark (SM121) Software Support is Severely Lacking" — [NVIDIA forums](https://forums.developer.nvidia.com/t/dgx-spark-sm121-software-support-is-severely-lacking-official-roadmap-needed/357663) (accessed 2026-10-29, title only)
- GPUDirect Storage is not supported on DGX Spark — [NVIDIA forums: GDS support for DGX Spark](https://forums.developer.nvidia.com/t/gds-support-for-dgx-spark/350328) (accessed 2026-10-29, per search snippet)
- The measured cu13 libcudf has no sm_121 or sm_120f SASS, only sm_120 (plus 120a) SASS and compute_120 PTX (section 4).

### Inferences
- sm_121 can execute plain sm_120 SASS (same major); the "a" variant is arch-locked. So GB10 should run without JIT. A cold JIT would only be needed if a kernel exists solely as PTX.

### Gaps
- No official RAPIDS release note confirming the version that added GB10 support. No cuDF-specific GB10 bug was found in the NVIDIA/cudf issue search.

## 8. cudf-polars status: beta, executor, fallback, SQL

### Takeaway
cudf-polars is still labelled "Open Beta". The default executor in 26.10 is `streaming`, backed by RapidsMPF since 26.06. Unsupported queries silently fall back to CPU Polars unless `raise_on_fail=True`. Polars SQL (`pl.SQLContext`) produces LazyFrames, which run on the GPU engine with node-level gaps; SQL coverage is being actively fixed for Polars 2.x on the 26.12 line.

### Cited Findings
- "available in Open Beta and is undergoing rapid development" — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/); [cuDF docs: Polars GPU engine](https://docs.nvidia.com/cudf/latest/cudf_polars/) (accessed 2026-10-29)
- Default executor in the 26.10 source, `ConfigOptions.from_polars_engine`: `user_executor = os.environ.get("CUDF_POLARS__EXECUTOR", "streaming")`. Valid executors are `in-memory` and `streaming`. `raise_on_fail` defaults to `False`. The streaming executor has `fallback_mode` WARN / SILENT / RAISE for single-partition fallback — [cudf release/26.10 config.py](https://github.com/NVIDIA/cudf/blob/release/26.10/python/cudf_polars/cudf_polars/utils/config.py) (accessed 2026-10-29)
- Streaming history:
  - Experimental Dask-orchestrated streaming executor in 25.06.
  - Streaming became the default in 25.08.
  - Moved to a RapidsMPF-based backend in 26.06, "now the recommended path for any nontrivial GPU workload".

  Sources: [pola.rs: GPU streaming backend (Brian Tepera, NVIDIA, 2026-08-28)](https://pola.rs/posts/gpu-streaming-backend); [NVIDIA blog: RAPIDS adds GPU Polars streaming](https://developer.nvidia.com/blog/rapids-adds-gpu-polars-streaming-a-unified-gnn-api-and-zero-code-ml-speedups/) (accessed 2026-10-29)
- Fallback: by default the query will "transparently fall back to the standard Polars engine". A `PerformanceWarning` is issued only with `pl.Config().set_verbose(True)`. `pl.GPUEngine(raise_on_fail=True)` raises instead — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- Unsupported on GPU (Polars docs, from [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/), accessed 2026-10-29):
  - Eager DataFrame API
  - Date, Categorical, Enum, Time, Array, Binary and Object dtypes
  - Some Datetime-with-timezone and List expressions
  - Time-series resampling
  - Folds
  - UDFs
  - Excel and database I/O
- Test coverage: 99.2% of Polars unit tests pass with CPU fallback, 88.8% without — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- Multi-GPU engines (`RayEngine`, `DaskEngine`, `SPMDEngine`) became available in cudf-polars 26.06 — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29)
- **SQL:** the Polars docs list the "SQL API" under Supported — [Polars GPU support](https://docs.pola.rs/user-guide/gpu-support/) (accessed 2026-10-29). The NVIDIA/cudf issues below show the gaps (all accessed 2026-10-29):

| Issue | Opened | State | What it says |
|---|---|---|---|
| [#24761](https://github.com/NVIDIA/cudf/issues/24761) | 2026-10-21 | open | `pl.SQLContext(...).execute("... NOT IN (SELECT ...)")` with `GPUEngine(raise_on_fail=True)` raises `NotImplementedError: nested loop join`. Without raise_on_fail it falls back. |
| [#24756](https://github.com/NVIDIA/cudf/issues/24756) | 2026-10-21 | open | Story: "Several SQL queries in Polars 2.0 benchmark repo ... raise errors" for TPC-H SQL. |
| [#24555](https://github.com/NVIDIA/cudf/issues/24555) | 2026-10-12 | open | Add SQLContext benchmarking. |
| [#24693](https://github.com/NVIDIA/cudf/issues/24693) | 2026-10-19 | open | **Wrong results, no fallback**: LEFT/RIGHT `join_where` and SQL LEFT/RIGHT JOIN with an inequality condition go through the equi-join path. |
| [#24695](https://github.com/NVIDIA/cudf/issues/24695) | 2026-10-19 | open | Polars 2.0 fused-predicate joins cannot be translated, so they fall back to CPU. |
| [#24843](https://github.com/NVIDIA/cudf/issues/24843) | 2026-10-26 | open | With Polars 2.1, multi-key `sort_by().first()` in group_by panics (`PanicException`) instead of falling back, "with or without raise_on_fail". |

### Inferences
- Polars SQL runs on the GPU engine because SQLContext produces a LazyFrame plan that `collect(engine="gpu")` consumes. Coverage is partial: certain plan nodes fall back silently. Silent fallback by default hides whether a given SQL query actually ran on the GPU. Production users need `raise_on_fail=True` or verbose mode to know.
- "Fallback" is not fully reliable. #24843 shows panics, and #24693 shows silent wrong results.

### Gaps
- No published percentage of TPC-H/TPC-DS SQL queries that run fully on GPU.

## 9. cudf.pandas status and limits

### Takeaway
cudf.pandas is GA-style "zero code change" acceleration with automatic CPU fallback. It is Linux/WSL2 only. Since 26.08 it is built against pandas 3.

### Cited Findings
- Unsupported operations fall back to CPU pandas. Fallback implies GPU↔CPU copies. The `CUDF_PANDAS_FALLBACK_MODE` env var can disable GPU for testing — [cudf.pandas FAQ](https://docs.nvidia.com/cudf/latest/cudf_pandas/faq/) (accessed 2026-10-29)
- Limitations, from the same [cudf.pandas FAQ](https://docs.nvidia.com/cudf/latest/cudf_pandas/faq/) (accessed 2026-10-29):
  - Mutability not always guaranteed.
  - Join row order not always preserved.
  - Not compatible with a direct `import cudf` in the same program.
  - Objects pickled with plain pandas can't be unpickled while enabled.
  - Globals can't be modified during fallback.
  - Proxy arrays trigger eager device→host copies.
- Conflict: the FAQ still says "only compatible with pandas 2". RSN 59 says cuDF moved to pandas 3.0+ in 26.08, and the cudf-cu12 26.10.2 wheel pins `pandas>=3.0.0,<3.1.0` — [cudf.pandas FAQ](https://docs.nvidia.com/cudf/latest/cudf_pandas/faq/); [RAPIDS notices](https://docs.nvidia.com/datascience/notices/); [PyPI cudf-cu12](https://pypi.org/pypi/cudf-cu12/json) (accessed 2026-10-29)
- Windows: same as cuDF, WSL2 only — [RAPIDS platform support](https://docs.nvidia.com/datascience/platform-support/) (accessed 2026-10-29)

### Gaps
- The FAQ appears stale, so the docs are unreliable on the pandas version.

## 10. Polars-on-GPU benchmarks (consumer GPUs vs CPU Polars / DuckDB)

### Takeaway
The 2025–2026 vendor benchmarks are datacenter-only (B200/H100) and compare against CPU Polars, not DuckDB. I found no NVIDIA or Polars benchmark on consumer RTX GPUs.

### Cited Findings
- From [pola.rs, 2026-08-28](https://pola.rs/posts/gpu-streaming-backend) (accessed 2026-10-29). The CPU baseline is an unnamed dual-socket Xeon, and there is no DuckDB comparison:

| Scale | Hardware | Benchmark | CPU | GPU | Speedup |
|---|---|---|---|---|---|
| SF1K | 1× B200 | PDS-H | 271.9 s | 84.7 s | 3.2× |
| SF1K | 1× B200 | PDS-DS | 734.0 s | 333.5 s | 2.2× |
| SF3K | 8× B200 | PDS-H | 1,118.3 s | 48.2 s | 23.2× |
| SF3K | 8× B200 | PDS-DS | 1,863.5 s | 169.8 s | 11.0× |

- An earlier NVIDIA claim of "up to 13×" speedup on PDS-H queries with heavy groupby/join is a secondary summary — [Towards AI](https://towardsai.net/?p=37099) (accessed 2026-10-29, per search snippet)
- A third-party H100 example: 22 GB, 12 s on CPU Polars vs 0.34 s with GPU — [DEV Community](https://dev.to/federicotrotta/accelerating-polars-with-rapids-cudf-3833) (accessed 2026-10-29, per search snippet)
- An NVIDIA blog on data exceeding VRAM (UVM / managed memory) — [NVIDIA blog](https://developer.nvidia.com/blog/how-to-work-with-data-exceeding-vram-in-the-polars-gpu-engine) (accessed 2026-10-29, not fetched)
- An open issue adds DuckDB validation for SQL benchmarks: `--validate-against duckdb` fails on dtype/column-name differences — [NVIDIA/cudf#24882](https://github.com/NVIDIA/cudf/issues/24882) (2026-10-28) (accessed 2026-10-29)

### Inferences
- Single-GPU speedups at SF1K are modest, 2.2–3.2× vs a dual-socket Xeon. On consumer GPUs with 8–24 GB VRAM and PCIe host links, gains at workstation scale are unproven by vendor data.

### Gaps
- No official consumer-GPU (RTX 30/40/50) or DuckDB-vs-GPU-Polars benchmark found for 2025–2026.
