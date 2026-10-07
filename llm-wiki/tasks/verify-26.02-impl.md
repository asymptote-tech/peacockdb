# verify-26.02 implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** every tier that links cuDF passes on shad-gpu under cuDF 26.02 (or each failure is fixed
with its own red case, or recorded as 26.02 behaviour), and every tpch query-mode cell the chain
turned on is benchmarked at sf40 on 26.02 and on 25.02 — the evidence #244 waits on.

**Architecture:** shad-gpu has no cargo and no sudo, and today's flow builds on the workstation,
ships `cpp/install/` and glibc-patches it there (`scripts/build-test-shadgpu.sh`). The 26.02 run is
that same flow in a second mode: the C++ built in the existing local `cpp/build26` against the local
26.02 env (as `build-test.sh` already does for verda), cargo in its own `target-cudf-rapids`, shipped
to a *separate* remote repo dir `/home/info/peacockdb-2602` so the 25.02 tree on the host is never
touched, and run with shad-gpu's `rapids-2602` env on the library path.

**Tech stack:** bash (`build-test-shadgpu.sh`, `lib/shadgpu-env.sh`, `build.sh`), cmake/ninja,
cargo, patchelf via `setup-glibc.sh`, one small Rust change in `test_support` for the benchmark
tree.

**Spec:** [`verify-26.02.md`](verify-26.02.md) — frozen (committed 9e563348).

## Global constraints

- Never `cpp/build`, `cpp/install` (the 25.02 trees) or `target-cudf-rapids-cuda-12.2`: the 26.02
  mode uses `cpp/build26`, `cpp/build26/install` and `target-cudf-rapids` only. Never share a cargo
  target dir across worktrees.
- On shad-gpu: never write under `/home/info/peacockdb` (the 25.02 repo dir) or `~/build-2602`
  (a separate C++-only tree of `~/peacockdb-src`); everything goes under `/home/info/peacockdb-2602`.
