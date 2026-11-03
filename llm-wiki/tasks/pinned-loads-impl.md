# pinned-loads implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** split a parquet load's time into host read, H2D copy (pageable or pinned), decompression,
decode and the rest, from an Nsight trace; use that split on the H100 to decide whether the engine
must configure cuDF's pinned host memory; make that change if the profile says so; and close #149
with measured numbers either way.

**Architecture:** a new capture reader, `scripts/calibration/nsys_loads.py`, sits beside
`nsys_calls.py`. It reuses that module's region parsing and writes `testdata/calibration/loads.tsv`,
one row per load node per case. `create_nsys_profile.sh --trace` runs it after `nsys_calls.py`. The
engine change, if the before profile calls for it, is one idempotent function in `cpp/src/`
(`peacock::ensure_pinned_io_pool`). `execute_scan` calls it before its first `read_parquet`, so
every binary that reads parquet through the engine gets it. The function configures cuDF's own pinned
pool through the 25.02/26.02 API, and only on a device without `pageableMemoryAccess`.

**Tech stack:** Python 3 stdlib + sqlite3 (the calibration harness, no pytest), C++20 / libcudf
25.02 (`cudf/utilities/pinned_memory.hpp`), gtest, Nsight Systems on nebius-h100.

**Spec:** [`pinned-loads.md`](pinned-loads.md). Siblings: [`new-hosts.md`](new-hosts.md) supplies
the H100 run path (`scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu …`, `create_nsys_profile.sh` there).
[`bench-rows.md`](bench-rows.md) and [`sf40-baseline.md`](sf40-baseline.md) come after this task,
and their profiles read `loads.tsv`.

## Global constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan.
- Base: `ENS-pinned-loads` on top of `ENS-new-hosts` (master's engine plus containers' scripts). Read
  containers-only files with `git show origin/ENS-containers:<path>` until the base has them.
- The research comes before any engine change, and the before profile comes before any engine
  change. "Unless cuDF already pins" is decided from the source and the profile. It is not assumed.
- The decision is made on the integrated/discrete branch: `cudaDeviceProp::pageableMemoryAccess`.
  It is not made on `integrated`.
