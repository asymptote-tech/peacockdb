# pinned-loads implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** split a parquet load's time into host read, H2D copy (pageable or pinned), decompression,
decode and the rest, from an Nsight trace; make the engine's default load read each file from a
pinned host file cache through a host-buffer source, through a one-chunk
`cudf::io::chunked_parquet_reader`; configure cuDF's pinned IO pool if the before profile says so;
measure each fix alone and together on the H100; and close #149 and #232 with the numbers.

**Architecture:** a new capture reader, `scripts/calibration/nsys_loads.py`, sits beside
`nsys_calls.py` and writes `testdata/calibration/loads.tsv`, one row per load node per case;
`create_nsys_profile.sh --trace` runs it. On the engine side, a process-wide
`peacock::PinnedHostCache` (`cpp/src/pinned_host_cache.cpp`) holds whole parquet files in
`cudaHostAlloc`'d memory, LRU, evicting only entries no read holds, reloading a file whose size or
mtime moved. `execute_scan` takes a lease on its files; with every file resident it reads
`cudf::io::source_info` over the pinned spans, else the paths as today. Either source goes through
`read_scan_table`, a one-chunk `chunked_parquet_reader`. **The benchmark fills the cache once per
process, before its first case**, through the C symbol `peacock_pinned_cache_prefill` (Rust
`prefill_pinned_cache`): it plans every case the process will run, collects their files
(`plan_files`), prefills the union, and every warm-up and measured round of every case reads those
pinned buffers. No measured number depends on the page cache; a section whose rounds filled or fell
back is a failed section. The cache's size is the session option `pinned_host_cache_bytes`: a
DataFusion config extension (`peacock.pinned_host_cache_bytes`) on the Rust side,
`peacock_set_pinned_host_cache_bytes` on the C side, `PEACOCK_PINNED_HOST_CACHE_BYTES` in the
environment over both; the cache and its option are process-wide, and the last set wins. cuDF's own
pinned pool (`peacock::ensure_pinned_io_pool`) is configured only if the before profile's Decision
says so. All of it is on a discrete device only (the `pageableMemoryAccess` branch).

**Tech stack:** Python 3 stdlib + sqlite3 (the calibration harness, no pytest), C++20 / libcudf
25.02 (`cudf/io/parquet.hpp`, `cudf/utilities/pinned_memory.hpp`), gtest, Rust (DataFusion 45
`extensions_options!`), Nsight Systems on nebius-h100.

**Spec:** [`pinned-loads.md`](pinned-loads.md). Siblings: [`new-hosts.md`](new-hosts.md) supplies
the H100 run path (`scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu …`, `create_nsys_profile.sh` there).
[`bench-rows.md`](bench-rows.md) and [`sf40-baseline.md`](sf40-baseline.md) come after this task;
their profiles read `loads.tsv`, and bench-rows' trailer reads the cache's state (Task 4's
`PinnedCache`) and the process's prefill (Task 4b's `PrefillStats`).

## Global constraints