- No 26.02-only fast path (#242); 25.02 stays supported (#244 decides later).
- A fix lands with its own red case first; after any fix, 25.02 must still pass (Task 7).
- Every ssh/rsync to shad-gpu needs the sandbox off; device cycles run in the foreground (a
  backgrounded build/push/run chain gets killed mid-build).
- Fall back to a temporary 26.02 host only if Task 2's ladder ends without a working runtime.

## Facts this plan rests on (read 2026-10-07)

| | workstation 26.02 env `~/data/miniforge3/envs/rapids` | shad-gpu `~/miniforge3/envs/rapids-2602` |
|---|---|---|
| libcudf | 26.02.01 (build 5b9658c4) | 26.02.01 (build 5b9658c4) |
| librmm / libkvikio / nvcomp | 26.02.00 / 26.02.00 / 5.1.0.21 | the same |
| CUDA (cuda-version, cudart) | 12.9 | 12.2 |
| libarrow | 21.0.0 | 25.0.0 |

- `libpeacock_gpu.so` needs `libarrow.so.2100`; `cpp/CMakeLists.txt:351` bundles Arrow and its
  dependencies into the install's `lib/`, which is first on the host's library path — so the
  host's Arrow 25 is never asked for.
- `libcudart.so.12` is *not* bundled: on the host it resolves from `rapids-2602/lib` (12.2) while
  our library was compiled by nvcc 12.9. Task 2 decides whether that loads.
- shad-gpu: glibc 2.31, H200, driver 535.247.01, `/usr/local/cuda-12.5/compat`, glibc prefixes
  `~/glibc-2.35` and `~/glibc-2.39`; the workstation builds against glibc 2.35.
- The benchmark harness times only `peacockdb-core/tests/common/corpus_benchmark_cases.inc`'s
  cases, tpch at sf40 (`/home/info/peacock-datasets/testdata/tpch.sf40`); and
  `results_file` writes one path per (dataset, sf, mode) with no cuDF version in it
  (`test_support/corpus_benchmark.rs:49-56`), so a 26.02 run would overwrite the 25.02 one.

## Review focus

1. **A 26.02 binary loading a 25.02 library.** The two modes share `scripts/` and remote helpers;
   a mixed library path (`rapids-cuda-12.2/lib` reached from the 26.02 dir) loads and then fails in
   the middle of a test. Task 1's `ldd` check on the host lists every resolved path and fails on
   any `rapids-cuda-12.2`.
2. **The 26.02 push deleting the 25.02 tree.** `--push-binaries` mirrors with `--delete`; pointed at
   the wrong remote dir it erases `/home/info/peacockdb/cpp/install`. Task 1 refuses a 26.02 mode
   whose `REMOTE_REPO` is the 25.02 one.
3. **A gate that ran nothing and passed.** A filter or a missing staging dir gives "0 passed" — the
   existing zero-test guards must be live in the 26.02 mode (Task 3 asserts the counts per tier).
4. **The 25.02 benchmark file overwritten by 26.02's.** Task 5 puts each run under its own
   version directory and writes `cudf=` into every run block; a test asserts the path.
5. **The CI wiring guard.** `test_ci_coverage/runners.rs` parses `build-test-shadgpu.sh`'s
   `RUST_TESTS=(` and `RUST_LIB_*`; a mode switch that moves or duplicates them fails it. Task 1
   runs it.

## File structure

| file | responsibility |
|---|---|
| `scripts/lib/shadgpu-env.sh` | `PEACOCK_CUDF=25.02|26.02` selects root, gcc, build/install dirs, remote repo, host env |
| `scripts/build.sh` | `--build-dir`, `--install-dir` (defaults `cpp/build`, `cpp/install`) |
| `scripts/build-test-shadgpu.sh` | uses the selected dirs everywhere it names `cpp/install` |
| `peacockdb-core/src/test_support/corpus_benchmark.rs`, `test_support/mod.rs` | `cudf=` in the run block; per-version results root |
| `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp` | a new symbol, `peacock_cudf_version()`, answers the cuDF version it was built against; `peacock_gpu_version()` is unchanged (`cpp/tests/cpu/test_executor.cpp:9` pins it at `"0.1.0"`) |
| `peacockdb-ffi/src/lib.rs` | `raw::peacock_cudf_version`, `cudf_version()` |
| `peacockdb-core/tests/common/corpus_benchmark_cases.inc` | the chain's flipped tpch cells at sf40 |
| `testdata/benchmark-results/cudf-25.02/`, `cudf-26.02/` | the two runs |
| `llm-wiki/build-test.md`, `tickets/system-hardening.md`, `tasks/verify-26.02-detail.md` | how to run; #244's evidence; the record |

---

### Task 1: A 26.02 mode for the shad-gpu flow

**Files:**
- Modify: `scripts/lib/shadgpu-env.sh` (the `CUDF_ROOT=` line must stay greppable as `^CUDF_ROOT=` for `docker-build.sh`)
- Modify: `scripts/build.sh:13-34` (flags), `scripts/build-test-shadgpu.sh` (every `cpp/install`)
- Test: `peacockdb-core/tests/test_ci_coverage.rs` (existing guard)

**Interfaces:**
- Produces: env `PEACOCK_CUDF` (`25.02` default, `26.02`); shell vars `CPP_BUILD_DIR`, `CPP_INSTALL_DIR`,
  `REMOTE_REPO`, `REMOTE_CUDF_LIB`; `build.sh --build-dir D --install-dir I`.

- [ ] **Step 1: Select the toolchain by version.** In `shadgpu-env.sh`, keep the 25.02 assignment
  first and literal, then override for 26.02:

```bash
CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2
PEACOCK_CUDF="${PEACOCK_CUDF:-25.02}"
case "$PEACOCK_CUDF" in
  25.02)
    GCC_VERSION=12
    CPP_BUILD_DIR=cpp/build;   CPP_INSTALL_DIR=cpp/install
    REMOTE_REPO=/home/info/peacockdb
    REMOTE_CUDF_LIB='$HOME/miniforge3/envs/rapids-cuda-12.2/lib' ;;
  26.02)
    CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids
    GCC_VERSION=14            # nvcc 12.9 accepts gcc-14 (build-test.sh's verda build)
    CPP_BUILD_DIR=cpp/build26; CPP_INSTALL_DIR=cpp/build26/install
    REMOTE_REPO=/home/info/peacockdb-2602
    REMOTE_CUDF_LIB='$HOME/miniforge3/envs/rapids-2602/lib' ;;
  *) echo "PEACOCK_CUDF=$PEACOCK_CUDF: 25.02 or 26.02" >&2; return 1 ;;
esac
export CUDF_ROOT
```

  and build `PATCHED_LD` from `REMOTE_CUDF_LIB` instead of the literal `rapids-cuda-12.2` path.
  `CARGO_TARGET_DIR` already keys off `basename "$CUDF_ROOT"`, so 26.02 lands in
  `target-cudf-rapids` (the one `build-test.sh` uses for verda) with no further change.

- [ ] **Step 2: Refuse the dangerous mix.** Directly after the `case`:

```bash
if [ "$PEACOCK_CUDF" = 26.02 ] && [ "$REMOTE_REPO" = /home/info/peacockdb ]; then
  echo "26.02 mode pointed at the 25.02 remote repo: a --delete push would erase it" >&2; return 1
fi
```

- [ ] **Step 3: `build.sh` takes its dirs.** Add `--build-dir)` / `--install-dir)` to the flag loop
  and replace `BUILD_DIR="${TARGET}/build"` / `INSTALL_DIR="${TARGET}/install"` with
  `BUILD_DIR="${BUILD_DIR:-${TARGET}/build}"` / `INSTALL_DIR="${INSTALL_DIR:-${TARGET}/install}"`.
  In `build-test-shadgpu.sh`, the three `build.sh` calls pass
  `--build-dir "$CPP_BUILD_DIR" --install-dir "$CPP_INSTALL_DIR"`; `RUST_TESTS_STAGING` and
  `BENCH_STAGING` become `$CPP_INSTALL_DIR/rust-tests`, `$CPP_INSTALL_DIR/rust-benchmarks`; the push
  mirrors `"$CPP_INSTALL_DIR/"` to `"$REMOTE:$REMOTE_REPO/cpp/install/"` — the remote layout stays
  `cpp/install`, so the remote gate and `setup-glibc.sh` need no change.

- [ ] **Step 4: Syntax and the guard.**

Run: `bash -n scripts/build-test-shadgpu.sh scripts/build.sh scripts/lib/shadgpu-env.sh && (. scripts/lib/shadgpu-env.sh; PEACOCK_CUDF=26.02 . scripts/lib/shadgpu-env.sh; echo "$CUDF_ROOT $CPP_INSTALL_DIR $REMOTE_REPO $CARGO_TARGET_DIR")`
Expected: `…/envs/rapids cpp/build26/install /home/info/peacockdb-2602 …/target-cudf-rapids`

Run: `scripts/cargo-cudf.sh test -p peacockdb-core --test test_ci_coverage`
Expected: PASS (`the_three_gpu_target_lists_agree` included).

- [ ] **Step 5: Build in the 26.02 mode.**

Run: `PEACOCK_CUDF=26.02 ./scripts/build-test-shadgpu.sh --build --build-benchmarks`
Expected: `cpp/build26/install/{bin,lib,rust-tests,rust-benchmarks}` populated; `cpp/install/` mtime unchanged.
Then: `readelf -d cpp/build26/install/lib/libpeacock_gpu.so | grep NEEDED` shows `libarrow.so.2100`,
and `ls cpp/build26/install/lib/libarrow.so.2100*` exists (bundled).

- [ ] **Step 6: Commit** — `scripts: a 26.02 mode for the shad-gpu flow, in its own dirs`.

### Task 2: Provision the remote dir and prove the runtime loads

**Files:** none in the repo beyond the detail file; remote `/home/info/peacockdb-2602/`.

- [ ] **Step 1: The remote dir.** The sf1 parquet is read in place from the 25.02 repo dir, never copied:

```bash
ssh shad-gpu 'set -e; R=/home/info/peacockdb-2602; mkdir -p $R/testdata $R/scripts
  for d in /home/info/peacockdb/testdata/*.sf1 /home/info/peacockdb/testdata/tpch.minimal; do
    [ -e "$d" ] && ln -sfn "$d" "$R/testdata/$(basename "$d")"; done; ls -l $R/testdata'
```

  (`pbench.sf1` is committed and arrives with the push if pbench added it to the push list; if it
  did not, add it to `--push-binaries`' hand-named list here — a gap in that task, noted.)

- [ ] **Step 2: Push and patch.**

Run: `PEACOCK_CUDF=26.02 ./scripts/build-test-shadgpu.sh --push-binaries --patch`
Expected: patch reports every binary under `bin/`, `rust-tests/`, `rust-benchmarks/` repointed to `~/glibc-2.35`.

- [ ] **Step 3: Every library resolves, from the right env.**

```bash
ssh shad-gpu 'R=/home/info/peacockdb-2602; LD=$R/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:$HOME/miniforge3/envs/rapids-2602/lib
  for f in $R/cpp/install/lib/libpeacock_gpu.so $R/cpp/install/bin/peacock_plan_tests; do
    LD_LIBRARY_PATH=$LD ldd $f | grep -E "not found|rapids|arrow|cudart|cudf"; done'
```

Expected: no `not found`; `libcudf.so`/`librmm.so` from `rapids-2602`; `libarrow.so.2100` from
`cpp/install/lib`; nothing from `rapids-cuda-12.2`.

- [ ] **Step 4: The runtime ladder** — run the smallest device binary, then stop at the first rung that passes:

```bash
ssh shad-gpu 'R=/home/info/peacockdb-2602; cd $R; LD=$R/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:$HOME/miniforge3/envs/rapids-2602/lib
  LD_LIBRARY_PATH=$LD cpp/install/bin/peacock_gpu_tests --gtest_filter="CudfGpu.*:RmmPool.*"'
```

  1. passes with the host's cudart 12.2 → record and go on;
  2. fails on cudart (`CUDA driver version is insufficient`, an unresolved `cudart` symbol, or
     `cudaErrorUnsupportedPtxVersion`) → copy the workstation's
     `~/data/miniforge3/envs/rapids/lib/libcudart.so.12*` into the remote `cpp/install/lib/` (first on
     the path) and rerun;
  3. still fails → build against a replica of the host's env: on the workstation
     `ssh shad-gpu '~/miniforge3/bin/conda list -n rapids-2602 --explicit' > /tmp/rapids-2602.txt &&
     conda create -n rapids-2602-shad --file /tmp/rapids-2602.txt`, then `PEACOCK_CUDF=26.02
     CUDF_ROOT=…/envs/rapids-2602-shad GCC_VERSION=12` with `CPP_BUILD_DIR=cpp/build26-shad` and a
     fresh `target-cudf-rapids-2602-shad`, and repeat from Task 1 Step 5;
  4. still fails → the spec's fallback: a temporary 26.02 host, recorded with the evidence.

- [ ] **Step 5: Record** the rung taken, the `ldd` output and the commands in
  `llm-wiki/tasks/verify-26.02-detail.md`; commit it.

### Task 3: The full gate on 26.02

- [ ] **Step 1: Run it detached** (tens of minutes; a backgrounded local chain dies):

Run: `PEACOCK_CUDF=26.02 ./scripts/build-test-shadgpu.sh --run-detached`, then poll
`PEACOCK_CUDF=26.02 ./scripts/build-test-shadgpu.sh --run-status` until finished.
Expected: the C++ loop ran every `peacock_*_tests` (count printed), the rust loop every staged
binary; exit 0, or the failures listed.

- [ ] **Step 2: The counts beside 25.02's.** For each tier in `build-test.md` that links cuDF — the
  C++ gtests (plan executor, cuDF smoke, `test_join_session`, the sf40 bare-cuDF suites), the
  `--lib -- gpu_tests::` rung, `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks`
  under `--skip bench_` — write a row `tier | 25.02 passed/failed | 26.02 passed/failed` into the
  detail file, the 25.02 numbers from the last green 25.02 gate log (`--run-status` in the default
  mode prints its tail) or a fresh 25.02 `--run`.

- [ ] **Step 3: The ffi rung and the cpu tiers, once, for the record** (workstation, no device):

Run: `CUDF_ROOT=~/data/miniforge3/envs/rapids CARGO_TARGET_DIR=$PWD/target-cudf-rapids cargo test -p peacockdb-core --lib -- ffi_tests::`
Run: `cargo test -p peacockdb-core --features rust-only --lib` (the cpu tier is cuDF-independent: one line saying so)
Expected: PASS; counts into the detail file.

- [ ] **Step 4: Commit** the detail file.

- [ ] **Step 3b: The device's answers on 26.02 against DuckDB.** One more corpus cycle in the
  26.02 mode with `PCK_WRITE_GPU_RESULT=26.02` (duckdb-oracle's writer then targets
  `gpu-result-26.02.txt`, gitignored), `--pull-results`, and locally
  `PCK_GPU_RESULT_VERSION=26.02 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus duckdb_gpu_`.
  A case red on 26.02 and green on the committed 25.02 file is a finding for Task 4; record the
  list either way in the detail file. Never copy the 26.02 file over `gpu-result.txt`.

### Task 4: Fix what fails

For each failure of Task 3, in order of tier (C++ first: a broken library breaks everything above it):

- [ ] **Step 1:** Use superpowers:systematic-debugging. Decide defect or 26.02 behaviour the engine
  should follow; the detail file gets one line either way.
- [ ] **Step 2:** For a defect, write the failing case at the lowest layer that shows it (a gtest
  for a cuDF call's behaviour; an operator harness case for a node) and run it red on 26.02.
- [ ] **Step 3:** Fix — portable code only (`__has_include` for headers, `CUDF_VERSION_MAJOR/MINOR`
  from `<cudf/version_config.hpp>` only where a call truly differs); rebuild in the 26.02 mode,
  rerun the case green, rerun its tier.
- [ ] **Step 4:** Commit per fix: `<what>: <the 26.02 difference>, with its case`.

### Task 5: The benchmark, per cuDF version

**Files:**
- Modify: `cpp/include/peacock_gpu.h:15`, `cpp/src/gpu_executor.cpp:119-127`, `peacockdb-ffi/src/lib.rs:264-274`
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs:49-56,~110`
- Modify: `peacockdb-core/tests/common/corpus_benchmark_cases.inc`
- Test: `peacockdb-core/tests/peacock_gpu_benchmarks.rs` (`a_modes_results_go_to_one_file_per_dataset_and_mode`)

**Interfaces:**
- Produces: `peacock_cudf_version()` → `"<major>.<minor>"` (`"26.2"`), `peacockdb_ffi::cudf_version()` → `"26.02"`
  (`"none"` under `rust-only`); `peacock_gpu_version()` and `peacockdb_ffi::version()` unchanged;
  `results_file(dataset, sf, mode)` →
  `testdata/benchmark-results/cudf-<version>/<dataset>.sf<sf>/<mode>.benchmark.txt`.

- [ ] **Step 1: The failing path test.** In `peacock_gpu_benchmarks.rs`, change the existing
  assertion to the versioned path:

```rust
let path = results_file("tpch", "40", "tp1_single");
let version = peacockdb_ffi::cudf_version();   // wraps peacock_cudf_version, Step 2
assert!(
    path.ends_with(format!("benchmark-results/cudf-{version}/tpch.sf40/tp1_single.benchmark.txt")),
    "{}", path.display()
);
```

Run: `scripts/cargo-cudf.sh test -p peacockdb-core --test peacock_gpu_benchmarks -- --skip bench_ a_modes_results`
Expected: FAIL (no `cudf-` component).

- [ ] **Step 2: A new symbol for the cuDF version.** `peacock_gpu_version()` keeps answering
  `"0.1.0"` — `cpp/tests/cpu/test_executor.cpp:9` asserts it, and CI's `ctest -L cpu` runs that —
  so the cuDF version gets its own additive symbol. `peacock_gpu.h`, beside `peacock_gpu_version`:

```c
/// The cuDF version the library was built against, "<major>.<minor>" (e.g. "26.2"). Static.
const char* peacock_cudf_version(void);
```

  `gpu_executor.cpp`, beside `peacock_gpu_version`:

```cpp
#include <cudf/version_config.hpp>
#define PEACOCK_STR2(x) #x
#define PEACOCK_STR(x) PEACOCK_STR2(x)
const char* peacock_cudf_version() {
  return PEACOCK_STR(CUDF_VERSION_MAJOR) "." PEACOCK_STR(CUDF_VERSION_MINOR);
}
```

  `peacockdb-ffi/src/lib.rs`: `fn peacock_cudf_version() -> *const std::ffi::c_char;` in `raw`'s
  extern block, and

```rust
/// The cuDF the library was built against, minor padded: "25.02", "26.02".
#[cfg(not(feature = "rust-only"))]
pub fn cudf_version() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        let raw = unsafe { std::ffi::CStr::from_ptr(raw::peacock_cudf_version()) }
            .to_str()
            .expect("version string is valid UTF-8");
        let (major, minor) = raw.split_once('.').expect("<major>.<minor>");
        format!("{major}.{:0>2}", minor)
    })
}