- The GPU tiers stay byte-identical. No golden moves.
- Out of scope, and each gets a ticket if the numbers point there: a host-buffer source, kvikio
  settings (`LIBCUDF_CUFILE_POLICY`, `KVIKIO_*`), the chunked reader (#232), uncompressed
  embeddings (#150).
- The pool is bounded and named. It must never size itself from host RAM, which is 196 GB on the H100.
- H100: `PCK_BENCH_HBM_SET=gh100`, `PCK_BENCH_HBM_PEAK_BW=3.35e12`. Every ssh from the Bash tool
  needs the sandbox off. A VM this task starts is stopped by it or left to its watcher.
- Never open `~/.nebius/*` or any `*.pem`.
- Python: plain module names; tests run with `python3 <file>` through `tests/harness.py`.
- C++: `git clang-format` on changed lines only.

## Review focus

1. **Multi-lane modes.** Two loads can be open at once (tp4, or a later fused DAG). Off-thread device
   work inside both windows must not be credited to either load; it is counted as ambiguous and
   printed. → Task 1, `test_concurrent_loads_leave_off_thread_work_unattributed`.
2. **A load with no H2D copy**, such as a `rows_only` scan or a load answered from metadata. Its
   rates are `0` and the row still writes. A division error or a dropped row is a failure.
   → Task 1, `test_load_without_copies_has_zero_rates`.
3. **A capture from an nsys without kernel names** (no `demangledName`), or one that has no loads.
   The reader refuses with a reason and writes nothing. It never writes all kernels as "other".
   → Task 1, `test_capture_without_kernel_names_is_refused`, `test_capture_without_loads_is_refused`.
4. **`LIBCUDF_PINNED_POOL_SIZE` set in the environment.** cuDF lets it override any configured size
   (`host_memory.cpp:147-149`). The engine reports the size cuDF will actually use; it never reports
   the constant. → Task 3, `PinnedIo.EnvironmentSizeWins`.
5. **cuDF's pinned resource is already configured before the engine's call**, for example because an
   earlier cuDF call touched it. The status says `AlreadyConfigured` and stderr says so. It is never
   reported as success. → Task 3, `PinnedIo.SecondCallReturnsTheFirstOutcome` and the state enum.

---

### Task 1: the load breakdown (`nsys_loads.py` → `loads.tsv`)

**Files:**
- Create: `scripts/calibration/nsys_loads.py`
- Modify: `scripts/calibration/tests/capture.py`: add columns to `SCHEMA` and new builder methods
- Create: `scripts/calibration/tests/test_loads.py`
- Modify: `scripts/create_nsys_profile.sh`: in the `--trace` block (containers' lines ~207-219),
  run `nsys_loads.py` after `nsys_calls.py`; add `LOADS_REL=calibration/loads.tsv`; add a header
  `WRITES` line
- Modify: `.gitignore`: `testdata/calibration/loads.tsv`. It comes from one host's run, as the
  capture does.
- Modify: `llm-wiki/build-test.md`:
  - "Benchmark data flow" (line ~1158): `└── nsys_loads.py ──► calibration/loads.tsv`, plus a
    column table;
  - the test-category row at line ~882: name `test_loads.py` and update the count;
  - the H100's HBM values next to the H200 defaults (line ~1786).

**Interfaces:**
- Consumes: `nsys_calls.domain_ids(conn)`, `nsys_calls.ranges(conn, domain_id)`,
  `nsys_calls.regions(own) -> [(case, seq, call, kind, partition, start, end, tid)]`,
  `record.write_tsv`, `record.read_tsv` (all containers' code, unchanged).
- Produces, for bench-rows, sf40-baseline and query-fuser profiles:
  - `scripts/calibration/nsys_loads.py --capture <sqlite> --out <tsv>` (exit non-zero, no file
    written, on refusal).
  - `LOAD_KIND = "CudfScan"`: a load is every partition region whose call kind is `CudfScan`.
  - `loads.tsv`: a `#` preamble, then these columns in this order. There is one row per
    `(dataset, sf, query, mode, recipe_seq)` load node. Each number is computed per execution and is
    the median over executions, column by column:

| column | meaning |
|---|---|
| `dataset sf query mode` | the case, as in `calls.tsv` |
| `recipe_seq` | the load's fb seq (the `GpuLoadParquet` node's `CudfScan` recipe) |
| `executions` | executions in the capture |
| `calls` | call indices per execution (batches the load was driven for) |
| `load_us` | Σ span of the load's partition regions in one execution; the same quantity as the tree's `GpuLoadParquet time_us` |
| `host_read_source` | `osrt` if the capture has `OSRT_API` rows, else `kvikio` if it has a `libkvikio` domain, else `none` |
| `host_read_us` | wall time, a union over threads, inside the load's windows, spent in `pread*`/`read`/`preadv*` OSRT calls, or else in depth-0 `libkvikio` ranges. kvikio's ranges contain its bounce copies, which `h2d_*` also count |
| `h2d_count h2d_bytes h2d_us` | HtoD memcpys launched in the window from any thread; `h2d_us` = Σ durations |
| `h2d_gbps` | `h2d_bytes / (h2d_us·1000)`: the per-copy rate, comparable to the probe's 55 / 10 GB/s; `0` if no copies |
| `h2d_pinned_bytes h2d_pinned_gbps h2d_pageable_bytes h2d_pageable_gbps` | the same split by CUPTI `srcKind`; another kind counts in the totals only |
| `h2d_max_bytes` | the largest single copy; this is the "too small to reach link rate" evidence |
| `decompress_us decode_us other_device_us` | Σ durations of kernels matching `DECOMPRESS_RE`, then `DECODE_RE`, then everything else launched in the window: other kernels, memsets, non-HtoD memcpys |
| `off_thread_device_us` | the part of all device time above that was launched from a thread other than the load's own; included in the columns above |
| `pinned_alloc_count pinned_alloc_us` | runtime/driver calls matching `PINNED_ALLOC_RE` that start in the window, from any thread; Σ host span |
| `unattributed_us` | `load_us` − \|union of host-read intervals, device-op `[start,end)` intervals and pinned-alloc intervals, each clipped to the windows\| |

  - Module constants (named so later tasks can extend them):
    - `MEMCPY_HTOD = 1`, `MEM_KIND_PAGEABLE = 1`, `MEM_KIND_PINNED = 2` (CUPTI enum values);
    - `DECOMPRESS_RE = r"unsnap|snappy|inflate|zstd|lz4|brotli|nvcomp|decompress"` (case-insensitive);
    - `DECODE_RE = r"cudf::io::parquet|gpuDecode|decode_page|DecodePage|ComputePage|page_header"`;
    - `PINNED_ALLOC_RE = r"^(cudaHostAlloc|cudaMallocHost|cudaHostRegister|cuMemHostAlloc|cuMemAllocHost|cuMemHostRegister)"`;
    - `KVIKIO_DOMAIN = "libkvikio"`.
- Attribution rule: a device op belongs to a load when its launching runtime/driver call (joined on
  `correlationId`) starts inside one of that load's partition windows `[start, end)`. It may be on
  any thread. A launch inside the windows of two different loads goes to neither. It is added to an
  `ambiguous` tally, which is printed. This replaces `nsys_calls.py`'s `(off-thread)` blind spot for
  loads only. `calls.tsv` is unchanged.

- [ ] **Step 1: extend the synthetic capture.** In `capture.py`:
  - `SCHEMA`: KERNEL gains `demangledName integer`; MEMCPY gains `bytes integer, copyKind integer,
    srcKind integer`; RUNTIME gains `end integer, nameId integer`; a new `OSRT_API (start integer,
    end integer, nameId integer, globalTid integer)` is created only by `osrt()`.
  - New methods:
    - `string(value) -> int`;
    - `kernel(launched_at, duration, name, tid=1)`;
    - `memcpy(launched_at, duration, nbytes, src_kind, tid=1, copy_kind=1)`;
    - `runtime_call(name, start, end, tid=1)`;
    - `osrt(name, start, end, tid=1)`;
    - `load(text, seq, call_index, start, end, tid=1)`: one case range around one `"{seq}.{i} CudfScan"`
      call range and its `p0` range, for tests that place device work by absolute time.
  - `device_work` keeps working unchanged. Run `python3 scripts/calibration/tests/test_calls.py`
    and `test_hbm.py`; expected: both PASS unchanged.

- [ ] **Step 2: failing tests**, in `test_loads.py`. Times are in ns. The helper `loads_of(capture, out)`
  mirrors `test_calls.calls_of`.
  - `test_load_row_splits_copy_from_decode`. One case, `tpch.sf40 q6 tp1-single`, with a load
    `0.0 CudfScan`/`p0` on tid 1 over `[0, 1_000_000)` (shifted by the capture clock), containing:
    - a `libkvikio` range `posix_device_read()` on tid 2 at `[100_000, 300_000)`;
    - a pinned memcpy of 4_000_000 B launched on tid 2 at 250_000, lasting 80_000;
    - a pageable memcpy of 1_000_000 B on tid 1 at 400_000, lasting 100_000;
    - kernel `cudf::io::unsnap_kernel` at 500_000, lasting 100_000;
    - kernel `void cudf::io::parquet::detail::gpuDecodePageData<int>()` at 600_000, lasting 150_000;
    - kernel `cudf::detail::cast_fn` at 750_000, lasting 50_000;
    - `cudaHostAlloc_v3020` at `[800_000, 820_000)` on tid 1.

    Assert the single row: `load_us=1000 host_read_source=kvikio host_read_us=200 h2d_count=2
    h2d_bytes=5000000 h2d_us=180 h2d_gbps=27.8 h2d_pinned_bytes=4000000 h2d_pinned_gbps=50.0
    h2d_pageable_bytes=1000000 h2d_pageable_gbps=10.0 h2d_max_bytes=4000000 decompress_us=100
    decode_us=150 other_device_us=50 off_thread_device_us=80 pinned_alloc_count=1
    pinned_alloc_us=20 unattributed_us=350 executions=1 calls=1 recipe_seq=0`.
  - `test_off_thread_work_outside_a_load_is_not_its_own`: a tid-2 memcpy launched inside a following
    `1.0 CudfFilter` region. The load row has `h2d_count=0`, and no row exists for seq 1.
  - `test_executions_take_the_median`: three executions with load spans 900/1000/5000 µs →
    `load_us=1000`, `executions=3`.
  - `test_batched_load_sums_its_calls`: call indices 0 and 1 per execution, each 400 µs → `calls=2`,
    `load_us=800`.
  - `test_osrt_preferred_when_present`: the case from the first test plus `osrt("pread64", 100_000,
    150_000, tid=2)` → `host_read_source=osrt`, `host_read_us=50`.
  - `test_load_without_copies_has_zero_rates`: a load with one decode kernel only → `h2d_count=0`,
    `h2d_gbps=0`, `h2d_pinned_gbps=0`, and the row is written.
  - `test_concurrent_loads_leave_off_thread_work_unattributed`: two loads on tids 1 and 3 overlap.
    A tid-2 memcpy is launched where both are open → `h2d_count=0` in both rows, and stdout names
    `ambiguous`.
  - `test_capture_without_kernel_names_is_refused`: drop `demangledName` from the KERNEL table →
    `raises(SystemExit, match="demangledName")`, and no `loads.tsv` exists.
  - `test_capture_without_loads_is_refused`: only a `CudfFilter` call →
    `raises(SystemExit, match="CudfScan")`, and no file exists.

- [ ] **Step 3: run them; expect FAIL** (`ModuleNotFoundError: nsys_loads`).
  `python3 scripts/calibration/tests/test_loads.py`

- [ ] **Step 4: implement `nsys_loads.py`.** Entry point: `main()`. Helpers:
  - `load_windows(regs) -> {(case, seq): {run: [(start, end, tid)]}}`. `run` is counted the way
    `nsys_calls.main` counts occurrences; it is divided by call indices to give executions.
  - `device_ops(conn) -> [(launch_start, launch_tid, op_start, op_end, kind, bytes, src_kind, name)]`,
    where `kind ∈ {kernel, memcpy, memset}`.
  - `union_us(intervals) -> int`.
  - `classify(name) -> "decompress"|"decode"|"other"`.

  Bisect over sorted window starts, as `DeviceWork` does. A capture that has no CUPTI MEMCPY table at
  all is valid and has zero copies. The docstring and `NOTES` preamble say what a row is, the
  attribution rule, and that times come from a profiled run.

- [ ] **Step 5: run; expect PASS.** Run
  `for f in scripts/calibration/tests/test_*.py; do python3 "$f" || exit 1; done`. Every file must
  pass.

- [ ] **Step 6: wire into the profile script.** After the `nsys_calls.py` line in the `--trace` block:
  `python3 scripts/calibration/nsys_loads.py --capture "testdata/$CAPTURE_REL.sqlite" --out
  "testdata/$LOADS_REL"`, then echo the row count. Run `bash -n scripts/create_nsys_profile.sh`
  and `shellcheck scripts/create_nsys_profile.sh`. Neither may print a new finding.

- [ ] **Step 7: wiki.** Make the `build-test.md` edits listed under Files. The column table is the one
  above, in short sentences. Add the H100 line: `PCK_BENCH_HBM_SET=gh100
  PCK_BENCH_HBM_PEAK_BW=3.35e12` (H100 80GB HBM3).

- [ ] **Step 8: commit.** `git add scripts/calibration/nsys_loads.py scripts/calibration/tests/{capture,test_loads}.py
  scripts/create_nsys_profile.sh .gitignore llm-wiki/build-test.md && git commit -m "pinned-loads:
  nsys_loads.py splits a parquet load into read, copy, decompress and decode"`

### Task 2: what cuDF 25.02 does, and the before profile on the H100

**Files:**
- Create: `llm-wiki/tasks/pinned-loads-detail.md`, with sections "cuDF 25.02 default", "Before
  profile", "Before load times", "Decision".

**Interfaces:**
- Consumes: Task 1's `loads.tsv`; new-hosts' `run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu prepare|ship|run|pull`, and
  `create_nsys_profile.sh` as new-hosts made it run on nebius-h100 (its build-test.md section names
  the exact `--host`/`--remote-*` arguments).
- Produces: the detail file's **Decision**: exactly one of `CONFIGURE_POOL`, `RAISE_THRESHOLD`, or
  `NULL_RESULT`, with the numbers that chose it. Task 3 reads it.

- [ ] **Step 1: research the default, from source.** Read `third_party/cudf`'s tag (`git -C third_party/cudf show
  v25.02.02:<path>`; the submodule's checkout is 25.10-dev, not 25.02) and the installed kvikio
  25.02.01 headers. Quote each of these with line numbers in the detail file, and confirm or correct
  the expected reading:
  - `cpp/src/utilities/host_memory.cpp:142-163`: the default IO host resource is a **pinned**
    fixed pool. Its size is `LIBCUDF_PINNED_POOL_SIZE`, else `min(total device memory / 200,
    100 MiB)`, which is **100 MiB on an 80 GB H100**. Allocations above the pool go to an upstream
    pinned resource, one `cudaHostAlloc` each (lines 59-70).
  - `host_memory.cpp:293-306`: `allocate_host_as_pinned_threshold` defaults to 0, so
    `make_host_vector` (`vector_factories.hpp:365-367`) is **pageable**.
    `cudf::detail::hostdevice_vector` (`io/utilities/hostdevice_vector.hpp:56`) is pinned.
  - `cpp/src/io/utilities/config_utils.cpp:38-63`: `LIBCUDF_CUFILE_POLICY` defaults to `KVIKIO`,
    with kvikio compat mode ON and 4 threads.
  - `cpp/src/io/utilities/datasource.cpp:50-125`: `file_source` prefers device reads through
    kvikio. `io/parquet/reader_impl_preprocess.cu:252-259`: the column-chunk bytes go through
    `device_read_async`, not through cuDF's host resource.
  - kvikio `include/kvikio/posix_io.hpp:~159`: a compat-mode read copies through kvikio's own pinned
    bounce buffer (`cuMemcpyHtoDAsync`). `defaults.hpp:248-260`: 16 MiB by default.

  The expected conclusion is that the bulk H2D is already pinned, but in 16 MiB pieces, from kvikio
  threads. The spec's API `cudf::io::set_host_memory_resource` does not exist in 25.02 or 26.02. Its
  successors are `cudf::set_pinned_memory_resource` and `cudf::config_default_pinned_memory_resource`
  in `cudf/utilities/pinned_memory.hpp`. Record that.

- [ ] **Step 2: build, then start and ship today's engine** to nebius-h100 (cuDF 25.02 run
  image). First the staged install in the 25.02 build image (new-hosts Task 10 Step 2:
  `stage-install.sh` on nebius-cpu128, its `cpp/install/` rsynced into this tree's `cpp/install/`;
  the CPU VM stopped right after). Only then
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu start` and
  `scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu ship` (default `--install cpp/install`), as new-hosts documents. Pass: `ship` exits 0.

- [ ] **Step 3: before load times (unprofiled).** Run this for each of `q1 q3 q6 q8`:
  `PCK_TEST_FILTER=bench_tpch_sf40_<q>_tp1_single scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu run bench`, then
  `pull`. Pass: each run exits 0 and
  `testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt` has the section.
  Copy every `GpuLoadParquet` line's `table=`, projections and `total_us` into "Before load times".
  Do not commit the results tree from this run. sf40-baseline owns it.

- [ ] **Step 4: before profile.** For each `<q>` in `q1 q3 q6 q8`, run
  `PCK_TEST_FILTER=bench_tpch_sf40_<q>_tp1_single PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12
  scripts/create_nsys_profile.sh --trace <new-hosts' H100 arguments>`. Then
  `cp testdata/calibration/loads.tsv testdata/calibration/loads-<q>-before.tsv` and
  `git checkout -- testdata/calibration/calls.tsv`; the committed calls.tsv is the H200's.

  Pass: each run prints `the load breakdown: N rows` with N ≥ 1, and the `recipes:` line matches the
  plan. Then check the kernel names. List the distinct kernel names in the load windows, with a
  one-off `sqlite3` query in the detail file. Every decompression and decode kernel must be caught by
  `DECOMPRESS_RE`/`DECODE_RE`. If one is not, fix the pattern in Task 1's module with a test case
  named for it, and rerun.

- [ ] **Step 5: write the breakdown into the detail file.** Give one table per query, one row per
  load, with every `loads.tsv` column. Add the sentence the spec asks for: the copies run at the
  pageable rate (~10 GB/s) or the pinned rate (~55 GB/s), as read from `h2d_pinned_gbps` and
  `h2d_pageable_gbps`. Also give `h2d_max_bytes`, and the copy's share of `load_us`:
  `h2d_us / load_us`, and `h2d_pageable_bytes/10 GB/s − h2d_pageable_bytes/55 GB/s` as the most
  pinning could save.

- [ ] **Step 6: decide.** Write the Decision, using the four loads' median:
  - `RAISE_THRESHOLD` if `h2d_pageable_bytes ≥ 10%` of `h2d_bytes`, **and** Step 1 traced those
    copies to `make_host_vector` (pageable by threshold). If they come from the datasource path
    instead, that is out of scope: file a ticket and choose by the next rule.
  - Otherwise `CONFIGURE_POOL` if `pinned_alloc_us ≥ 5%` of `load_us`. This is cuDF's 100 MiB pool
    overflowing into a `cudaHostAlloc` per buffer.
  - Otherwise `NULL_RESULT`. cuDF already pins what it copies, and #149 closes as misdiagnosed.

  Stop the VM: `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile
  editor-eu stop`.

- [ ] **Step 7: commit.** `git add llm-wiki/tasks/pinned-loads-detail.md && git commit -m "pinned-loads:
  cuDF 25.02's IO host memory, and the H100 before profile"`

### Task 3: the engine configures cuDF's pinned IO memory on a discrete device

Skip it when the Decision is `NULL_RESULT`. Then write "Task 3 skipped: NULL_RESULT" in the detail
file and go to Task 4.

**Files:**
- Create: `cpp/src/peacock/pinned_io.h` (declarations), `cpp/src/pinned_io.cpp`
- Modify: `cpp/CMakeLists.txt:148-169`: add `src/pinned_io.cpp` to `peacock_gpu`
- Modify: `cpp/src/operators/scan.cpp:92-99`: call `ensure_pinned_io_pool()` before `read_parquet`
  (and before `read_parquet_metadata` in `rows_of`, line 25)
- Modify: `cpp/tests/gpu/test_cudf.cpp`: new `PinnedIo.*` cases; `main()` (line 51) calls
  `peacock::ensure_pinned_io_pool()` right after `install_rmm_pool`. This is the engine function,
  not a copy of the cuDF call. It runs first so that no earlier cuDF call configures the default.
- Modify: `llm-wiki/architecture.md` "cuDF options" table (line ~1341): a row for the pinned IO pool

**Interfaces:**
- Consumes: Task 2's Decision and its measured peak.
- Produces:

```cpp
namespace peacock {
// Bounded; from Task 2's profile: the next power of two ≥ 2× the largest per-load pinned
// demand seen, floor 256 MiB, cap 4 GiB. 1 GiB unless the profile says otherwise.
inline constexpr std::size_t kPinnedIoPoolBytes = std::size_t{1} << 30;
// Only for RAISE_THRESHOLD: host vectors up to 16 MiB go to the pinned pool; larger stay
// pageable so one big vector cannot drain the pool into per-call cudaHostAlloc.
inline constexpr std::size_t kPinnedHostVectorThreshold = std::size_t{16} << 20;

struct PinnedIoStatus {
  enum class State { Configured, AlreadyConfigured, KeptDefault /* pageableMemoryAccess */ };
  State state = State::KeptDefault;
  std::size_t pool_bytes = 0;  // what cuDF will use: LIBCUDF_PINNED_POOL_SIZE wins
};
bool wants_pinned_io_pool(int pageable_memory_access);       // pure: == 0
std::size_t pinned_io_pool_bytes();                          // env, else kPinnedIoPoolBytes
const PinnedIoStatus& ensure_pinned_io_pool();               // once per process (std::call_once)
}
```

  `ensure_pinned_io_pool` reads the current device's `pageableMemoryAccess`. On a discrete device it
  calls `cudf::config_default_pinned_memory_resource({pinned_io_pool_bytes()})`, and `false` maps to
  `AlreadyConfigured`. For `RAISE_THRESHOLD` it also calls
  `cudf::set_allocate_host_as_pinned_threshold(kPinnedHostVectorThreshold)`. It writes one stderr
  line, shaped like rmm_pool's: `[pinned-io] cuDF pinned pool 1.0 GiB on a discrete device` /
  `… already configured …` / `… kept cuDF's default (pageable memory access)`. Leave out whichever
  constant the Decision does not use.

- [ ] **Step 1: failing gtests**, in `test_cudf.cpp`:
  - `PinnedIo.IntegratedKeepsTheDefault`: `EXPECT_FALSE(wants_pinned_io_pool(1));
    EXPECT_TRUE(wants_pinned_io_pool(0));`. This covers the GB10 branch without a GB10.
  - `PinnedIo.DiscreteDeviceGetsTheBoundedPool`: if the device has `pageableMemoryAccess == 0`,
    then `ensure_pinned_io_pool().state == Configured`, `pool_bytes == pinned_io_pool_bytes()`,
    and `pool_bytes <= 4 GiB`. A 1 MiB allocation from `cudf::get_pinned_memory_resource()` has
    `cudaPointerGetAttributes(...).type == cudaMemoryTypeHost`. Otherwise `state == KeptDefault`.
  - `PinnedIo.SecondCallReturnsTheFirstOutcome`: two calls return the same address and the same
    state.
  - `PinnedIo.EnvironmentSizeWins`: `setenv("LIBCUDF_PINNED_POOL_SIZE", "268435456", 1)` makes
    `pinned_io_pool_bytes() == 268435456`. `unsetenv` brings back `kPinnedIoPoolBytes`.

- [ ] **Step 2: build; expect FAIL to compile** (`pinned_io.h` missing). Use the 25.02 build image,
  `scripts/build.sh` as build-test.md documents for cudf 25.02.

- [ ] **Step 3: implement** `pinned_io.cpp` and the two `scan.cpp` calls. No other engine file
  changes.

- [ ] **Step 4: build for both cuDF versions.** Build in the 25.02 and the 26.02 build images. Pass:
  both compile. The API exists in both (`pinned_memory.hpp:32-57` in third_party).

- [ ] **Step 5: run on the H100.** Ship, then run `scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu run list`. Pass:
  the device tiers have the same pass counts as new-hosts' sf1 corpus run, the four `PinnedIo.*`
  pass, and stderr shows `[pinned-io] cuDF pinned pool … on a discrete device`. Byte-identity:
  the GPU corpus goldens pass unmodified.

- [ ] **Step 6: wiki.** Add the row to the architecture.md "cuDF options" table:
  - Option: `cudf::config_default_pinned_memory_resource` (and the threshold, if used).
  - Set at: `pinned_io.cpp`, from `scan.cpp`.
  - Value: `kPinnedIoPoolBytes`; `LIBCUDF_PINNED_POOL_SIZE` overrides it. Host cost: that many bytes
    pinned for the life of the process, allocated at the first load. Discrete devices only.
  - What the default would do: a 100 MiB pool, then one `cudaHostAlloc` per larger buffer, as
    measured in Task 2.

- [ ] **Step 7: commit.** Run `git clang-format` first. Then
  `git commit -m "pinned-loads: the engine sizes cuDF's pinned IO pool on a discrete device (#149)"`.

### Task 4: the after profile, the before/after times, and #149 closed

**Files:**
- Modify: `llm-wiki/tasks/pinned-loads-detail.md`: sections "After profile", "Before/after",
  "Integrated: deferred"
- Modify: `llm-wiki/tickets/performance.md`: remove #149, its `## Contents` line 9 and the `#148`
  text that points at it, if any
- Modify: `llm-wiki/archive/archived-tickets.md` (`## Done`): #149 with the measured numbers; or under
  stale/obsolete if the result is `NULL_RESULT`, as "misdiagnosed"
- Modify: `llm-wiki/tickets.md`: performance count 9→8, and drop `#149` from the index line (33)

**Interfaces:**
- Consumes: Tasks 1-3.
- Produces: nothing new in code.

- [ ] **Step 1: after load times and profile.** Only if Task 3 ran. Start the VM, build and ship as
  in Task 2 Step 2, then repeat Task 2 Steps 3-4 with the suffix `-after`. Pass: the same as there.

- [ ] **Step 2: read the copy rate.** Pass condition: in every load with `h2d_max_bytes ≥ 16 MiB`,
  the after `h2d_pinned_gbps` is within 20% of 55.5 GB/s. If not, the detail file explains the gap
  from the profile. Possible explanations: the copy size (`h2d_max_bytes`, against the probe's 4 GiB),
  kvikio's 16 MiB bounce chunks, or copies overlapping on the link (`h2d_us` against the union of
  copy intervals). The task does not move on until the gap is explained. For `NULL_RESULT`, Step 2
  applies to the before profile: show it, explain it, and state that nothing changed.

- [ ] **Step 3: before/after table.** In the detail file, one row per query × load:
  - `GpuLoadParquet total_us` before and after (unprofiled);
  - the change;
  - the copy-share estimate from Task 2 Step 5.

  Write one paragraph. A gain smaller than the share is a finding, explained. A gain within the
  run-to-run spread of `runs=[…]` is noise and is recorded as noise. Open a ticket in
  `tickets/performance.md` only if the profile points at decode, decompression or the read path, and
  name it by what the profile shows.

- [ ] **Step 4: integrated, deferred.** One line: no GB10 is available to this chain. The branch is
  covered by `PinnedIo.IntegratedKeepsTheDefault`, and #149's "integrated does not regress"
  measurement is deferred.

- [ ] **Step 5: close #149.** Move the ticket text to the archive with: the cuDF 25.02 finding, the
  four queries' before/after numbers, the H2D rates by kind, and the decision. Update the index and
  the counts. Then run `python3 scripts/ticket_widget.py`. Pass: it exits 0, and
  `testdata/report.html` changes only if the registry names #149 (`grep -n 149
  testdata/cost-registry.csv`).

- [ ] **Step 6: CPU gates.** Run `for f in scripts/calibration/tests/test_*.py; do python3 "$f" ||
  exit 1; done` and `cargo test -p cost-report --test test_source_formatting`. Pass: both are green.
  Then stop the VM, or confirm that its watcher is installed.

- [ ] **Step 7: commit.** Commit the detail file, the tickets, the archive, `tickets.md` and the
  report if it changed, with the message `pinned-loads: H100 before/after, #149 closed`.

## Self-review notes

- Spec A → Task 1. B → Task 2, Steps 3-5. C → Task 4, Step 2. 0 → Task 2, Step 1. 1 → Task 3,
  conditional on the Decision, as the spec's "unless cuDF already pins" requires. 2 → Task 2 Step 3
  and Task 4 Step 3. 3 → Task 3's constant, its test bound and the architecture.md row. 4 → Task 4
  Step 5. Constraints: GB10 deferred → Task 4 Step 4; the H100 via new-hosts → Task 2's Consumes.
- `cudf::io::set_host_memory_resource`, named by the spec, is absent from both pinned cuDF versions.
  Task 3 uses its successor. Task 2 Step 1 records this.