- **VM time is minimal** (the human's rule, 2026-11-03). A VM starts right before its first
  remote command and stops right after its last `pull`. It is never left up across a wait: a local
  build, CI, a review, the human. Binaries are built and staged first, and the H100 starts only
  once they are ready to ship. No step leaves a VM running for a later task. Each start and stop
  goes into the detail file's `## VM time` table, in the host's local time, with the hours per VM
  for the task. The watchers (1 h idle, 4 h per process, the hourly GitHub check) are the backstop,
  not the plan. Order (C4): the staged install is built first, on nebius-cpu128, which is stopped
  right after; then the H100 starts, runs, profiles, pulls and stops; only then local tests, fixes
  and commits. Never a cargo or cmake edit, build or test with the H100 up. This plan has two H100
  sessions: Task 2 (today's engine) and Task 7 (every change at once), and two L40S brackets:
  Task 2 Step 3 (`devprops` only, before any engine change) and Task 7 Step 6 (the list, cache on
  and off); Tasks 3-6 are tested offline and on no VM.
- **Developers never commit, push, or dispatch a workflow** (`prompts.md`). A step that says
  *Commit `<msg>`* means: report a checkpoint `<msg>` and stop; the coordinator stages, commits and pushes, then
  dispatches the next step. Only the plan's last step (Task 8 Step 6) is green. A step marked *(coordinator)* is the coordinator's; the developer stops before it
  and says what it needs.
- **The staged install is new-hosts' "CPU-VM build"** (new-hosts' Global Constraints): the chain
  workspace as it stands, uncommitted, sent to nebius-cpu128 by `scripts/nebius-sync.sh
  --instance-id computeinstance-e01t2mm4265y8x88ey --profile eu-west1 --no-data`, built there with
  `scripts/env/stage-install.sh` in the 25.02 build image, its `cpp/install/` rsynced back over
  `-e "$C rsh"`, the CPU VM stopped. Never a `git pull` on a VM.
- Host roles (`build-test.md`, "Nebius host", host roles): the H100 runs the sf40 benchmark and
  Nsight through `run-gpu.sh`; the L40S runs correctness only (Task 2 Step 3's `devprops`, Task 7
  Step 6's list); the H100 runs no correctness list, only `run filter` (Task 7 Step 2); the CPU
  VM stages the H100's install. An H100 start that exits 75 (no capacity) is reported and the step stops;
  the coordinator owns the retry.
- **The H100's record:** `run-tests.sh` refuses a non-empty `~/peacock/<i>/testdata/calibration/records.tsv`.
  After every `pull` of a `run bench`, remove it:
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu ssh 'rm -f ~/peacock/<i>/testdata/calibration/records.tsv'`.
- Base: `ENS-pinned-loads` on top of `ENS-new-hosts` (master's engine plus containers' scripts). Read
  containers-only files with `git show origin/ENS-containers:<path>` until the base has them.
- The research and the before profile come before any engine change. "Unless cuDF already pins" is
  decided from the source and the profile. It is not assumed.
- **The discrete/integrated branch** is `cudaDeviceProp::pageableMemoryAccess`, never `integrated`:
  `peacock::is_discrete_for_pinning(int pageable_memory_access)` (Task 3) is the one predicate,
  for the cache and the pool. An HMM host (open kernel module, kernel ≥ 6.1) reports
  `pageableMemoryAccess = 1` on a discrete card, so Task 2 Step 3 records the attribute on the
  L40S and the H100 before Task 3 starts. If a discrete card reports 1, stop for the human
  (`blocked(building)`): the spec names the attribute, and the branch is the human's to change.
- **No device, no cache** (decided). If `cudaGetDeviceCount` fails or returns 0, or
  `cudaGetDeviceProperties` fails, the cache's capacity is 0 and its state is `NO_DEVICE` (Rust
  `PinnedCacheState::NoDevice`, Display `off no-device`): the CUDA error is cleared
  (`cudaGetLastError`), nothing throws, and a prefill returns `Off`. This is tested in the ffi
  rung, which has no device by definition (`build-test.md` "ffi"; CI's dataset-matrix runs
  `test_ffi` with no GPU). Every test that needs the cache on runs in a device tier run with
  `--test-threads=1`: `peacock_gpu_tests`, or the lib's `gpu_tests::` rung (the existing
  `peacockdb_core_gpu_lib` target, so no GPU target list and no CI wiring guard changes).
- **The cache's names, used by every later chain L plan:** session option
  `pinned_host_cache_bytes` (Rust `peacock.pinned_host_cache_bytes`; unset = the engine default;
  `0` = off); default `min(host RAM, the cgroup memory limit when one is set) ×
  kPinnedHostCacheRamNumerator / kPinnedHostCacheRamDenominator`, 3/4 (147 GB on the 196 GB H100 VM
  with no container limit); environment `PEACOCK_PINNED_HOST_CACHE_BYTES=<bytes>` wins over both, as
  `PEACOCK_RMM_POOL_BYTES` does; the cache and its option are **process-wide: the last set wins**
  (the human's decision, 2026-11-03; architecture.md says so, Task 8); the measurement switch
  `PEACOCK_SCAN_READER=chunked|read_parquet` (default `chunked`); stderr prefix `[pinned-cache]`.
  The prefill: C `peacock_pinned_cache_prefill(executor, paths, n_paths, peacock_prefill_stats_t*)`;
  Rust `prefill_pinned_cache(&self, files: &[PathBuf]) -> Result<PrefillStats, PrefillError>`,
  `PrefillError::{DoesNotFit { needed, limit }, Io { path, error }, Off}`; `plan_files(roots) ->
  BTreeSet<PathBuf>`; the benchmark's refusal reason `prefill: needs <needed> > limit <limit>`.
- **The ABI grows by three symbols**, as the spec's Scope now lists, each because nothing else can do its job:
  `peacock_set_pinned_host_cache_bytes` is how the session option reaches the C++ cache before the
  prefill (the ABI has no process-wide options door; plan bytes are per executor and arrive after
  the prefill, and the rmm pool has its own symbol for the same reason), and
  `peacock_pinned_host_cache_stats` is the only way to read the counters the benchmark asserts on
  (no fill, no fallback across a case's rounds) and bench-rows prints (`pinned_cache=`).
- **The GPU tiers stay byte-identical, with the cache on and off** (`PEACOCK_PINNED_HOST_CACHE_BYTES=0`),
  and with either reader. No golden moves. The CPU backend and the CPU corpus are untouched: the
  cache lives in `libpeacock_gpu`, which a `rust-only` build does not link; the only Rust change a
  CPU run sees is the registered (unset) config extension, and the CPU corpus goldens stay
  byte-identical (Task 4 Step 7).
- Out of scope, and each gets a ticket if the numbers point there: kvikio settings
  (`LIBCUDF_CUFILE_POLICY`, `KVIKIO_*`), GDS, uncompressed embeddings (#150), a cache shared
  across processes.
- cuDF's pinned pool (Task 6) is bounded and named and never sized from host RAM. The file cache
  is the one pinned allocation sized from host RAM, and only through the divisor.
- H100: `PCK_BENCH_HBM_SET=gh100`, `PCK_BENCH_HBM_PEAK_BW=3.35e12`. Every ssh from the Bash tool
  needs the sandbox off. A VM this task starts is stopped by it or left to its watcher.
- Never open `~/.nebius/*` or any `*.pem`.
- Python: plain module names; tests run with `python3 <file>` through `tests/harness.py`.
- C++: `git clang-format` on changed lines only. Rust: the visibility gates
  (`cost-report/tests/test_source_formatting/visibility.rs`): no `pub(super)`, no `pub use`.

## Review focus

1. **Multi-lane modes.** Two loads can be open at once (tp4, or a later fused DAG). Off-thread device
   work inside both windows must not be credited to either load; it is counted as ambiguous and
   printed. → Task 1, `test_concurrent_loads_leave_off_thread_work_unattributed`.
2. **A read racing an eviction.** tp4's lanes and a fused DAG's scans acquire the cache from
   several threads; a buffer freed under a read is a wrong answer or a fault, not an error. No
   leased entry is ever freed or reused. → Task 3, `PinnedHostCache.ConcurrentReadAndEviction`,
   `PinnedHostCache.LeasedEntryIsNeverEvicted`, `PinnedHostCache.SecondReaderWaitsForTheFill`.
3. **A file rewritten between runs** (regenerated testdata, a re-fetched sf40 file). Same size,
   new mtime, still reloads; a reader holding the old bytes keeps them. → Task 3,
   `PinnedHostCache.ChangedFileIsReloaded`, `PinnedHostCache.ChangedFileWhileLeasedKeepsTheOldBytes`.
4. **A scan that does not fit, or a `cudaHostAlloc` that fails** (pinned memory is not swappable;
   the host can refuse). The whole scan reads its paths, never a mix of sources, the run says so
   once per file, and nothing throws. → Task 3, `PinnedHostCache.ScanThatDoesNotFitReadsEveryPath`,
   `PinnedHostCache.AllocationFailureFallsBack`, `PinnedHostCache.FallbackIsSaidOnce`.
5. **The chunked reader returning several chunks** (a limit set by mistake, a cuDF whose "0 = no
   limit" changed). The scan's table must equal `read_parquet`'s. → Task 5,
   `ScanReader.SeveralChunksConcatenateToReadParquet`.
6. **A load with no H2D copy**, such as a `rows_only` scan. Its rates are `0` and the row still
   writes. → Task 1, `test_load_without_copies_has_zero_rates`.
7. **A capture from an nsys without kernel names**, or one with no loads: refused with a reason,
   nothing written. → Task 1, `test_capture_without_kernel_names_is_refused`,
   `test_capture_without_loads_is_refused`.
8. **`LIBCUDF_PINNED_POOL_SIZE` set, or cuDF's pinned resource already configured**, if Task 6
   runs: the engine reports what cuDF will use and `AlreadyConfigured`, never success. → Task 6,
   `PinnedIo.EnvironmentSizeWins`, `PinnedIo.SecondCallReturnsTheFirstOutcome`.
9. **A prefill that does not fit, halfway.** Pinning the first files and then refusing leaves the
   host short of pinned memory and the cache holding some of a dataset; the call refuses before
   any allocation, evicting nothing. → Task 3, `PinnedHostCache.PrefillRefusesBeforePinningAnything`,
   `PinnedHostCache.PrefillCountsLeasesAgainstTheLimit`; Task 4b,
   `a_refused_prefill_fails_every_case_with_its_size`.
10. **A case the process runs that the prefill did not plan** (a libtest filter the selection
   misread, a mode spelled differently). Its loads would fill from disk mid-round. The case fails
   naming the file, before its warm-up. → Task 4b, `selection_follows_libtest_filters`,
   `a_case_outside_the_prefill_is_refused`.
11. **A benchmark run in a container with a memory limit** (`docker run --memory`). Three quarters of
   host RAM could exceed the container's limit and the OOM killer ends the run; the default takes
   the smaller. → Task 3, `PinnedHostCache.DefaultIsThreeQuartersOfTheSmallerOfRamAndCgroup`,
   `PinnedHostCache.CgroupLimitIsParsed`.
12. **No device** (CI's ffi rung, a container without `--gpus`): the cache is off, says so, and
   nothing throws. → Task 3, `PinnedHostCache.NoDeviceIsOff`; Task 4,
   `pinned_host_cache_is_off_without_a_device` (`test_ffi`).
13. **A fill that fails while others wait for it**, or two fills at once near the ceiling: every
   waiter wakes and falls back, nothing hangs, and the cache never passes its capacity. → Task 3,
   `PinnedHostCache.AFailedFillWakesItsWaiters`, `PinnedHostCache.ConcurrentFillsStayUnderCapacity`,
   `PinnedHostCache.ReleaseEvictsWhileOverCapacity`.
14. **A file larger than one `pread`** (Linux returns at most 0x7ffff000 bytes per call; sf40
   `lineitem` is about 9 GB). → Task 3, `PinnedHostCache.ShortReadsAreLooped`,
   `PinnedHostCache.ShortReadAtEofIsAChangedFile`.
15. **A refused `cudaHostAlloc`** leaves the runtime's last error set, and cuDF's next
   `cudaPeekAtLastError` check would throw it. → Task 3,
   `PinnedHostCache.RefusedAllocationClearsTheCudaError` (GPU).
16. **A micro query** (`aggregate_groupby`, file `aggregate-groupby.sql`) in the prefill's plans.
   → Task 4b, `a_micro_query_plans_by_its_file_name`, `selection_follows_libtest_filters`.

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

- [ ] **Step 8: Commit** `pinned-loads: nsys_loads.py splits a parquet load into read, copy, decompress and decode`
  (paths: `scripts/calibration/nsys_loads.py`, `scripts/calibration/tests/{capture,test_loads}.py`,
  `scripts/create_nsys_profile.sh`, `.gitignore`, `llm-wiki/build-test.md`).

### Task 2: what cuDF 25.02 does, the before profile, and #232's re-measure on the H100

**Files:**
- Create: `llm-wiki/tasks/pinned-loads-detail.md`, with sections "cuDF 25.02 default", "Discrete
  cards", "Host memory", "Before profile", "Before load times", "Readers from pinned memory
  (#232)", "Decision".
- Create: `cpp/tests/gpu/test_reader_probe.cpp`, added to `peacock_gpu_tests`' sources in
  `cpp/CMakeLists.txt` (`add_executable(peacock_gpu_tests …)`, line ~205). A test, not an engine
  change: one `DISABLED_` case, run only by name.
- A one-off probe, not committed: `devprops.cu` in the scratchpad, printing the current device's
  `name`, `pageableMemoryAccess`, `pageableMemoryAccessUsesHostPageTables` and `integrated`
  (`cudaGetDeviceProperties`), built on dev in the 25.02 build image with `nvcc` (cudart static,
  nvcc's default, so it runs on the host with only the driver).

**Interfaces:**
- Consumes: Task 1's `loads.tsv`; new-hosts' `run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu prepare|ship|run|pull`, and
  `create_nsys_profile.sh` as new-hosts made it run on nebius-h100 (its build-test.md section names
  the exact `--host`/`--remote-*` arguments).
- Produces: `DISABLED_ReaderProbe.ReadParquetAgainstOneChunk`, reading `PEACOCK_PROBE_FILE` (one
  parquet path), `PEACOCK_PROBE_COLUMNS` (comma list), `PEACOCK_PROBE_RUNS` (default 10). It reads
  the file once into a `cudaHostAlloc` buffer, then times four reads of the same columns and every
  row group: {`source_info{path}`, `source_info` over the pinned span} × {`read_parquet`,
  `chunked_parquet_reader` with limit 0}. One stdout line each:
  `[reader-probe] source=<path|pinned> reader=<read_parquet|chunked> chunks=<n> rows=<n> us=<2nd-min> runs=[…]`.
  It asserts the four row and column counts agree.
- Produces: the detail file's **Discrete cards** (per card: name, the four values, discrete or
  not; Task 6's gtest reads that list), **Host memory** (`free -b` total on the H100; the run
  container's cgroup limit, `cat /sys/fs/cgroup/memory.max` inside `run-gpu.sh`'s `docker run`;
  the bytes of `tpch.sf40` and `tpcds.sf40` parquet, `du -cb`; the default cache, min(total,
  limit) × 3/4; whether each dataset fits), and **Decision**: exactly one of `CONFIGURE_POOL`, `RAISE_THRESHOLD`, or
  `NULL_RESULT`, for cuDF's pool (Task 6), with the numbers that chose it.

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
  - **The host-buffer source** (`datasource.cpp`, `host_buffer_source` and `datasource::create(host_span<std::byte const>)`;
    `cudf/io/types.hpp`, the single-span `source_info(cudf::host_span<std::byte const>)`, which a
    load uses since it has one file; the multi-span form in 25.02 is the template
    `source_info(host_span<host_span<T>>)`, not a `std::vector` overload). Expected:
    `host_buffer_source::supports_device_read()` is false in 25.02, so the reader `host_read`s a
    non-owning view and issues one `cudaMemcpyAsync(cudaMemcpyDefault)` per coalesced chunk run,
    syncing in `count_page_headers`. A lease held across `read_scan_table` therefore covers the
    async copy. Record that, and whether cuDF registers or copies the span.
  - **The chunked reader** (`cudf/io/parquet.hpp`, `chunked_parquet_reader(std::size_t chunk_read_limit,
    parquet_reader_options const&, …)`, `has_next()`, `read_chunk()`; the same in 26.02): that a
    `chunk_read_limit` of 0 means no limit, one chunk, and what `pass_read_limit` defaults to.

  The expected conclusion is that the bulk H2D is already pinned, but in 16 MiB pieces, from kvikio
  threads, after a CPU copy out of the page cache. The spec's API `cudf::io::set_host_memory_resource`
  does not exist in 25.02 or 26.02. Its successors are `cudf::set_pinned_memory_resource` and
  `cudf::config_default_pinned_memory_resource` in `cudf/utilities/pinned_memory.hpp`. Record that.

- [ ] **Step 2: the reader probe, offline.** Write `test_reader_probe.cpp` (Interfaces). Without
  `PEACOCK_PROBE_FILE` it `GTEST_SKIP()`s naming the variable. Build in the 25.02 and the 26.02
  build images (`scripts/build.sh` as build-test.md documents): both compile. Report a checkpoint
  `pinned-loads: a reader probe, read_parquet against one chunk, from a path and from pinned memory`.

- [ ] **Step 3: the L40S's attribute, then build, start and ship today's engine** to nebius-h100
  (cuDF 25.02 run image). First build the `devprops` probe on dev. **The L40S, before any engine
  change** (spec item 1): one short bracket as `prompts.md` says: start nebius-gpu, rsync
  `devprops` to it, run it and `nvidia-smi -q | grep -i 'Addressing Mode'`, stop it. Write its
  four values and addressing mode into "Discrete cards" (`pageableMemoryAccess = 1` → stop for
  the human, Global constraints). Then the CPU-VM build (Global constraints) into this tree's
  `cpp/install/`. Only then
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu start` and
  `scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu ship` (default `--install cpp/install`), as new-hosts documents. Pass: `ship` exits 0.
  **Discrete cards, before anything else on the VM:** rsync `devprops` to the H100 over
  `-e "<nebius.sh options> rsh"`, run it there (`$N ssh ./devprops`), and
  `$N ssh "nvidia-smi -q | grep -i 'Addressing Mode'"`. Write the four values and the addressing
  mode into "Discrete cards". A discrete card with `pageableMemoryAccess = 1` → stop the H100 and
  stop for the human (Global constraints).
  **Host memory:** `$N ssh 'free -b'`, `$N ssh 'du -cb <the sf40 data dirs>/*.parquet'` (the
  paths new-hosts' fetch wrote), and the run container's limit (`cat /sys/fs/cgroup/memory.max`,
  else `/sys/fs/cgroup/memory/memory.limit_in_bytes`, in the image `run-gpu.sh` runs, with its
  `docker run` options) into "Host memory".

- [ ] **Step 4: before load times (unprofiled).** Run this for each of `q1 q3 q6 q8`:
  `PCK_TEST_FILTER=bench_tpch_sf40_<q>_tp1_single scripts/env/run-gpu.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu run bench`, then
  `pull`, then remove the host record (Global constraints). Pass: each run exits 0 and
  `testdata/benchmark-results/h100-80gb-hbm3/tpch.sf40/tp1_single.benchmark.txt` has the section.
  Copy every `GpuLoadParquet` line's `table=`, projections and `total_us` into "Before load times",
  then move the file aside (`…/tp1_single.before.txt` in the scratchpad). Do not commit the results
  tree from this run. sf40-baseline owns it.

- [ ] **Step 5: before profile.** For each `<q>` in `q1 q3 q6 q8`, run
  `PCK_TEST_FILTER=bench_tpch_sf40_<q>_tp1_single PCK_BENCH_HBM_SET=gh100 PCK_BENCH_HBM_PEAK_BW=3.35e12
  scripts/create_nsys_profile.sh --trace <new-hosts' H100 arguments>`. Then
  `cp testdata/calibration/loads.tsv testdata/calibration/loads-<q>-before.tsv`, copy the pulled
  capture (`testdata/$CAPTURE_REL.sqlite`) to the scratchpad as `capture-<q>-before.sqlite`, and
  `git show HEAD:testdata/calibration/calls.tsv > testdata/calibration/calls.tsv` (a read, no git
  write); the committed calls.tsv is the H200's.

  Pass: each run prints `the load breakdown: N rows` with N ≥ 1, and the `recipes:` line matches the
  plan. After `q1`, read its `host_read_source`. If it is `none` (the trace pass records no `osrt`),
  stop the VM; add `osrt` to the trace pass's `-t` list in `create_nsys_profile.sh` (`bash -n`,
  shellcheck, the Task 1 suite); start again and rerun Step 5 from `q1`. Then check the kernel names. List the distinct kernel names in the load windows, with a
  one-off `sqlite3` query in the detail file. Every decompression and decode kernel must be caught by
  `DECOMPRESS_RE`/`DECODE_RE`. If one is not, fix the pattern offline, with the H100 stopped: in
  Task 1's module with a test case named for it, then re-derive each `loads-<q>-before.tsv` from
  its `capture-<q>-before.sqlite` (`nsys_loads.py --capture … --out …`). Never re-profile for it
  (VM-time rule).

- [ ] **Step 6: #232's re-measure, from pinned memory, before any engine change.** Run the probe
  on the H100 for sf40 `lineitem` twice: q6's columns (`l_shipdate,l_discount,l_quantity,l_extendedprice`)
  and q1's (`l_quantity,l_extendedprice,l_discount,l_tax,l_returnflag,l_linestatus,l_shipdate`),
  with `GTEST_ALSO_RUN_DISABLED_TESTS=1 GTEST_FILTER='DISABLED_ReaderProbe.*'` and the three
  `PEACOCK_PROBE_*` variables in the run's environment, through `run-gpu.sh run` the way
  build-test.md "Nebius host" passes `PCK_TEST_FILTER`; if `run` cannot run one gtest binary
  alone, `$N ssh` the `docker run` line `run-gpu.sh` prints, with `peacock_gpu_tests` and those
  variables. Copy the eight `[reader-probe]` lines into "Readers from pinned memory (#232)", with
  one sentence: does one chunk beat `read_parquet` on the H100, from the path and from pinned
  memory, and by how much. Then run the q6 probe once more under `nsys profile -t cuda,nvtx`
  (the spec's 0b asks for the re-measure "in the before profile"; #232 asks for NVTX), pull the
  report to the scratchpad, and write its H2D and kernel totals per reader and source beside the
  lines. Then stop the H100:
  `scripts/nebius.sh --instance-id computeinstance-e00e6yena4nzjgaym0 --profile editor-eu stop`.

- [ ] **Step 7: write the breakdown into the detail file.** Give one table per query, one row per
  load, with every `loads.tsv` column. Add the sentence the spec asks for: the copies run at the
  pageable rate (~10 GB/s) or the pinned rate (~55 GB/s), as read from `h2d_pinned_gbps` and
  `h2d_pageable_gbps`. Also give `h2d_max_bytes`, and the copy's share of `load_us`:
  `h2d_us / load_us`, and `h2d_pageable_bytes/10 GB/s − h2d_pageable_bytes/55 GB/s` as the most
  pinning could save.

- [ ] **Step 8: decide cuDF's pool.** Write the Decision, using the four loads' median:
  - `RAISE_THRESHOLD` if `h2d_pageable_bytes ≥ 10%` of `h2d_bytes`, **and** Step 1 traced those
    copies to `make_host_vector` (pageable by threshold). If they come from the datasource path
    instead, the cache (Tasks 3-4) is the fix for them: choose by the next rule.
  - Otherwise `CONFIGURE_POOL` if `pinned_alloc_us ≥ 5%` of `load_us`. This is cuDF's 100 MiB pool
    overflowing into a `cudaHostAlloc` per buffer.
  - Otherwise `NULL_RESULT`. cuDF already pins what it copies; the cache and the chunked reader
    still go in (spec items 0a, 0b), and #149's pool part closes as misdiagnosed.


- [ ] **Step 9: Commit** `pinned-loads: cuDF 25.02's IO host memory, the H100 before profile, #232 re-measured`
  (the detail file).

### Task 3: the pinned host file cache, alone (spec item 0a)

**Files:**
- Create: `cpp/src/peacock/pinned_host_cache.h`, `cpp/src/pinned_host_cache.cpp`
- Modify: `cpp/CMakeLists.txt`: `src/pinned_host_cache.cpp` in `peacock_gpu` (lines 148-169);
  `tests/cpu/test_pinned_host_cache.cpp` in `peacock_cpu_tests` (line 190), which gains
  `${CMAKE_CURRENT_SOURCE_DIR}/src` in its include directories
- Create: `cpp/tests/cpu/test_pinned_host_cache.cpp` (no GPU: a counting `malloc` allocator)
- Modify: `cpp/tests/gpu/test_cudf.cpp`: `PinnedHostCache.CudaAllocatorPinsHostMemory` and
  `PinnedHostCache.RefusedAllocationClearsTheCudaError`

**Interfaces:**
- Produces (Task 4 wires it; Task 6 reuses the predicate):

```cpp
namespace peacock {
inline constexpr std::size_t kPinnedHostCacheRamNumerator = 3, kPinnedHostCacheRamDenominator = 4;  // default = min(RAM, cgroup) × 3/4
bool is_discrete_for_pinning(int pageable_memory_access);     // pure: == 0
// min(host_ram, cgroup_limit when set) / divisor
std::size_t default_pinned_host_cache_bytes(std::size_t host_ram_bytes,
                                            std::optional<std::size_t> cgroup_limit_bytes);
// A cgroup memory file's text: "max" (v2, no limit) → nullopt; a decimal → its bytes; else nullopt.
std::optional<std::size_t> parse_cgroup_memory_limit(std::string_view text);
// /sys/fs/cgroup/memory.max (v2), else /sys/fs/cgroup/memory/memory.limit_in_bytes (v1);
// unreadable → nullopt. A v1 "unlimited" (a value above host RAM) is harmless: min() drops it.
std::optional<std::size_t> cgroup_memory_limit_bytes();

struct HostAllocator {                         // nullptr from allocate = refused, never a throw
  void* (*allocate)(std::size_t bytes);
  void (*deallocate)(void* p, std::size_t bytes);
};
// cudaHostAlloc(cudaHostAllocDefault) / cudaFreeHost. A refused cudaHostAlloc returns nullptr
// after cudaGetLastError(), so no later cuDF check (cudaPeekAtLastError) sees the error.
HostAllocator cuda_pinned_allocator();

struct FileReader {                            // the fill's read; tests inject short reads
  ssize_t (*pread)(int fd, void* buf, std::size_t count, off_t offset);
};
FileReader posix_file_reader();                // ::pread
// Reads exactly `size` bytes from offset 0, looping: a short read goes on, EINTR is retried.
// EOF before `size` → false, errno ESTALE (the file changed); a -1 → false, its errno.
bool read_whole_file(int fd, std::byte* dst, std::size_t size, FileReader reader);

enum class DeviceKind { NoDevice, Discrete, Integrated };
// !query_ok or device_count <= 0 → NoDevice; else is_discrete_for_pinning(pageable_memory_access)
DeviceKind classify_device(bool query_ok, int device_count, int pageable_memory_access);

struct PinnedCacheStats {
  std::size_t capacity_bytes = 0, resident_bytes = 0, files = 0;
  std::size_t fills = 0, fill_bytes = 0, fallbacks = 0, evictions = 0, reloads = 0;
  std::uint64_t fill_us = 0;                   // host IO into pinned memory, wall clock, summed
};

struct PrefillOutcome {
  enum class Status { Ok, DoesNotFit, Io, Off };
  Status status = Status::Ok;
  std::uint64_t files = 0, bytes = 0, us = 0;  // read from disk by THIS call (0 on a repeat); wall µs
  std::uint64_t needed = 0;                    // Σ st_size of the distinct named paths
  std::uint64_t limit = 0;                     // capacity − bytes leased outside the named set
  std::size_t failed_index = 0;                // Io: which path
  int error = 0;                               // Io: errno (ENOMEM when the allocator refused)
};

class PinnedHostCache {
 public:
  PinnedHostCache(std::size_t capacity_bytes, HostAllocator allocator,
                  FileReader reader = posix_file_reader());
  class Lease {                                // move-only; its files stay resident while it lives
   public:
    bool cached() const;                       // false: read the paths, every one of them
    std::vector<std::span<const std::byte>> const& spans() const;  // one per path, in order
  };
  Lease acquire(std::vector<std::string> const& paths);
  PrefillOutcome prefill(std::vector<std::string> const& paths);  // between calls only
  void set_capacity(std::size_t bytes);        // evicts unleased entries, LRU first, down to it
  PinnedCacheStats stats() const;
};
}
```

  `prefill`'s rules: it `stat`s every distinct path first (a failure → `Io`, nothing allocated,
  nothing evicted); capacity 0 → `Off`, nothing touched; `needed > limit` → `DoesNotFit` with both
  set, **before any allocation and with no eviction**; otherwise it evicts unleased entries outside
  the named set, LRU first, until `needed` fits, then loads every named file that is missing or
  changed (size or mtime: a reload, `reloads` counts it). A file resident and unchanged costs
  nothing, so a second identical call returns `Ok` with `files == 0`, `bytes == 0`. An allocation
  refused mid-way is `Io` with `ENOMEM`, and a read that ends before `st_size` is `Io` with
  `ESTALE`; files this call already loaded stay (a retry costs only the rest). Its loads count in `fills`, `fill_bytes` and `fill_us` like any fill. No fallback is
  ever counted by `prefill`, and an off cache (`capacity 0`) counts no fallback in `acquire`
  either.

  Rules the tests pin: an entry is a whole file, keyed by path and validated by `(st_size,
  st_mtim)` on every `acquire`. A changed file is reloaded; the stale entry leaves the map and
  is freed when its last lease ends. Eviction (LRU by last `acquire`) happens only inside
  `acquire`, `prefill` and `set_capacity`, and at a lease's release while `resident_bytes` is
  over capacity; it never frees a leased entry: if the unleased entries cannot make room, that
  file falls back. The spec's "only between calls" is met by leases, not by timing: `acquire` may
  evict unleased entries while another query runs, but no read's buffer is freed under it.
  **The hard ceiling:** `resident_bytes` counts every buffer still allocated, a stale entry still
  leased and an in-flight fill included. A fill reserves its bytes under the lock before it reads,
  so two fills never both see room and pass the capacity together. A lease is all-or-nothing: if any path cannot be resident
  (does not fit, the allocator refuses, `stat` fails), the lease holds none and `cached()` is
  false, so a scan never mixes sources. A fallback prints once per path per process:
  `[pinned-cache] <path> (<size>) not cached: <does not fit in <cap>|pinned allocation refused>; read from its path`.
  A fill never holds the cache's lock while it reads the file; a second `acquire` of a file
  being filled waits for that fill rather than filling again. **A failed fill** (the allocator
  refused, the read failed or came up short) releases its reservation, and every waiter wakes
  with that error: its lease falls back (`cached() == false`) and counts a fallback. `fill_us`
  times the read into the pinned buffer (allocation included).

- [ ] **Step 1: failing tests**, in `test_pinned_host_cache.cpp`, over temp files written by the
  test (1 KiB = 1024 B each unless said) and a counting allocator that poisons a buffer
  (`0xDD`) before freeing it:
  - `PinnedHostCache.HitsReuseTheBuffer`: two `acquire({A})` → the same `spans()[0].data()`, its
    bytes equal the file's, `stats().fills == 1`.
  - `PinnedHostCache.LeastRecentlyUsedIsEvicted`: capacity 3 KiB; acquire A, B, C (each lease
    dropped), A again, then D → `evictions == 1`, B's buffer freed, A's pointer unchanged.
  - `PinnedHostCache.LeasedEntryIsNeverEvicted`: capacity 2 KiB; hold a lease on A; acquire B
    (dropped), then C → B is evicted, A is not, A's bytes still equal the file's.
  - `PinnedHostCache.ConcurrentReadAndEviction`: capacity 2 KiB; thread 1 holds a lease on A and
    `memcmp`s its span against the file 1000 times; thread 2 acquires and drops B, C, D, E in a
    loop 1000 times. Every `memcmp` is 0; A is never freed while leased (the allocator records
    it); afterwards `resident_bytes <= 2 KiB`.
  - `PinnedHostCache.SecondReaderWaitsForTheFill`: an allocator that blocks on a latch; two
    threads acquire A at once; release the latch → both see the same pointer, `fills == 1`.
  - `PinnedHostCache.AFailedFillWakesItsWaiters`: a latch allocator that returns `nullptr` once
    released; four threads acquire A at once (one fills, three wait); release → all four
    `cached() == false`, `fallbacks == 4`, `resident_bytes == 0`; the threads are joined through
    `std::future::wait_for(10s)`, so a hang fails the case rather than the run.
  - `PinnedHostCache.ConcurrentFillsStayUnderCapacity`: capacity 2 KiB; a latch allocator that
    records the peak of bytes outstanding; three threads acquire distinct 1 KiB files at once;
    release → at most two `cached()`, the allocator's peak ≤ 2048, `resident_bytes <= 2048`.
  - `PinnedHostCache.ReleaseEvictsWhileOverCapacity`: capacity 2 KiB; lease A; rewrite A (new
    mtime); lease A again (new bytes) → `resident_bytes == 2048` (the stale copy counts); acquire
    B → falls back; drop the old lease → `resident_bytes == 1024`. Then leases on A and C,
    `set_capacity(1 KiB)` → nothing freed yet; drop C's lease → C freed, `resident_bytes <= 1024`.
  - `PinnedHostCache.ShortReadsAreLooped`: a `FileReader` that returns at most 100 bytes per call
    and `EINTR` on its second call → `acquire` of a 1 KiB and of a 4 KiB file is `cached()`, each
    span equal to the file's bytes.
  - `PinnedHostCache.ShortReadAtEofIsAChangedFile`: a reader that returns 0 after 512 bytes →
    `acquire({A})` is `cached() == false`, `fallbacks == 1`, its buffer freed; `prefill({A})` →
    `Io`, `error == ESTALE`.
  - `PinnedHostCache.NoDeviceIsOff`: `classify_device(false, 0, 0) == NoDevice`,
    `(true, 0, 0) == NoDevice`, `(true, 1, 0) == Discrete`, `(true, 1, 1) == Integrated`.
  - `PinnedHostCache.ChangedFileIsReloaded`: acquire A (dropped); rewrite A at the same size with
    other bytes and set its mtime 1 s later (`utimensat`) → `reloads == 1`, the new bytes; then a
    rewrite at another size → `reloads == 2`.
  - `PinnedHostCache.ChangedFileWhileLeasedKeepsTheOldBytes`: lease L1 on A; rewrite A; L2 =
    acquire A → L2 sees the new bytes, L1 still the old; drop both → `resident_bytes == ` the new size.
  - `PinnedHostCache.ScanThatDoesNotFitReadsEveryPath`: capacity 3 KiB; `acquire({A, Big})` with
    Big 4 KiB → `cached() == false`, `spans().empty()`, `fallbacks == 1`, A not left resident by it.
  - `PinnedHostCache.AllocationFailureFallsBack`: an allocator returning `nullptr` → `cached() ==
    false`, `fallbacks == 1`, no exception.
  - `PinnedHostCache.FallbackIsSaidOnce`: `testing::internal::CaptureStderr`; two acquires of Big →
    one `[pinned-cache]` line naming Big's path.
  - `PinnedHostCache.MissingFileIsLeftToTheReader`: acquire of a path that does not exist →
    `cached() == false`, no throw (cuDF's own read reports it).
  - `PinnedHostCache.ZeroCapacityIsOff`: capacity 0 → never calls `allocate`, `cached() == false`,
    `fallbacks == 0`, and no `[pinned-cache]` line.
  - `PinnedHostCache.SetCapacityEvictsTheUnleased`: A leased, B not; `set_capacity(1 KiB)` → B
    freed, A kept, `capacity_bytes == 1024`.
  - `PinnedHostCache.DefaultIsThreeQuartersOfTheSmallerOfRamAndCgroup`:
    `default_pinned_host_cache_bytes(196'000'000'000, std::nullopt) == 147'000'000'000`;
    `(196'000'000'000, 64'000'000'000) == 48'000'000'000`; `(196'000'000'000, 400'000'000'000) ==
    147'000'000'000`; `is_discrete_for_pinning(0)` true, `(1)` false. This covers the GB10 branch
    without a GB10.
  - `PinnedHostCache.CgroupLimitIsParsed`: `parse_cgroup_memory_limit("max\n") == nullopt`,
    `("8589934592\n") == 8589934592`, `("") == nullopt`, `("lots") == nullopt`.
  - The prefill (capacity 4 KiB unless said):
    - `PinnedHostCache.PrefillMakesEveryFileResident`: `prefill({A, B})` → `Ok`, `files == 2`,
      `bytes == 2048`, `needed == 2048`, `limit == 4096`, `us > 0`; then `acquire({A, B})` is
      `cached()` with the file's bytes and `fills` does not move.
    - `PinnedHostCache.PrefillIsIdempotent`: `prefill({A, B})` twice, then `prefill({B, A, A})` →
      the second and third `Ok` with `files == 0`, `bytes == 0`, `needed == 2048`; the
      allocator's call count is 2 throughout.
    - `PinnedHostCache.PrefillRefusesBeforePinningAnything`: capacity 3 KiB, C resident (acquired,
      dropped); `prefill({A, Big})` with Big 4 KiB → `DoesNotFit`, `needed == 5120`,
      `limit == 3072`; the allocator was not called by it, C is still resident, `evictions == 0`.
    - `PinnedHostCache.PrefillCountsLeasesAgainstTheLimit`: capacity 2 KiB; leases held on A and
      B; `prefill({C})` → `DoesNotFit`, `needed == 1024`, `limit == 0`, A and B untouched.
    - `PinnedHostCache.PrefillReloadsAChangedFile`: `prefill({A})`; rewrite A at the same size
      with other bytes, mtime 1 s later → `prefill({A})` is `Ok`, `files == 1`, `reloads == 1`,
      `acquire({A})` sees the new bytes.
    - `PinnedHostCache.PrefillEvictsBetweenCalls`: capacity 2 KiB; `prefill({A, B})`, then
      `acquire({B})` (dropped), then `prefill({C})` → `Ok`, `evictions == 1`, A's buffer freed
      (the allocator recorded it), B and C resident.
    - `PinnedHostCache.PrefillOfAMissingFileIsIo`: `prefill({A, Missing})` → `Io`,
      `failed_index == 1`, `error == ENOENT`, the allocator never called.
    - `PinnedHostCache.PrefillAllocationRefusedIsIo`: an allocator that refuses its second call;
      `prefill({A, B})` → `Io`, `error == ENOMEM`, `failed_index == 1`, A resident.
    - `PinnedHostCache.PrefillWhenOffIsOff`: capacity 0 → `Off`, the allocator never called.

  In `test_cudf.cpp`: `PinnedHostCache.CudaAllocatorPinsHostMemory`: a 1 MiB
  `cuda_pinned_allocator().allocate` has `cudaPointerGetAttributes(...).type == cudaMemoryTypeHost`.
  `PinnedHostCache.RefusedAllocationClearsTheCudaError`: `cuda_pinned_allocator().allocate(1ull <<
  50) == nullptr`, then `cudaPeekAtLastError() == cudaSuccess`, then a `cudf::io::read_parquet`
  of tpch.minimal's `customer.parquet` succeeds.

- [ ] **Step 2: build; expect FAIL to compile** (`pinned_host_cache.h` missing), in the 25.02 build
  image.

- [ ] **Step 3: implement** `pinned_host_cache.h`/`.cpp`: a `std::mutex` over
  `std::unordered_map<std::string, std::shared_ptr<Entry>>` plus an LRU list; a lease holds
  `shared_ptr<Entry>`s, and an entry's buffer is returned to the allocator in its destructor.
  Files are read with `open` and `read_whole_file` (a `pread` loop until `st_size` bytes are in:
  Linux returns at most 0x7ffff000 bytes per call). An entry being filled carries a condition
  variable and its outcome; waiters wait on it and read the outcome. `prefill` takes the lock for
  its `stat`s, its size check, its evictions and its reservations, then fills outside it as
  `acquire` does; `us` is `std::chrono::steady_clock` around the whole call.
  `cgroup_memory_limit_bytes` reads the root files only: inside a cgroup namespace (`run-gpu.sh`'s
  `docker run`) that is the container's own limit. Outside one (on a host under a slice, or
  `--cgroupns=host`) the limit sits at `/proc/self/cgroup`'s path and is not read; Task 8 notes it.

- [ ] **Step 4: run** `ctest -L cpu` in the 25.02 build image → every `PinnedHostCache.*` CPU case
  passes, and the existing CPU cases still do. Build in the 26.02 image too: it compiles. The GPU
  case runs in Task 7.

- [ ] **Step 5: Commit** `pinned-loads: a pinned host file cache, LRU between calls, reloaded on change, prefilled whole`.
  Run `git clang-format` on the changed lines first.

### Task 4: the scan reads the cache; `pinned_host_cache_bytes` and the prefill through Rust and the C ABI

**Files:**
- Modify: `cpp/src/pinned_host_cache.cpp`: the process singleton
- Modify: `cpp/src/operators/scan.cpp:92-99`: lease the files, read `source_info` over the spans
- Modify: `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`: three symbols (beside
  `peacock_install_rmm_pool`)
- Modify: `peacockdb-ffi/src/lib.rs` (`raw`), `peacockdb-ffi/tests/test_ffi.rs` (the no-device case only)
- Create: `peacockdb-core/src/options.rs` (`pub mod options;` in `lib.rs`), `peacockdb-core/src/options/tests.rs`
- Modify: `peacockdb-core/src/lib.rs` `build_session_state`: register the extension
- Create: `peacockdb-core/src/executor/pinned_cache.rs` (no feature gate), with `tests.rs`
  (rust-only) and `gpu_tests.rs` (`#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`, the lib's
  device rung: `gpu_tests::` with `--test-threads=1`)
- Modify: `peacockdb-core/src/executor/instrument.rs`: the FFI calls (beside `install_rmm_pool`)
- Modify: `peacockdb-core/src/executor/mod.rs`: `GpuContext::prefill_pinned_cache`, and the
  one-expression delegates
- Modify: `peacockdb-core/src/test_support/gpu_session.rs`: `Session::prefill_pinned_cache`
  (one expression, over the same implementation)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`: `PinnedHostCache.ScanFromTheCacheEqualsThePath`,
  `PinnedHostCache.PrefilledScanFillsNothing`

**Interfaces:**
- Consumes: Task 3's `PinnedHostCache` (`acquire`, `prefill`, `PrefillOutcome`),
  `is_discrete_for_pinning`, `default_pinned_host_cache_bytes`, `cgroup_memory_limit_bytes`,
  `cuda_pinned_allocator`.
- Produces, C++: `PinnedHostCache& pinned_host_cache();` (first use: `cudaGetDeviceCount` and the
  device's `pageableMemoryAccess`, through `classify_device`; no device or a failed query →
  capacity 0, state no-device, the CUDA error cleared with `cudaGetLastError()`; integrated →
  capacity 0, state integrated; else the capacity from
  `PEACOCK_PINNED_HOST_CACHE_BYTES` (decimal bytes; anything else throws at the first scan or
  prefill, naming the variable), else the value set through the ABI, else
  `default_pinned_host_cache_bytes(sysconf(_SC_PHYS_PAGES) * sysconf(_SC_PAGESIZE),
  cgroup_memory_limit_bytes())`). Process-wide: every executor in the process shares it, and a
  later `peacock_set_pinned_host_cache_bytes` replaces an earlier one (the last set wins). The
  singleton is leaked on purpose (`static auto* cache = new PinnedHostCache(…)`): no static
  destructor unpins every buffer at exit or races a thread still holding a lease. One
  stderr line at first use: `[pinned-cache] 136.9 GiB of 182.5 GiB host RAM (default) on a discrete device`
  (with a cgroup limit: `… of 64.0 GiB cgroup limit (default) …`) /
  `[pinned-cache] off (pinned_host_cache_bytes = 0, env)` / `[pinned-cache] off: integrated device (pageable memory access)` /
  `[pinned-cache] off: no CUDA device`.
  Each of the three ABI entries catches: a `PEACOCK_PINNED_HOST_CACHE_BYTES` parse error thrown
  by the first-use initializer returns `-1` from `set`/`stats` (the message on stderr) and
  `PEACOCK_PREFILL_BAD_ARGUMENT` from the prefill (the message in `peacock_last_error`); nothing
  crosses `extern "C"`.
- Produces, C ABI (`peacock_gpu.h`, additive). The prefill is the spec's text; the other two are
  kept for the reasons Global constraints gives (the option's door, and the counters):

```c
enum { PEACOCK_PINNED_CACHE_ON = 0, PEACOCK_PINNED_CACHE_OFF = 1, PEACOCK_PINNED_CACHE_INTEGRATED = 2,
       PEACOCK_PINNED_CACHE_NO_DEVICE = 3 };
enum { PEACOCK_PINNED_CACHE_FROM_DEFAULT = 0, PEACOCK_PINNED_CACHE_FROM_SESSION = 1,
       PEACOCK_PINNED_CACHE_FROM_ENV = 2 };
#define PEACOCK_PINNED_CACHE_DEFAULT_BYTES UINT64_MAX   /* "the engine's default" */
typedef struct PeacockPinnedCacheStats {
  int32_t state;  int32_t source;
  uint64_t capacity_bytes, resident_bytes, files, fills, fill_bytes, fill_us,
           fallbacks, evictions, reloads;
} PeacockPinnedCacheStats;
/// Process-wide, like the rmm pool: the cache serves every executor in the process, and the
/// last set wins. Shrinking evicts unleased files now. PEACOCK_PINNED_HOST_CACHE_BYTES, when
/// set, wins over every set (source = ENV).
/// @return 0 unless out is NULL.
int peacock_set_pinned_host_cache_bytes(uint64_t bytes, PeacockPinnedCacheStats* out);
int peacock_pinned_host_cache_stats(PeacockPinnedCacheStats* out);

enum { PEACOCK_PREFILL_OK = 0, PEACOCK_PREFILL_DOES_NOT_FIT = 1, PEACOCK_PREFILL_IO = 2,
       PEACOCK_PREFILL_OFF = 3, PEACOCK_PREFILL_BAD_ARGUMENT = -1 };
typedef struct { uint64_t files, bytes, us; uint64_t needed, limit; } peacock_prefill_stats_t;
/// Make every named file resident in the process's pinned host cache, whole, from disk. A file
/// resident and unchanged costs nothing: the call is idempotent (files = bytes = 0 on a repeat).
/// Refuses before pinning anything when the total exceeds the cache's limit, with needed and
/// limit set. The executor only carries the error: on PEACOCK_PREFILL_IO, out->files is the
/// index into paths of the file that failed and peacock_last_error(executor) reads
/// "<path>: <strerror>". NULL executor, out, or paths with n_paths > 0 → BAD_ARGUMENT.
int peacock_pinned_cache_prefill(peacock_executor_t* executor,
                                 const char* const* paths, size_t n_paths,
                                 peacock_prefill_stats_t* out);
```

- Produces, Rust:
  - `peacockdb_core::options::PeacockOptions`, an `extensions_options!` struct with
    `pub pinned_host_cache_bytes: Option<u64>, default = None`, `ConfigExtension::PREFIX = "peacock"`;
    `build_session_state` adds it (`SessionConfig::with_option_extension`), so
    `SET peacock.pinned_host_cache_bytes = 0` works in any session.
  - `executor/pinned_cache.rs` (no feature gate):
    - `pub(crate) struct PinnedCache { pub state: PinnedCacheState, pub source: PinnedCacheSource,
      pub capacity_bytes: u64, pub resident_bytes: u64, pub files: u64, pub fills: u64,
      pub fill_bytes: u64, pub fill_us: u64, pub fallbacks: u64, pub evictions: u64,
      pub reloads: u64 }`, `enum PinnedCacheState { On, Off, Integrated, NoDevice }`,
      `enum PinnedCacheSource { Default, Session, Env }`, each `#[derive(Default)]` (`Off`,
      `Default`; the struct's counters 0) so tests write `..Default::default()`, and `impl
      Display`, one token run with no newline (bench-rows' `pinned_cache` value):
      `on cap=136.9GiB resident=12.3GiB files=8 fills=8 fill_us=812345 fallbacks=0 evictions=0 reloads=0 source=default`,
      `off source=env`, `off integrated`, `off no-device`. GiB with one decimal, as `RmmPool`'s
      Display prints.
    - `pub(crate) struct PrefillStats { pub files: u64, pub bytes: u64, pub us: u64, pub needed: u64,
      pub limit: u64 }` (`Debug, Clone, Copy, PartialEq, Eq, Default`).
    - `pub(crate) enum PrefillError { DoesNotFit { needed: u64, limit: u64 }, Io { path: PathBuf,
      error: String }, Off }`, `impl Display`: `needs <needed> > limit <limit>`,
      `<path>: <error>`, `the pinned host cache is off`. The benchmark's reason is
      `prefill: {error}`, so a refusal reads `prefill: needs <needed> > limit <limit>`.
    - `pub(crate) fn plan_files(roots: &[&dyn GpuNode]) -> BTreeSet<PathBuf>`: every
      `GpuLoadParquet`'s `file`, walking `children()` from each root and reading a load through
      `as_any().downcast_ref::<GpuLoadParquet>()`. A tree is one root; a fused DAG is its roots
      (dag-driver's `SharedSource` answers `as_any` with its inner load and has no children, so
      a shared load is found once per parent and the set keeps it once). A `rows_only()` load is
      skipped: `execute_scan` answers it from the footer through `rows_of` (scan.cpp:71) before
      any lease, so its pinned file would be read by nothing.
  - `instrument.rs` (the GPU build only): `pub(crate) fn apply_pinned_host_cache(config: &SessionConfig) -> PinnedCache`
    (unset → `PEACOCK_PINNED_CACHE_DEFAULT_BYTES`), `pub(crate) fn pinned_host_cache() -> PinnedCache`,
    and `pub(crate) fn prefill_on(executor: *mut PeacockExecutor, files: &[PathBuf]) ->
    Result<PrefillStats, PrefillError>` (paths as `CString`s; a path that is not UTF-8 or holds a
    NUL is `Io` with that reason, before the call), with one-expression delegates in
    `executor/mod.rs` as `install_rmm_pool` has.
  - `GpuContext::prefill_pinned_cache(&self, files: &[PathBuf]) -> Result<PrefillStats, PrefillError>`
    and `Session::prefill_pinned_cache(&self, files: &[PathBuf]) -> Result<PrefillStats, PrefillError>`,
    each `prefill_on(self.executor, files)`.

- [ ] **Step 1: failing tests.**
  - `options/tests.rs` (rust-only): `the_cache_option_is_unset_by_default` (`build_session_state(1)`'s
    extension reads `None`); `set_reads_back` (`ctx.sql("SET peacock.pinned_host_cache_bytes = 0")`
    → `Some(0)`); `a_non_number_is_refused` (`= 'lots'` → `Err` naming the key).
  - `executor/pinned_cache/tests.rs` (rust-only):
    - `pinned_cache_reads_as_one_trailer_value`: the three Display forms above from literal
      structs, and `!s.contains('\n')`.
    - `a_refusal_reads_as_its_sizes`: `PrefillError::DoesNotFit { needed: 160_000_000_000, limit:
      147_000_000_000 }.to_string() == "needs 160000000000 > limit 147000000000"`.
    - `plan_files_of_q3_names_its_three_tables`: `plan_at("tpch", "1", "q3", tp1_single)` (sf1, as
      dag-driver plans `("tpcds", "1", "q95")`; tpch.minimal has no `lineitem` or `orders`, and the
      rust-only `--lib` rung already needs sf1) → exactly the `lineitem`, `orders` and `customer`
      files, as the plan's `GpuLoadParquet` nodes name them.
    - `plan_files_over_roots_is_the_union`: sf1 roots `[q3, q6, q3]` → q3's set ∪ q6's
      (`lineitem` once).
    - `plan_files_skips_a_rows_only_load`: a `GpuLoadParquet` over `customer` with an empty
      projection (`rows_only()`) → `plan_files` is empty.
  - `executor/pinned_cache/gpu_tests.rs` (the lib's device rung, `--test-threads=1`), over the Rust
    wrappers. Each first reads `PEACOCK_PINNED_HOST_CACHE_BYTES`: when it is set (Task 7 Step 6's
    off run), it asserts `source == Env` and `Off`/`Err(Off)` and stops there.
    - `the_session_option_reaches_the_cache`: a session after `SET peacock.pinned_host_cache_bytes
      = 0` → `apply_pinned_host_cache(&ctx.state().config())` is `Off`, `source == Session`,
      `capacity_bytes == 0`; a fresh session (unset) → `On`, `source == Default`,
      `capacity_bytes > 0`.
    - `prefill_reports_its_files_and_refuses_what_does_not_fit`: two temp files of 1 MiB;
      `prefill_on` both → `Ok`, `files == 2`, `bytes == 2 MiB`, `needed == 2 MiB`; again → `Ok`,
      `files == 0`; a session at 1 MiB applied, prefill both → `Err(DoesNotFit { needed: 2 MiB,
      limit: 1 MiB })`, `resident_bytes` unchanged by the refused call; a missing path at index 1
      → `Err(Io)` naming the path; a session at 0 → `Err(Off)`. Restore the default at the end.
  - `test_ffi.rs` (the ffi rung, no device):
    - `pinned_host_cache_is_off_without_a_device`: `peacock_pinned_host_cache_stats` → `state ==
      NO_DEVICE`, `capacity_bytes == 0`; `set(PEACOCK_PINNED_CACHE_DEFAULT_BYTES)` → still
      `NO_DEVICE`; a prefill of one temp file through an executor → `PEACOCK_PREFILL_OFF`,
      `files == 0`; `stats(NULL)` non-zero; prefill with NULL `out` → `BAD_ARGUMENT`. One case,
      so libtest's parallel threads share no cache state with another.
  - `test_plan_executor.cpp` (GPU). Both `GTEST_SKIP()` naming `PEACOCK_PINNED_HOST_CACHE_BYTES`
    when it is set (Task 7 Step 6's off run):
    - `PinnedHostCache.ScanFromTheCacheEqualsThePath`: one `CudfScan` over tpch.minimal's
      `customer.parquet` (two row groups, 122880 + 27120 rows; tpch.minimal has no `lineitem`),
      executed with the cache at its default and
      again after `peacock_set_pinned_host_cache_bytes(0, …)`: the two tables are equal column by
      column (`cudf::binary_operation(…, NULL_EQUALS, BOOL8)` reduced with `all`), and the first
      run's stats show `files >= 1`, the second `state == OFF`.
    - `PinnedHostCache.PrefilledScanFillsNothing`: prefill `customer.parquet` through
      `peacock_pinned_cache_prefill`, read the stats, run the scan three times → `fills`,
      `fallbacks` and `reloads` unchanged across the three, and the table equals the path's.

- [ ] **Step 2: run the rust-only ones; expect FAIL to compile.**
  `cargo test -p peacockdb-core --features rust-only --lib -- options pinned_cache`

- [ ] **Step 3: implement.** The singleton and the three ABI symbols in C++ (the prefill maps
  `PrefillOutcome` onto the struct and the return code). In `execute_scan`, before building `opts`:
  `auto lease = pinned_host_cache().acquire(paths);` then the single-span
  `source_info(cudf::host_span<std::byte const>(lease.spans()[0]))` when `lease.cached()` (a load
  has one file, `node_writer.rs:85`), else `source_info{paths}`; the lease lives until
  `read_scan_table` returns, which covers the reader's async copies (Task 2 Step 1). `rows_of` stays on the paths (footer only).
  `PEACOCK_LOG_SCAN_ROWS` adds `source=pinned|path`. The Rust extension, `PinnedCache`,
  `PrefillStats`, `PrefillError`, `plan_files`, the FFI wrappers and the two methods. The harness
  is Task 4b's.

- [ ] **Step 4: run; expect PASS.** The rust-only command of Step 2; then the CPU gates, unchanged:
  `cargo test -p peacockdb-core --features rust-only --lib`,
  `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens`,
  `cargo test -p peacockdb-core --features rust-only --test test_cpu_corpus`, and
  `cargo test -p cost-report --test test_source_formatting` → green, and `git status --short testdata`
  prints nothing (no CPU golden moved). C++: compile both cuDF versions; `ctest -L cpu` green.
  The ffi rung in the 25.02 build image with no `--gpus` (no device):
  `cargo test -p peacockdb-core --lib -- ffi_tests::` and `cargo test -p peacockdb-ffi --test
  test_ffi` → green, `pinned_host_cache_is_off_without_a_device` among them.
  `cargo check -p peacockdb-core --features gpu --tests` against the 25.02 install compiles. The
  GPU cases run in Task 7.

- [ ] **Step 5: Commit** `pinned-loads: the scan reads parquet files from the pinned host cache; pinned_host_cache_bytes; the prefill call`.

### Task 4b: the benchmark prefills once per process, and every round reads the pinned buffers

**Files:**
- Create: `peacockdb-core/src/test_support/benchmark_prefill.rs`, the pure helpers, and
  `benchmark_prefill/tests.rs`. **Decided:** the helpers' module compiles in rust-only. On master
  and on containers `mod corpus_benchmark` is `#[cfg(not(feature = "rust-only"))]`
  (`test_support/mod.rs`), so its `tests.rs` would compile in no rust-only build, and no tier
  runs a default-feature `--lib` unfiltered. `benchmark_prefill` is declared ungated with
  `#[cfg_attr(feature = "rust-only", allow(dead_code))]`, as containers' `benchmark_trailer` is;
  its inputs (`BenchmarkCase`, `executor/pinned_cache.rs`) are ungated already.
- Modify: `peacockdb-core/src/test_support/corpus_benchmark.rs`: `process_prefill` and the checks,
  inside `measure`
- Modify: `peacockdb-core/src/test_support/mod.rs`: the `mod benchmark_prefill;` line; nothing
  public is new (the corpus binary still calls `benchmark_case`)

**Interfaces:**
- Consumes: Task 4's `apply_pinned_host_cache`, `pinned_host_cache() -> PinnedCache`,
  `plan_files`, `Session::prefill_pinned_cache`, `PrefillStats`, `PrefillError`;
  containers' `BenchmarkCase` inventory and its failed section (`CaseOutcome::Failed`, written as
  `status=failed reason=<the case's panic message>`, `FAILED` in `test_support/mod.rs`).
- Produces (bench-rows prints them in the trailer; query-fuser reuses the pieces). In
  `benchmark_prefill.rs`: `selected_benchmark_cases`, `case_query_file`, `ProcessPrefill`,
  `rounds_were_pinned`, `prefill_reason`, `unprefilled`. In `corpus_benchmark.rs`:
  `process_prefill`.
  - `pub(crate) fn case_query_file(case: &BenchmarkCase) -> String`: `case.query.replace('_',
    "-")`, the macro's own spelling (`stringify!($query).replace('_', "-")`), so
    `aggregate_groupby` plans `aggregate-groupby.sql`.
  - `pub(crate) fn selected_benchmark_cases<'a>(args: &[String], cases: &'a [BenchmarkCase]) -> Vec<&'a BenchmarkCase>`:
    the cases this libtest process will run, by libtest's own rules over the test name
    `bench_<dataset>_sf<sf>_<query>_<mode>` (the macro's): positional arguments are filters, a
    name matching any one runs (none = all); `--exact` makes them equalities; each `--skip <s>`
    drops names containing `s`; the flags that take a value (`--test-threads`, `--format`,
    `--color`, `--logfile`, `-Z`, `--skip`) consume it, and their `=` forms
    (`--test-threads=1`, as run-tests.sh passes it) consume nothing more; `NOT_TIMED` cases are
    never selected.
  - `pub(crate) struct ProcessPrefill { pub stats: PrefillStats, pub files: BTreeSet<PathBuf> }`.
  - `pub(crate) async fn process_prefill() -> &'static Result<ProcessPrefill, PrefillError>`: once
    per process (`std::sync::OnceLock`, set after the planning `await`s; the binary runs
    `--test-threads=1`). It applies the first selected case's session option
    (`apply_pinned_host_cache`), plans every selected case (`plan_at(case.dataset, case.sf,
    &case_query_file(case), mode_named(case.mode))`, footers only, untimed),
    takes `plan_files` of each tree, and prefills the union through a `Session` opened on the first
    tree. Prints once: `[pinned-cache] prefill: cases=<k> files=<n> bytes=<n> us=<n> needed=<n> limit=<n>`
    , `[pinned-cache] prefill refused: <error>`, or `[pinned-cache] prefill skipped: the pinned host
    cache is off`. `Off` is kept as the result, not an error to the
    case (pinned-loads' own `cache off` configurations measure from the path on purpose).
  - `pub(crate) fn rounds_were_pinned(before: &PinnedCache, after: &PinnedCache) -> Result<(), String>`:
    `Err("pinned cache: <f> fills, <b> fallbacks, <r> reloads, <e> evictions during the rounds")`
    when any of the four moved, else `Ok`.
  - `pub(crate) fn prefill_reason(prefill: &Result<ProcessPrefill, PrefillError>) -> Option<String>`
    (`Some("prefill: {error}")` for `DoesNotFit` and `Io`, else `None`) and
    `pub(crate) fn unprefilled(case_files: &BTreeSet<PathBuf>, prefilled: &BTreeSet<PathBuf>) -> Option<PathBuf>`.
  - **Inside `measure`**, the future `benchmark_case` hands to `measure_into`, so a panic here is
    the case's recorded section (`status=failed reason=…`, `benchmark_section.rs`). This departs
    on purpose from `benchmark_case`'s "outside the failure record" checks (a refusal is
    per-process, but every case's section must say why it has no numbers: sf40-baseline's skip
    table reads it); add one sentence saying so beside that comment. Under a capture no section
    is written, and a panic is only a panic, as today. In this order, first in `measure`:
    `process_prefill().await`;
    `Err(DoesNotFit | Io)` → `panic!("prefill: {error}")` (the failed section's reason, exactly
    `prefill: needs <needed> > limit <limit>`); `Err(Off)` → go on with no check; `Ok(p)` →
    every file of `plan_files(&[tree])` is in `p.files`, else `panic!("prefill: {path} was not
    prefilled")`. Then `let before = pinned_host_cache()` just before the warm-up, `let after =
    pinned_host_cache()` just after the last measured run, and `rounds_were_pinned(&before,
    &after)` → `Err(r)` is `panic!("{r}")`: the case's section is a failed one, never numbers that
    read a file from disk. After the runs: `eprintln!("[pinned-cache] {what}: {after}")` and
    `eprintln!("[load-bytes] {what}: compressed=<n> decoded=<n>")`: `compressed` = Σ over the
    tree's `GpuLoadParquet` nodes of their `survivors`' `bytes` (projected parquet bytes), and
    `decoded` = Σ bytes of the batches those nodes emitted in the reported run (from
    `RunReport::emitted`, what bench-rows' `batch_bytes` renders). bench-rows moves the cache and
    the prefill into the trailer; Task 7 Step 9 reads `[load-bytes]`.

- [ ] **Step 1: failing tests** (rust-only, `benchmark_prefill/tests.rs`), over a literal case list
  `[tpch 40 q1 tp1_single, tpch 40 q6 tp1_single, tpch 40 q6 tp4_sized, tpcds 40 q3 tp1_single,
  tpch 40 aggregate_groupby tp1_single, tpch 40 q2 NOT_TIMED]`:
  - `selection_follows_libtest_filters`:
    - `[]` → the five timed cases; `["bench_tpch_sf40_q6"]` → both q6 modes;
    - `["--exact", "bench_tpch_sf40_q6_tp1_single"]` → that one;
    - `["bench_tpch", "--skip", "tp4_sized"]` → q1, q6 and aggregate_groupby at tp1_single;
    - `["--test-threads", "1", "--nocapture", "q1"]` → q1 only (`1` is not a filter);
    - `["--test-threads=1", "q1"]` → q1 only;
    - `["aggregate_groupby"]` → that case;
    - `["q2"]` → none (NOT_TIMED).
  - `a_micro_query_plans_by_its_file_name`: `case_query_file` of the `aggregate_groupby` case ==
    `"aggregate-groupby"`, of `q1` == `"q1"`.
  - `rounds_were_pinned_names_what_moved`: equal snapshots → `Ok`; `fills` 8 → 9 and
    `fallbacks` 0 → 1 → `Err("pinned cache: 1 fills, 1 fallbacks, 0 reloads, 0 evictions during the rounds")`.
  - `a_refused_prefill_fails_every_case_with_its_size`: `prefill_reason(&Err(DoesNotFit { needed:
    60, limit: 49 }))` (the helper `measure` panics with, `-> Option<String>`) ==
    `Some("prefill: needs 60 > limit 49")`; `Err(Off)` → `None`; `Ok(_)` → `None`.
  - `a_case_outside_the_prefill_is_refused`: `unprefilled(&{a, b}, &{a, c}) == Some(c)`
    (`fn unprefilled(case_files: &BTreeSet<PathBuf>, prefilled: &BTreeSet<PathBuf>) -> Option<PathBuf>`,
    the first missing in order); `({a}, {a, b})` → `None`.
- [ ] **Step 2: run; expect FAIL to compile.**
  `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_prefill`
- [ ] **Step 3: implement** the helpers, `process_prefill` and the `measure` order above. Remove
  nothing else from `benchmark_case` or `measure`.
- [ ] **Step 4: run; expect PASS**, then Task 4 Step 4's CPU gates and its `cargo check … --features
  gpu --tests`. The device half (a benchmark run whose `[pinned-cache] prefill:` line shows
  `files > 0` and whose cases all pass `rounds_were_pinned`) runs in Task 7 Step 3.
- [ ] **Step 5: Commit** `pinned-loads: the benchmark prefills every case's files once per process; rounds read pinned buffers only`.

### Task 5: the scan reads through a one-chunk `chunked_parquet_reader` (spec item 0b, #232)

**Files:**
- Modify: `cpp/src/operators/scan.cpp`: `read_scan_table`, called by `execute_scan` for either source
- Modify: `cpp/src/peacock/operators.h`: its declaration (for the test)
- Test: `cpp/tests/gpu/test_plan_executor.cpp`: `ScanReader.*`

**Interfaces:**
- Produces:

```cpp
namespace peacock {
enum class ScanReader { Chunked, ReadParquet };
// 0 = cuDF's "no limit": the call's row groups come back as one chunk. The read limit is the
// plan's batch, which the loader already sized; a second size here is what #232 warns against.
inline constexpr std::size_t kScanChunkReadLimit = 0;
ScanReader scan_reader();   // PEACOCK_SCAN_READER, read once: unset or "chunked" → Chunked,
                            // "read_parquet" → ReadParquet, anything else throws naming it
cudf::io::table_with_metadata read_scan_table(cudf::io::parquet_reader_options const& opts,
                                              ScanReader reader,
                                              std::size_t chunk_read_limit = kScanChunkReadLimit);
}
```

  `Chunked`: one `chunked_parquet_reader(chunk_read_limit, opts)`, `read_chunk()` while
  `has_next()`; one chunk is returned as is, several are concatenated (`cudf::concatenate`, the
  first chunk's metadata). `read_scan_table` returns `struct ScanRead { cudf::io::table_with_metadata
  table; std::size_t chunks; }` (`ReadParquet` → `chunks == 1`), so a test reads the count
  without the log. `PEACOCK_LOG_SCAN_ROWS` adds `reader=<chunked|read_parquet> chunks=<n>`.

- [ ] **Step 1: failing gtests** over tpch.minimal's `customer.parquet` (two row groups, 122880 +
  27120 rows; tpch.minimal has no `lineitem`), all its row groups, four columns:
  - `ScanReader.OneChunkEqualsReadParquet`: `read_scan_table(opts, Chunked)` equals
    `read_scan_table(opts, ReadParquet)` (column-by-column, as Task 4's test compares), names equal.
  - `ScanReader.SeveralChunksConcatenateToReadParquet`: `chunk_read_limit = 1 << 20` → equal to
    `read_parquet`'s table, and the returned `chunks > 1`.
  - `ScanReader.RowGroupSelectionIsKept`: `opts.set_row_groups({{1}})` → the same rows from both.
  - `ScanReader.TheSwitchIsReadOnce`: `scan_reader()` returns `Chunked` with the variable unset.
- [ ] **Step 2: build; expect FAIL to compile** (no `read_scan_table`).
- [ ] **Step 3: implement**, and replace `execute_scan`'s `cudf::io::read_parquet(opts)` with
  `read_scan_table(opts, scan_reader()).table`.
- [ ] **Step 4: build for both cuDF versions** (the constructor and `has_next`/`read_chunk` exist
  in both; Task 2 Step 1 quoted them). `ctest -L cpu` green. The GPU cases run in Task 7.
- [ ] **Step 5: Commit** `pinned-loads: the scan reads through a one-chunk chunked parquet reader (#232)`.

### Task 6: the engine configures cuDF's pinned IO memory on a discrete device (spec item 1)

Skip it when the Decision is `NULL_RESULT`. Then write "Task 6 skipped: NULL_RESULT" in the detail
file and go to Task 7.

**Skipped, 2026-11-05: the Decision is `NULL_RESULT`** (detail file, "Decision").

**Files:**
- Create: `cpp/src/peacock/pinned_io.h` (declarations), `cpp/src/pinned_io.cpp`
- Modify: `cpp/CMakeLists.txt:148-169`: add `src/pinned_io.cpp` to `peacock_gpu`
- Modify: `cpp/src/operators/scan.cpp`: call `ensure_pinned_io_pool()` before `read_scan_table`
  (and before `read_parquet_metadata` in `rows_of`, line 25)
- Modify: `cpp/tests/gpu/test_cudf.cpp`: new `PinnedIo.*` cases; `main()` (line 51) calls
  `peacock::ensure_pinned_io_pool()` right after `install_rmm_pool`. This is the engine function,
  not a copy of the cuDF call. It runs first so that no earlier cuDF call configures the default.

**Interfaces:**
- Consumes: Task 2's Decision and its measured peak; Task 3's `is_discrete_for_pinning`.
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
std::size_t pinned_io_pool_bytes();                          // env, else kPinnedIoPoolBytes
const PinnedIoStatus& ensure_pinned_io_pool();               // once per process (std::call_once)
}
```

  `ensure_pinned_io_pool` reads the current device's `pageableMemoryAccess`. On a discrete device
  (`is_discrete_for_pinning`) it calls `cudf::config_default_pinned_memory_resource({pinned_io_pool_bytes()})`,
  and `false` maps to `AlreadyConfigured`. For `RAISE_THRESHOLD` it also calls
  `cudf::set_allocate_host_as_pinned_threshold(kPinnedHostVectorThreshold)`. It writes one stderr
  line, shaped like rmm_pool's: `[pinned-io] cuDF pinned pool 1.0 GiB on a discrete device` /
  `… already configured …` / `… kept cuDF's default (pageable memory access)`. Leave out whichever
  constant the Decision does not use.

- [ ] **Step 1: failing gtests**, in `test_cudf.cpp`:
  - `PinnedIo.DiscreteDeviceGetsTheBoundedPool`: if the device's `name` is one of the cards
    "Discrete cards" lists as discrete (`"NVIDIA H100 80GB HBM3"`, `"NVIDIA L40S"`, as the detail
    file names them), then `ensure_pinned_io_pool().state == Configured`, with no else branch for
    those; the same when `pageableMemoryAccess == 0`. Then `pool_bytes == pinned_io_pool_bytes()`,
    and `pool_bytes <= 4 GiB`. A 1 MiB allocation from `cudf::get_pinned_memory_resource()` has
    `cudaPointerGetAttributes(...).type == cudaMemoryTypeHost`. Any other device: `state == KeptDefault`.
  - `PinnedIo.SecondCallReturnsTheFirstOutcome`: two calls return the same address and the same
    state.
  - `PinnedIo.EnvironmentSizeWins`: `setenv("LIBCUDF_PINNED_POOL_SIZE", "268435456", 1)` makes
    `pinned_io_pool_bytes() == 268435456`. `unsetenv` brings back `kPinnedIoPoolBytes`.
- [ ] **Step 2: build; expect FAIL to compile** (`pinned_io.h` missing), in the 25.02 build image.
- [ ] **Step 3: implement** `pinned_io.cpp` and the two `scan.cpp` calls. No other engine file
  changes.
- [ ] **Step 4: build for both cuDF versions.** Pass: both compile. The API exists in both
  (`pinned_memory.hpp:32-57` in third_party). The GPU cases run in Task 7.
- [ ] **Step 5: Commit** `pinned-loads: the engine sizes cuDF's pinned IO pool on a discrete device (#149)`.
  Run `git clang-format` on the changed lines first.

### Task 7: the after runs, each fix alone and together, on the H100; the L40S list, cache on and off

**Files:**
- Modify: `llm-wiki/tasks/pinned-loads-detail.md`: sections "After runs", "After profile",
  "Readers from pinned memory (#232)" (the after half), "Before/after", "Cache", "Signoff: load
  rate", "Integrated: deferred"

**Interfaces:**
- Consumes: Tasks 1-6, 4b (the `[pinned-cache] prefill:`, `[pinned-cache] <case>:` and
  `[load-bytes]` lines).
- Produces: the detail file's numbers, which Task 8 archives with the tickets; "Signoff: load
  rate" is spec Done-when 4's table.

The configurations, each a set of variables in the run's environment (the way `PCK_TEST_FILTER`
reaches it):

| name | `PEACOCK_PINNED_HOST_CACHE_BYTES` | `PEACOCK_SCAN_READER` | what it isolates |
|---|---|---|---|
| `before` | (Task 2, today's engine) | | the baseline: `read_parquet` from the path |
| `pool` (only if Task 6 ran) | `0` | `read_parquet` | cuDF's pool alone |
| `cache` | unset | `read_parquet` | the cache alone (with the pool, if any): `read_parquet` from the cache |
| `chunked` | `0` | unset | the chunked reader alone (with the pool, if any): chunked from the path |
| `all` | unset | unset | the engine's default, everything together: chunked from the cache |

Under `cache` and `all` each `run bench` process prefills its query's files once (Task 4b) and
every round reads them pinned; under `pool` and `chunked` the cache is off (`prefill` is `Off`) and
every round reads the path, on purpose, since those rows measure from the path.

- [ ] **Step 1: build first.** The CPU-VM build of this tree (Tasks 3-6 in it) into `cpp/install/`;
  the CPU VM stopped.
- [ ] **Step 2: start the H100, ship, run the new cases.** `start`; `ship`; only
  `PCK_TEST_FILTER=duckdb_gpu_tpch_q6_tp1_single run filter` (one corpus case, cache on). The
  correctness list is the L40S's (host roles, VM-time rule), in Step 6. `run filter` still runs
  the gtest binaries, which hold the new C++ cases, and the lib's `gpu_tests::` rung, which holds
  `pinned_cache::gpu_tests`, whole; the filter narrows the other rust binaries. Pass:
  `PinnedHostCache.CudaAllocatorPinsHostMemory`, `PinnedHostCache.RefusedAllocationClearsTheCudaError`,
  `PinnedHostCache.ScanFromTheCacheEqualsThePath`, the four `ScanReader.*`, the `PinnedIo.*` (if
  Task 6 ran), `PinnedHostCache.PrefilledScanFillsNothing`,
  `the_session_option_reaches_the_cache`,
  `prefill_reports_its_files_and_refuses_what_does_not_fit` and the corpus case pass; stderr shows the
  `[pinned-cache] … on a discrete device` line (and `[pinned-io] …` if Task 6 ran).
- [ ] **Step 3: after load times (unprofiled).** For each configuration but `before`, for each of
  `q1 q3 q6 q8`: `run bench` as in Task 2 Step 4 with the configuration's variables, `pull`, remove
  the host record, and move the section file aside as `tp1_single.<config>.txt`. Copy each
  `GpuLoadParquet` `total_us`, the `[pinned-cache] prefill:` line, each case's
  `[pinned-cache] <case>: …` and `[load-bytes] <case>: …` lines into "After runs".
  Pass: every run exits 0 and no section is `status=failed` (a `pinned cache: … during the rounds`
  or `prefill: …` reason fails the step and is explained before a rerun); under `cache` and `all`,
  the prefill line shows `files` > 0, and `[pinned-cache] <case>:` shows `on`, `fallbacks=0` and
  `resident` > 0; under `pool` and `chunked`, `off source=env` and `prefill skipped: the pinned
  host cache is off`.
- [ ] **Step 4: after profiles.** `create_nsys_profile.sh --trace` (Task 2 Step 5's command) for
  each of `q1 q3 q6 q8` under `all`, under `cache` (read_parquet from the cache: #232's after
  half) and under `chunked` (the chunked reader from the path), keeping `loads-<q>-all.tsv`,
  `loads-<q>-cache.tsv` and `loads-<q>-chunked.tsv` and restoring `calls.tsv` as there. The
  profiled process prefills before nsys's capture of the measured rounds, so no load window holds
  a disk read under `all` or `cache`: their `host_read_us` is 0, and a non-zero one is explained.
  Then stop the H100. Pass: as in Task 2 Step 5.
- [ ] **Step 5: the cache's cost, from the runs.** Into "Cache": per query process under `all`,
  the prefill's `files`, `bytes` and `us` (the only read from disk; no round paid it) and the
  cache's `resident`; per dataset, the largest prefill and the sum of the four; the capacity
  (`limit`), the run container's cgroup limit and the H100's host RAM; and one line: every
  query's files fit the default, or which prefill refused and its `needed`.
- [ ] **Step 6: the L40S list, cache on and off** (the Definition of done; correctness only, no
  sf40), the H100 stopped, bracketed as `prompts.md` says at this point of the chain (the L40S
  bracket builds on the VM after its start; containers-only moves the build first later): the
  L40S runs its list at the default, runs it again with `PEACOCK_PINNED_HOST_CACHE_BYTES=0`, and
  stops. Pass: both green, with the same pass counts and the GPU corpus goldens unmodified (the
  byte-identity check, on and off). The first run's stderr shows `[pinned-cache] … on a discrete
  device`, the second `off (… env)`. (The L40S's `devprops` values were recorded in Task 2 Step
  3.)
- [ ] **Step 7: read the copy rate.** Pass condition: under `all`, in every load with
  `h2d_max_bytes ≥ 16 MiB`, `h2d_pinned_gbps` is within 20% of 55.5 GB/s and `h2d_pageable_bytes`
  is under 1% of `h2d_bytes` (cuDF's small `make_host_vector` copies stay pageable at threshold
  0; the bulk copies are what must be pinned). If not, the detail file explains the gap from the profile. Possible explanations: the copy
  size (`h2d_max_bytes`, against the probe's 4 GiB), copies overlapping on the link (`h2d_us`
  against the union of copy intervals), or the host-buffer source not copying as Task 2 Step 1
  read it. The task does not move on until the gap is explained.
- [ ] **Step 8: before/after table.** In "Before/after", one row per query × load:
  `GpuLoadParquet total_us` under each configuration, the change of each against `before`, and the
  copy-share estimate from Task 2 Step 7. Then "Readers from pinned memory (#232)": `cache` against
  `all` (read_parquet against one chunk, both from the cache), beside Task 2's probe lines. Write
  one paragraph per fix: its own share, and whether the shares add up under `all`. A gain smaller
  than the copy share is a finding, explained. A gain within the run-to-run spread of `runs=[…]`
  is noise and is recorded as noise. Open a ticket in `tickets/performance.md` only if the profile
  points at decode, decompression or the read path, and name it by what the profile shows.
- [ ] **Step 9: the signoff load-rate table** (spec Done-when 4), in "Signoff: load rate", from
  Steps 3-5 and Task 2 Steps 4-5. One table per query of `q1 q3 q6 q8`, one row per reader and
  source:

  | reader, source | config | compressed bytes | decoded bytes | `load_us` | compressed GB/s | decoded GB/s | H2D GB/s (profile) |
  |---|---|--:|--:|--:|--:|--:|--:|
  | `read_parquet`, path (before) | `before` | | | | | | |
  | `read_parquet`, cache | `cache` | | | | | | |
  | chunked, path | `chunked` | | | | | | |
  | chunked, cache (the default) | `all` | | | | | | |

  - compressed and decoded bytes: the case's `[load-bytes]` line (`compressed`, `decoded`; equal
    in every config, since the plan is the same: a difference is explained);
  - `load_us`: Σ `GpuLoadParquet` `total_us` of the reported run, the same quantity bench-rows
    calls `load_us`;
  - each GB/s is `bytes / (load_us · 1000)`;
  - the H2D rate is `h2d_gbps` from `loads-<q>-<config>.tsv`, bytes-weighted over the query's
    loads, with `h2d_pinned_gbps` beside it; `before` reads Task 2's `loads-<q>-before.tsv`.

  The `before` row's bytes come from the `all` run's `[load-bytes]` line (today's engine has no
  such line; the plan is unchanged). Below the tables, the prefill per dataset: files, bytes, `us`
  and GB/s (`bytes / (us · 1000)`), from Step 5. One sentence per fix: the cache's share
  (`before` → `cache`, and `chunked` → `all`) and the reader's (`before` → `chunked`, and
  `cache` → `all`).
- [ ] **Step 10: integrated, deferred.** One line in "Integrated: deferred": no GB10 is available
  to this chain. The branch is covered by
  `PinnedHostCache.DefaultIsThreeQuartersOfTheSmallerOfRamAndCgroup`'s `is_discrete_for_pinning`
  cases, and #149's "integrated does not regress" measurement is deferred.
- [ ] **Step 11: Commit** `pinned-loads: the H100 after runs, each fix alone and together, the signoff load rates; the L40S list, cache on and off`
  (the detail file).

### Task 8: the wiki, #149 and #232 closed

**Files:**
- Modify: `llm-wiki/architecture.md`:
  - "cuDF options" table (line ~1341): the `parquet_reader_options` row's "Set at" names
    `read_scan_table`; new rows for the source (`source_info` over the cache's pinned spans, else
    the paths), the reader (`chunked_parquet_reader`, `kScanChunkReadLimit = 0`: one chunk;
    `PEACOCK_SCAN_READER=read_parquet` for measurement), the cache (`pinned_host_cache_bytes`) and,
    if Task 6 ran, the pool (`cudf::config_default_pinned_memory_resource`, `kPinnedIoPoolBytes`,
    `LIBCUDF_PINNED_POOL_SIZE` overrides; "What the default would do": a 100 MiB pool, then one
    `cudaHostAlloc` per larger buffer, as measured in Task 2).
  - The cache row's host-memory cost, in words: up to `min(host RAM, the cgroup memory limit when
    one is set) × 3/4` pinned for the life of the process (about 147 GB on the 196 GB H100 VM, whose run
    container has no limit), never swappable, filled on first use or by the prefill, freed only by
    eviction or exit; `0` turns it off; `PEACOCK_PINNED_HOST_CACHE_BYTES` wins; off on an integrated
    device. With the pool, the two together are the process's pinned host memory.
  - The scope, in one sentence beside it: **the cache and its option are process-wide**; every
    executor and every session in the process shares one cache, and the last
    `pinned_host_cache_bytes` applied wins (the human's decision, 2026-11-03).
  - The prefill: `peacock_pinned_cache_prefill` makes named files resident whole, idempotent,
    refusing before it pins anything when they exceed the limit; the benchmark calls it once per
    process over every case's `plan_files`, so its rounds read only pinned buffers.
  - Who applies the option: only the harnesses (the benchmark, Task 4b, and query-fuser's fused
    run) call `apply_pinned_host_cache` today; no engine path does, so another GPU run gets the
    default or the environment's value.
  - "Eviction only between calls" holds through leases, not timing: `acquire` evicts unleased
    entries while another query runs, and a leased buffer is never freed.
  - No CUDA device: the cache is off (`off no-device`). The cgroup limit is read from the root
    cgroup files only, which is the container's own inside a cgroup namespace; outside one (a host
    slice, `--cgroupns=host`) it is not seen.
  - The ABI sentence (`:1117`): the count on the branch + 3, in words, and
    `set_pinned_host_cache_bytes`, `pinned_host_cache_stats`, `pinned_cache_prefill` in the
    instrumentation group.
- Modify: `llm-wiki/build-test.md`: beside `PEACOCK_RMM_POOL_BYTES` (`:1648`):
  `PEACOCK_PINNED_HOST_CACHE_BYTES` and `PEACOCK_SCAN_READER`, and that a benchmark process
  prefills every selected case's files once before its first case (the `[pinned-cache] prefill:`
  line), a refusal failing each case `prefill: needs <needed> > limit <limit>`, and a round that
  filled or fell back failing its case; the "C++ CPU/FFI unit" row (`:883`) names the cache's cases and its count; the
  "cuDF GPU smoke" and "Plan-executor" rows' counts; the ffi rung's count (`test_ffi` + 1,
  `:672`) and the gpu lib rung's (+2, `pinned_cache::gpu_tests`); the header total recounted.
- Modify: `llm-wiki/tickets/performance.md`: remove #149 and #232, their `## Contents` lines
  (9 and 12) and any text that points at them (`#148`'s)
- Modify: `llm-wiki/archive/archived-tickets.md` (`## Done`): #149 and #232 with the measured
  numbers (for #149, under stale/obsolete as "misdiagnosed" only if `NULL_RESULT` and the cache
  gained nothing)
- Modify: `llm-wiki/tickets.md`: performance count 9→7, the total open count (`77 open tickets`,
  `:22`) recounted (closing a ticket leaves the next free number, `currently <N>` at `:15`, as
  it is), and drop `#149` and `#232` from the index line (33)

- [ ] **Step 1: the architecture.md edits.**
- [ ] **Step 2: the build-test.md edits.** Recount from `--list` and `ctest -N` as the page's rule says.
- [ ] **Step 3: close #149 and #232.** Move the ticket texts to the archive with: the cuDF 25.02
  finding; the four queries' numbers per configuration; the H2D rates by kind; the signoff
  load-rate tables (Task 7 Step 9) and the prefill's bytes and time per dataset; the reader comparison from Task 2's probe and Task 7; the Decision. Update the
  index and the counts. Then run `python3 scripts/ticket_widget.py`. Pass: it exits 0, and
  `testdata/report.html` changes only if the registry names #149 or #232 (`grep -nE '149|232'
  testdata/cost-registry.csv`).
- [ ] **Step 4: CPU gates.** `for f in scripts/calibration/tests/test_*.py; do python3 "$f" ||
  exit 1; done`; `cargo test -p peacockdb-core --features rust-only --lib`;
  `cargo test -p peacockdb-core --features rust-only --test test_corpus_goldens`;
  `cargo test -p cost-report --test test_source_formatting`. Pass: all green. Every VM is stopped
  (`status` on each). The detail file's `## Suites` names each suite this task ran and where
  (`scripts/calibration/tests` and `ctest -L cpu` on dev; the gtests and device tiers on the H100
  and the L40S). CI runs `scripts/calibration/tests` and `peacock_cpu_tests`; nothing new needs a
  CI glob.
- [ ] **Step 5: Commit** `pinned-loads: the cache, the reader and their cost in the wiki; #149 and #232 closed`
  (`architecture.md`, `build-test.md`, the tickets, the archive, `tickets.md`, the report if it changed).
- [ ] **Step 6: green.** Report the task green: the CPU gates of Step 4, the H100 and L40S runs of
  Task 7, the detail file complete.

## Self-review notes

- Spec A → Task 1. B → Task 2, Steps 4-5, 7. C → Task 7, Steps 4 and 7. 0 → Task 2, Step 1.
  0a → Task 3 (the cache and its prefill, CPU-side: eviction between calls, reload, fallback, a
  read against an eviction, prefill idempotence, refusal before pinning, the cgroup default),
  Task 4 (the scan, the session option, the three ABI symbols, `prefill_pinned_cache`,
  `plan_files`, the device tests in `pinned_cache::gpu_tests`, the no-device case in `test_ffi`)
  and Task 4b (the benchmark: one prefill per process over every
  selected case, every round from pinned buffers, a refusal as `prefill: needs … > limit …`, no
  fill or fallback in any round). The trailer's `prefill_files`/`prefill_bytes`/`prefill_us` are
  bench-rows' (it owns the trailer); Task 4b prints them.
  0b → Task 5; #232's re-measure first → Task 2 Step 6, its after half → Task 7 Steps 4, 8.
  1 → Task 6, conditional on the Decision, as "unless cuDF already pins" requires. 2 → Task 2
  Step 4 and Task 7 Steps 3, 8 (each fix alone and together; byte-identical on and off → Task 7
  Steps 2, 6). 3 → the cache's divisor, the pool's constant, and Task 8's architecture.md rows.
  4 → Task 7 Step 9 (the signoff table: four reader × source rows per query, the prefill per
  dataset), archived in Task 8 Step 3. 5 → Task 8 Step 3; the harness gets the cache through the
  engine (Tasks 4, 4b), and so does a fused run (query-fuser prefills each batch's union through
  the same calls). Constraints: GB10 deferred → Task 7 Step 10; host memory per dataset → Task 2
  Step 3, Task 7 Step 5; the H100 via new-hosts → Task 2's Consumes.
- `cudf::io::set_host_memory_resource`, named by the spec, is absent from both pinned cuDF versions.
  Task 6 uses its successor. Task 2 Step 1 records this.
- The cache and its option are process-wide (decided 2026-11-03): "session option" means it is
  set through the session's config (`peacock.pinned_host_cache_bytes`) and applied when a GPU run
  starts from that session; two sessions in one process share one cache, and the last applied
  size wins. architecture.md says so (Task 8).
- `kScanChunkReadLimit = 0` (no limit) is decided, not open.
- The three cache symbols match the spec's Scope (settled 2026-11-03).