#[cfg(feature = "rust-only")]
pub fn cudf_version() -> &'static str {
    "none"
}
```

  A cpu gtest beside `test_executor.cpp:9`: `EXPECT_THAT(peacock_cudf_version(), MatchesRegex("[0-9]+\\.[0-9]+"))`
  (or a hand check of the digits if gmock's regex is unavailable there). `test_ci_coverage`'s
  symbol list, if it enumerates the ABI, gains the new symbol.

- [ ] **Step 3: The path and the run block.** `corpus_benchmark::results_file` joins
  `format!("benchmark-results/cudf-{}/{dataset}.sf{sf}", peacockdb_ffi::cudf_version())`; the run block writer
  (`corpus_benchmark.rs:~110`) adds `cudf=<version>` beside `build=` and `allocator=`. Move the
  two committed files under `testdata/benchmark-results/cudf-25.02/tpch.sf40/` with `git mv`
  (they were 25.02 runs); `--pull-benchmarks` already copies the whole `benchmark-results/` tree.

Run: the Step 1 command. Expected: PASS. Run `test_corpus_goldens` (its `benchmark` module reads the tree). Expected: PASS.

- [ ] **Step 4: The cells to time.** From join-backend's completeness comparison
  (`join-rewrite-cell-estimate.md`, the flipped rows), take every **tpch** query whose gpu cell
  turned on in this chain and add a `corpus_query_benchmark!(tpch, 40, <q>, <its enabled modes>)`
  line; tpcds and pbench have no sf40 data, so their flipped cells are listed in the detail file
  as not benchmarked, with that reason.

- [ ] **Step 5: Run both.**

Run: `./scripts/build-test-shadgpu.sh --build-benchmarks --push-binaries --patch --run-benchmarks-detached`, poll `--benchmark-status`, then `--pull-benchmarks`.
Run: the same with `PEACOCK_CUDF=26.02`.
Expected: `testdata/benchmark-results/cudf-25.02/tpch.sf40/*.benchmark.txt` and `cudf-26.02/…`,
each run block saying `cudf=`; a query that fails at sf40 (memory, plan) is a line in the detail file.

- [ ] **Step 6: Commit** — `benchmarks: per-version trees; the chain's tpch cells at sf40, 25.02 and 26.02`.

### Task 6: Record it

- [ ] **Step 1:** `llm-wiki/build-test.md`: the 26.02 mode (`PEACOCK_CUDF=26.02`, its dirs, the
  remote dir, the runtime rung Task 2 settled), the benchmark tree's `cudf-<version>/` level.
- [ ] **Step 2:** `tickets/system-hardening.md` #244: append the evidence — each tier's outcome on
  26.02 against 25.02, each fix, and the benchmark per query-mode (26.02 against 25.02, `run_us`).
- [ ] **Step 3:** Commit — `#244: what shad-gpu's 26.02 run showed`.

### Task 7: 25.02 still green

- [ ] **Step 1:** Run: `./scripts/build-test-shadgpu.sh --all` (default mode: 25.02, `cpp/install`, `/home/info/peacockdb`).
  Expected: exit 0, every tier's count equal to Task 3's 25.02 column.
- [ ] **Step 2:** If anything differs, it is a fix of Task 4 that broke 25.02: back to Task 4 for it.
- [ ] **Step 3:** Commit the detail file's final table.

## Self-review

- Spec coverage: where it runs (Tasks 1–2), the 26.02 build and dirs (Task 1), every tier (Task 3),
  fixes with red cases (Task 4), the benchmark on both versions (Task 5), #244 and the wiki (Task 6),
  the 25.02 rerun (Task 7), the temporary-host fallback (Task 2 Step 4.4).
