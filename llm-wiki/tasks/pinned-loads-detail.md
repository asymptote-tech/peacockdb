# pinned-loads — detail

## Run log

- 2026-11-04: `ENS-pinned-loads` branched from `ENS-new-hosts` at `add2a081`, while new-hosts is
  `blocked(building)` (the human's GitHub PAT; see `new-hosts-detail.md`, "Blocked"). The H100
  path (the CPU-VM build, `run-gpu.sh`) is not live yet, so only the offline steps can run: Task 1,
  and Task 2 Steps 1–2. Task 2 Step 3 onward needs the H100 and waits.
- 2026-11-04: Task 1 offline: `nsys_loads.py` → `loads.tsv`, its tests, the `--trace` wiring and
  the `build-test.md` edits. Notes below.
- 2026-11-04: Task 2 Steps 1–2, and Step 3's L40S bracket only. The H100 is not live, so Step 3
  stops after the L40S: no CPU-VM build, no H100 start. Steps 3 (H100 part) to 9 wait for it.
- 2026-11-04: Step 3's "Discrete cards", both cards: the L40S on the coordinator's retry, the
  H100 in its own short bracket. Also the H100's `free -b` for "Host memory". Both report
  `pageableMemoryAccess = 0`. The rest of Step 3 (CPU-VM build, `ship`, sf40 sizes, container
  limit) still waits for the H100 path.
- 2026-11-04: Task 3 offline (the cache alone, no VM): `pinned_host_cache.h`/`.cpp`, 30 CPU
  cases, 2 GPU cases. Notes below.
- 2026-11-04: Task 4 offline (the scan reads the cache; the option and the three symbols), no
  VM. Notes below.
- 2026-11-04: Task 4b offline (the benchmark's prefill), no VM. Notes below.
- 2026-11-04: Task 5 Steps 1–4 offline (the scan reads through a one-chunk chunked parquet
  reader), no VM. Notes below.
- 2026-11-05: new-hosts done; the block lifted. Rebased onto `596261f8`, re-proven offline, then
  Task 2 Steps 3–8 on the H100 path. Notes under "Resumed after new-hosts".

## Task 1: the load breakdown

- **Red → green.** `test_loads.py` failed first with `ModuleNotFoundError: No module named
  'nsys_loads'`, then 10/10 passed. Every `scripts/calibration/tests/test_*.py` passes: calls 6,
  fetch 3, hbm 14, loads 10, split 11 on dev; plot 7 in the 25.02 build image (dev has no
  matplotlib).
- **One test more than the plan:** `test_work_reaching_past_the_load_is_clipped_to_it`. No plan
  test pinned the clipping: with clipping removed the other nine still passed.
- **Checked by mutation:** crediting an ambiguous launch to one load fails
  `test_concurrent_…`; counting every launch as off-thread fails the first test; matching every
  OS call as a read fails `test_osrt_…`; removing the clipping fails the new test.
- **No `.gitignore` line.** `testdata/.gitignore` is deny-by-default over `calibration/`, so
  `loads.tsv` is already ignored (`git check-ignore -v` names `testdata/.gitignore:15`). The plan
  asked for a root `.gitignore` line; it would be dead.
- **Small departures from the plan's interface.** `load_windows` keys windows by execution, not
  by raw occurrence: run // call indices. `device_ops` returns a namedtuple that also carries
  `copyKind`, since a non-HtoD copy goes to `other_device_us`. Launches are read from
  `CUPTI_ACTIVITY_KIND_RUNTIME` and, when present, `…_DRIVER`, because `PINNED_ALLOC_RE` names
  `cu*` driver calls. Rates are computed per execution, then the median is taken, then rounded
  to 0.1 GB/s.
- **What the ambiguity rule covers.** Device operations and pinned allocations launched where two
  loads are open go to neither. Only the device operations are printed as `ambiguous`. Host reads
  are intervals, not launches: they are clipped to each load's windows, so two concurrent loads
  both count a read that overlaps them.
- **For Task 2, on the first real capture:**
  - `--trace` records `nvtx,cuda` only, so there is no `OSRT_API`. `host_read_source` is
    `kvikio` only if 25.02's kvikio pushes its `libkvikio` NVTX ranges, else `none`. Check
    which in the before profile. Adding `osrt` to `--trace` would cost overhead in the spans the
    trace pass exists to keep clean; it is not done here.
  - `DECOMPRESS_RE` and `DECODE_RE` have only met synthetic names. Task 2 Step 5 checks them
    against the real kernel names.
  - `nsys_calls.regions` finds a partition's call as the latest call starting before it, on any
    thread, and refuses it if that call is on another thread. So a multi-lane capture whose
    calls interleave across threads is refused before `nsys_loads.py` sees it. tp1_single is
    one lane and is not affected.
- **shellcheck** is on neither dev nor the build image. `koalaman/shellcheck:stable` over HEAD's
  `create_nsys_profile.sh` and over the edited one gives the same findings: SC2034
  (`REMOTE_SSH` unused) and SC1091 (info), both there before. `bash -n` is clean. The image was
  removed after.

## Task 3: the pinned host file cache

- **Red → green, in the 25.02 build image** (build dir `/tmp/pl-build25` mounted over
  `cpp/build`; dev's own `cpp/build` is a host build and was left alone). Red:
  `test_pinned_host_cache.cpp:5:10: fatal error: peacock/pinned_host_cache.h: No such file or
  directory`. Green: `ctest -L cpu` 1/1; `peacock_cpu_tests` 56/56 (26 before + 30
  `PinnedHostCache.*`), three runs in a row. No warning from the new files.
- **The GPU cases are compiled, not run:** `PinnedHostCache.CudaAllocatorPinsHostMemory` and
  `PinnedHostCache.RefusedAllocationClearsTheCudaError` in `peacock_gpu_tests` (listed by
  `--gtest_list_tests`). They run in Task 7. `peacock_gpu_tests` gained `PEACOCK_TESTDATA_DIR`
  (env overrides) for the second one's `customer.parquet`.
- **Not done: the 26.02 compile** the plan's Step 4 asks for. Dev holds one image at a time (the
  dispatch's disk rule). The new code uses no API that differs between the two: CUDA runtime
  (`cudaHostAlloc`, `cudaFreeHost`, `cudaGetLastError`), POSIX, and in the GPU test
  `read_parquet`, which the reader probe already compiles against both. Task 4 or 5, which
  build 26.02 anyway, cover it.
- **First green run had 3 failures, all in the test's allocator.** `was_freed(p)` read false
  after an eviction that was followed by a new 1 KiB allocation: glibc handed the freed block
  straight back, and the allocator un-marked it. The allocator now poisons a freed block and
  keeps it until the test ends, so an address is never reused within a test (this also makes a
  read of a freed entry in `ConcurrentReadAndEviction` see `0xDD` for certain).
- **Checked by mutation:** letting `make_room_locked` evict a leased entry fails
  `LeasedEntryIsNeverEvicted`, `ConcurrentReadAndEviction`, `ReleaseEvictsWhileOverCapacity` and
  `SetCapacityEvictsTheUnleased`. Dropping `acquire`'s wait for an in-flight fill fails
  `SecondReaderWaitsForTheFill` and `AFailedFillWakesItsWaiters`.
- **One rule for the fit, in acquire and prefill:** `limit = capacity − leased bytes that are
  not the named files' current contents` (leased entries of other paths, a named path's leased
  entry whose key moved, stale copies still leased: `stale_`). A request fits when the named
  sizes' sum ≤ limit; only then are unleased entries evicted, so a scan that falls back evicts
  nothing. In `acquire` the fallback line names the path at which the running sum passes the
  limit.
- **Small departures from the plan's text:**
  - A path that cannot be `stat`ed, in `acquire`, is no fallback: not counted, not said. The
    scan reads the path and cuDF names the error. (The plan says only "no throw".)
  - A fill whose read fails says `not cached: read failed: <strerror>` (for example `Stale file
    handle` for a short read at EOF). The plan names only the two reasons for a fit and a refused
    allocation.
  - `peacock_cpu_tests` did not need `src/` added to its include directories: `peacock_gpu`
    exports it as a PUBLIC build interface already, and `test_executor.cpp` relies on it.
  - `Lease` is move-only with no public default constructor; tests hold an optional one.
  - The cache frees a buffer under its lock (`cudaFreeHost` included), so `resident_bytes` never
    drops before the memory does. Eviction is rare in a run (prefill first), so the cost is
    accepted.
  - The fallback line is said once per path per cache instance; the process has one (Task 4's
    singleton).
- **For Task 4:** the singleton is still to write (`pinned_host_cache()`); nothing calls the
  cache yet, and `scan.cpp` is untouched. `PrefillOutcome::failed_index` is an index into the
  caller's `paths` (the first occurrence of a duplicate).

## Task 4: the scan reads the cache; the option and the prefill

- **Red → green.** Rust, rust-only: `cargo test … --lib -- pinned_cache` failed to compile
  (`unresolved imports crate::executor::PeacockOptions, PinnedCache, …, plan_files`); then the
  three option tests failed on `Could not find config namespace "peacock"` until
  `build_session_state` registered the extension; then 8/8. C++, in the 25.02 image: both
  `peacock_cpu_tests` (`‘PinnedCacheConfig’ in namespace ‘peacock’ does not name a type`) and
  `peacock_plan_tests` (`‘peacock_pinned_cache_prefill’ was not declared`) failed to compile;
  then `peacock_cpu_tests` 60/60. The FFI shape first failed on `SessionConfig`'s path
  (`datafusion::execution::context::SessionConfig` in DataFusion 45), which no rust-only build
  compiles.
- **Departures from the plan's text:**
  - No `options.rs` and no `pub mod options;`. `pub_mod_declares_a_component_and_nothing_else`
    pins `lib.rs` to six components, and `bare_pub_is_the_surface_and_nothing_else` refuses a
    bare `pub` outside `SURFACE`. `PeacockOptions` and the cache's types are `pub(crate)` in
    `executor/mod.rs`, beside `RmmPool`; `executor/pinned_cache.rs` holds their trait impls and
    `plan_files`. Fields are `pub(crate)`: `executor/mod.rs` is a surface file, and a bare `pub`
    field there fails the same gate. The option's tests are in `executor/pinned_cache/tests.rs`.
  - `extensions_options!` cannot carry `Option<u64>`: its `set` calls `FromStr` and its
    `entries` calls `Display`. `ExtensionOptions` is written by hand; its entry's key carries the
    `peacock.` prefix, since DataFusion 45 adds none to an extension's entries.
  - No `GpuContext::prefill_pinned_cache` and no `Session::prefill_pinned_cache` yet: neither
    has a caller before Task 4b, and either would be a dead-code warning. Task 4b adds the
    `Session` one-liner over `executor::prefill_on` with its first caller.
  - One case more than the plan in the lib's ffi rung, `executor::pinned_cache::ffi_tests::
    the_cache_is_off_without_a_device`: the Rust mapping of `NO_DEVICE` and `OFF`. It is also
    what keeps `apply_pinned_host_cache`, `pinned_host_cache` and `prefill_on` from being dead
    in a test build until Task 4b. Each carries `#[cfg_attr(not(test), allow(dead_code))]`;
    Task 4b turns that into the harness's `not(feature = "test-support")` form.
  - Four CPU cases more than the plan: the process cache's sizing is pure functions
    (`size_pinned_cache`, `describe_pinned_cache`, `parse_pinned_host_cache_bytes`), so the
    environment > session > default order, no-device and integrated, and the first-use line
    are tested without a device.
  - The two GPU gtests are in a new file, `cpp/tests/gpu/test_pinned_scan.cpp`, in
    `peacock_plan_tests`, not in `test_plan_executor.cpp` (already 3046 lines).
  - The scan builds its source from the multi-span constructor whatever the file count; one
    span is the one-file case.
  - A non-UTF-8 path is not refused: `prefill_on` hands a path over as its bytes. Only a NUL
    is refused, as that file's `Io`.
  - `source` is reported as `Env`/`Session`/`Default` in every state, no-device and integrated
    included; `PEACOCK_PINNED_HOST_CACHE_BYTES` is parsed at first use whatever the device.
- **What is subtle.** The first-use line is said at the first scan or prefill, not at the
  first `set` or `stats`: a benchmark applies its session option first, so the line names the
  capacity it will run with. A prefill answers `OFF` before it `stat`s anything, so an off
  cache never reports a missing file. The process cache's mutex keeps a set and the capacity
  it computes together; the cache's own mutex is taken inside it.
- **Both cuDF versions compile**, with no warning from a changed file: 25.02 in `/tmp/pl-build25`,
  26.02 configured fresh in `/tmp/pl-build26` (`scripts/build.sh --configure --build`); `ctest -L cpu`
  and `peacock_cpu_tests` 60/60 in each. This also covers Task 3's 26.02 compile. The 25.02 image
  and `target-img-25.02/` (19 GB) were removed after the 25.02 runs to fit the 26.02 image; dev
  holds the 26.02 build image now, and Task 4b's FFI rung rebuilds its target dir cold.
- **Unrun (no device):** `PinnedHostCache.ScanFromTheCacheEqualsThePath`,
  `PinnedHostCache.PrefilledScanFillsNothing` (`peacock_plan_tests`, listed by
  `--gtest_list_tests`), and `executor::pinned_cache::gpu_tests::{the_session_option_reaches_
  the_cache, prefill_reports_its_files_and_refuses_what_does_not_fit}` (compiled by
  `cargo test --no-run --features gpu`). They run in Task 7, cache on and off.

## Task 4b: the benchmark prefills once per process

- **Red → green.** `cargo test -p peacockdb-core --features rust-only --lib -- benchmark_prefill`
  failed to compile (`E0432 unresolved imports super::ProcessPrefill, super::case_query_file, …`),
  then 5/5. Mutations: reading `--test-threads`' value as a filter, and selecting `NOT_TIMED`
  cases, each fail `selection_follows_libtest_filters`.
- **Departures from the plan's text:**
  - `a_case_outside_the_prefill_is_refused`: the plan expects `unprefilled(&{a, b}, &{a, c}) ==
    Some(c)`. The missing case file is `b`, and the test says `Some(b)`.
  - `selected_benchmark_cases` takes `impl IntoIterator<Item = &BenchmarkCase>`, not a slice:
    `inventory::iter` gives no slice, and `BenchmarkCase` is not `Clone`.
  - Under `--exact` a `--skip` is an equality too, as in libtest (the plan says "containing").
    `--skip=<s>` is read, and `--shuffle-seed` takes a value; one more selection assertion pins
    the `--exact` skip.
  - `process_prefill` leaves out a case whose dataset directory is missing, or whose query panics
    while planning (`catch_unwind`, a stderr line naming it). Without this, a missing tpcds sf40 or
    one query that does not plan fails every case of the run, tpch included; with it, that case
    fails on its own, as build-test.md already says a missing dataset does.
  - The session option is applied after planning, from the first planned case's context; nothing
    reads the cache before the prefill, so the order is the plan's in effect.
  - With no case selected (a selection that misreads libtest), the prefill is `Ok` with no files,
    so the running case fails `prefill: <path> was not prefilled`, also with the cache off.
  - `Session::prefill_pinned_cache` is added here (Task 4 deferred it). The dead-code attributes
    Task 4 left on the cache's Rust items are now `not(feature = "test-support")` on the three FFI
    wrappers and `any(not(feature = "test-support"), feature = "rust-only")` on the types and
    `plan_files`: a rust-only build compiles only `benchmark_prefill`, which leaves variants
    unconstructed.
  - `rounds_were_pinned` runs with the cache off too: an off cache counts no fallback
    (`acquire` returns before counting at capacity 0), so it passes.
  - The `[load-bytes]` sums walk the driver's `PlanIndex`, which is what `RunReport::emitted` is
    indexed by.
  - The gpu-shape compile ran in the 26.02 build image, not 25.02: dev holds one image, and this
    task changes no C++ or ABI, so the cuDF version does not reach the Rust code.
- **What is subtle.** A run under `PEACOCK_BENCH_RESUME=1` still plans and prefills the cases it
  will keep, since they are selected; their bytes count against the limit. The prefill happens in
  the first case's `measure`, before its warm-up, so no case's time includes it. `create_nsys_profile.sh`'s
  metrics pass runs one case per process (`<name> --exact`), so each prefills that case's files
  only; the selection reads `--exact` after the name as libtest does.
- **Unrun (needs a device):** a benchmark run whose `[pinned-cache] prefill:` line shows
  `files > 0` and whose cases all pass `rounds_were_pinned`; the refusal and `[load-bytes]` lines
  on real data. Task 7 Step 3.

## Task 5: the scan reads through a one-chunk chunked parquet reader

- **Red → green.** The 26.02 build image (on dev at the start): `ninja peacock_cpu_tests
  peacock_plan_tests -k 0` failed to compile both new files (`‘ScanReader’ has not been declared
  in ‘peacock’`, `‘parse_scan_reader’ is not a member of ‘peacock’`, `‘read_scan_table’ is not a
  member of ‘peacock’`). Then, in each image, a full `ninja` (exit 0, no warning in the log),
  `ctest -L cpu` 1/1, `peacock_cpu_tests` 63/63, and `peacock_plan_tests --gtest_list_tests`
  131 cases. 26.02 in `/tmp/pl-build26`; 25.02 in `/tmp/pl-build25`, after the 26.02 image was
  removed and the 25.02 image pulled. Mutation: `scan_reader()` reading the variable on every
  call fails `ScanReader.TheSwitchIsReadOnce` (26.02).
- **Departures from the plan's text:**
  - `read_scan_table` returns `ScanRead { table, chunks }`, as the plan's prose says. Its code
    block still shows `table_with_metadata`; the prose and Step 3's `.table` win.
  - The device cases are in a new file, `cpp/tests/gpu/test_scan_reader.cpp`, in
    `peacock_plan_tests`, not in `test_plan_executor.cpp` (Task 4's reason: that file is 3046
    lines). They call `read_scan_table` directly with reader options over `customer.parquet`,
    not through a plan.
  - `ScanReader.TheSwitchIsReadOnce` runs in `peacock_cpu_tests`
    (`cpp/tests/cpu/test_scan_reader_switch.cpp`), so `ctest -L cpu` runs it. It reads the switch,
    sets `PEACOCK_SCAN_READER=read_parquet`, and reads `Chunked` again. It skips if the variable is
    set when it starts.
  - Two CPU cases more than the plan, over a pure `parse_scan_reader(const char*)` (null = unset):
    `ScanReader.TheSwitchNamesTwoReaders` and `ScanReader.AnyOtherValueIsRefusedByName` (`""`,
    `Chunked`, `read-parquet`, `chunked `). The plan's "anything else throws naming it" cannot be
    tested through `scan_reader()`, which reads once. An empty value is refused, not taken as unset.
  - `operators.h` now includes `cudf/io/parquet.hpp` and `cudf/io/types.hpp`, for `ScanRead` and
    the options type.
- **What is subtle.** cuDF's `has_next()` is true once even for an empty read
  (`is_first_output_chunk`, `reader_impl.cpp:818-827` at `v25.02.02`), so the `while (has_next())`
  loop always returns at least one chunk. The lease is taken before `read_scan_table` and dropped
  after it, so it covers the chunked reader's copies as it covered `read_parquet`'s.
- **Unrun (needs a device):** `ScanReader.OneChunkEqualsReadParquet`,
  `ScanReader.SeveralChunksConcatenateToReadParquet` (Review focus 5),
  `ScanReader.RowGroupSelectionIsKept`. They compile in both images and are listed by
  `--gtest_list_tests`. They run in Task 7 with the L40S list, cache on and off. The GPU tiers'
  byte-identity under `PEACOCK_SCAN_READER=read_parquet` is Task 7's too.

## cuDF 25.02 default

Read from `third_party/cudf` at `v25.02.02` (`git show v25.02.02:<path>`; the tag was fetched into
the submodule's object store, its checkout untouched), and from kvikio 25.02.01's headers in the
25.02 build image's `/opt/cudf/include`. Line numbers are the tag's. Each item gives the plan's
expected reading, then the verdict.

- **cuDF's IO host resource is a pinned fixed pool. Confirmed.**
  `cpp/src/utilities/host_memory.cpp:142-164`, `make_default_pinned_mr`: the size is
  `LIBCUDF_PINNED_POOL_SIZE` (147-149), else `config_default_pinned_memory_resource`'s `pool_size`
  (151), else `std::min(total / 200, size_t{100} * 1024 * 1024)` (153-155). That is 100 MiB on an
  80 GB H100 (0.5 % is 400 MB) and on the 46 GB L40S (230 MB). Initial size = maximum (158-159).
  Lines 59-70: an allocation larger than the pool, or one the pool cannot serve, goes to the
  upstream `rmm::mr::pinned_host_memory_resource` (33), one allocation each. That resource is a
  `cudaHostAlloc` per call (read in 26.02's rmm header, `pinned_host_memory_resource.hpp:75`; 25.02's
  rmm header was not re-read). The pool is built on the first `host_mr()` call (191-195), i.e. the
  first pinned allocation.
- **`make_host_vector` is pageable; `hostdevice_vector` is pinned. Confirmed.**
  `host_memory.cpp:293-305`: `allocate_host_as_pinned_threshold` defaults to 0
  (`LIBCUDF_ALLOCATE_HOST_AS_PINNED_THRESHOLD`). `cpp/include/cudf/detail/utilities/host_memory.hpp:41-48`,
  `get_host_allocator`: pinned only if `size * sizeof(T) <= threshold`, so at 0 only an empty vector
  is pinned. `vector_factories.hpp:364-368`, `make_host_vector`, takes that allocator: pageable.
  `cpp/src/io/utilities/hostdevice_vector.hpp:55-62` builds its host side with
  `make_pinned_vector_async` (`vector_factories.hpp:483-487`), which takes
  `cudf::get_pinned_memory_resource()`: pinned. Also `host_memory.cpp:279-284`:
  `kernel_pinned_copy_threshold` is 0, so every pinned copy is a `cudaMemcpyAsync`, never a kernel.
- **kvikio is the default, in compat mode, with 4 threads. Confirmed** (lines 38-65, not 38-63).
  `cpp/src/io/utilities/config_utils.cpp:38-46`: `LIBCUDF_CUFILE_POLICY` defaults to `KVIKIO`.
  55-65, `set_up_kvikio`: `KVIKIO_COMPAT_MODE` defaults to `CompatMode::ON` (59-60) and
  `KVIKIO_NTHREADS` to 4 (62-63).
- **A path read goes to the device through kvikio, not through cuDF's host resource. Confirmed,
  with one addition.** `cpp/src/io/utilities/datasource.cpp:495-515`: a path makes a
  `memory_mapped_source` (`LIBCUDF_MMAP_ENABLED` defaults to `ON`, 499-510). That class (155-246) is
  a `file_source`. `file_source`'s constructor (52-63) opens a `kvikio::FileHandle` under the
  `KVIKIO` policy. `supports_device_read` is then true (91-94), and `is_device_read_preferred` is
  always true with kvikio (96-104). `device_read_async` is `_kvikio_file.pread(dst, …)` (106-116).
  So the mmap serves only `host_read`s (the footer). `cpp/src/io/parquet/reader_impl_preprocess.cu:230-259`:
  adjacent column chunks are coalesced (contiguous, same compression, same source), and each run
  goes through `device_read_async` into a padded `rmm::device_buffer` (252-259).
- **kvikio's compat read: a pinned bounce buffer. Confirmed, but the pieces are 4 MiB, not
  16 MiB.** `kvikio/posix_io.hpp:137-173`, `posix_device_io`: each call takes a bounce buffer from
  `AllocRetain` (144) and loops: `pread` into the bounce buffer, `cuMemcpyHtoDAsync` to the device,
  `cuStreamSynchronize` (156-160). The bounce buffer is `defaults::bounce_buffer_size()`
  (`bounce_buffer.hpp:30-36`), 16 MiB unless `KVIKIO_BOUNCE_BUFFER_SIZE` is set
  (`defaults.hpp:246-253`). Its allocation is in `libkvikio.so`, not the headers. It is pinned:
  the shim's only host allocator is `cudaAPI::MemHostAlloc`, i.e. `cuMemHostAlloc`
  (`shim/cuda.hpp:28-35`), and `libkvikio.so` carries that name. **The correction:**
  `FileHandle::pread` (`file_handle.hpp:225-260`) splits a read into `task_size` tasks on the thread
  pool (`parallel_operation.hpp:55-94`). `task_size` defaults to 4 MiB (`defaults.hpp:212-217`,
  `KVIKIO_TASK_SIZE`). So each `posix_device_io` call gets at most 4 MiB, and each H2D copy is at
  most 4 MiB. The 16 MiB bounce buffer is never more than a quarter used. A read under
  `gds_threshold`, 1 MiB by default (`defaults.hpp:232-237`), skips the pool and runs on the calling
  thread (`file_handle.hpp:231-232`). Within one task the read and the copy do not overlap: each
  piece is read, copied and synced before the next.
- **The host-buffer source. Confirmed; cuDF neither registers nor copies the span, with one
  correction about the sync.** `datasource.cpp:313-338`, `host_buffer_source`, "zero-copy":
  `host_read(offset, size)` returns a `non_owning_buffer` over the span (325-330), and
  `supports_device_read()` is false (332). The base `is_device_read_preferred` returns
  `supports_device_read()` (`cudf/io/datasource.hpp:202-205`): false. So the reader takes the
  `host_read` branch (`reader_impl_preprocess.cu:260-272`): one
  `cudaMemcpyAsync(…, cudaMemcpyDefault, stream)` per coalesced run, straight from the span, on the
  reader's stream and the calling thread. Over a `cudaHostAlloc`'d span that is a pinned DMA.
  Nothing calls `cudaHostRegister` and nothing copies the bytes on the host.
  The single-span constructor is a template, like the multi-span one:
  `source_info(cudf::host_span<T> host_data)`, `T` byte-like (`cudf/io/types.hpp:433-439`), and
  `source_info(host_span<host_span<T>>)` (410-426). `host_span<std::byte const>` works: the reader
  probe uses it, and it compiles against 25.02 and 26.02. `functions.cpp:166` →
  `datasource::create(host_span<std::byte const>)` (`datasource.cpp:523-526`).
  **The sync, corrected:** `read_compressed_data` (`reader_impl_preprocess.cu:1038-1062`) syncs in
  `count_page_headers` (`device_to_host_sync`, 306) only when the file has no page index. With
  a page index in every row group (`_has_page_index`, 1310-1312), `count_page_headers_with_pgidx`
  (328-344) does not sync. The first sync is then the decode's `_stream.synchronize()`
  (`reader_impl.cpp:499`). Either way the stream has synced before `read_parquet` or `read_chunk`
  returns. So a lease held across `read_scan_table` covers the copy in both cases.
- **The chunked reader: a limit of 0 means no limit, one pass, one chunk. Confirmed.**
  `cudf/io/parquet.hpp:539-543` is `chunked_parquet_reader(std::size_t chunk_read_limit,
  parquet_reader_options const&, stream, mr)`, and 564-569 adds `pass_read_limit`. Both document
  `0` as no limit. The two-argument form passes `pass_read_limit = 0` (`functions.cpp:636-643`).
  With `pass_read_limit == 0`, `compute_input_passes` makes one pass over every row group
  (`reader_impl_chunking.cu:1607-1610`) and one subpass (1366). With `chunk_read_limit == 0`, the
  subpass is one output chunk (1689-1693). `reader_impl.hpp:90-92`: `read_chunk()` then returns the
  whole file. `has_next()`/`read_chunk()`: `parquet.hpp:585`, 598. 26.02 (its build image's
  `/opt/cudf/include/cudf/io/parquet.hpp`): the same two constructors (from lines 730 and 779), with the
  same "or `0` if there is no limit", plus overloads that take prebuilt datasources and
  `FileMetaData`. `has_next` is at 831, `read_chunk` at 844.
- **`cudf::io::set_host_memory_resource` does not exist. Confirmed.** No match in `cpp/` at
  `v25.02.02` (`git grep`). No match under the 26.02 build image's `/opt/cudf/include/cudf`. Its
  successors: `cudf::set_pinned_memory_resource` and
  `cudf::config_default_pinned_memory_resource(pinned_mr_options{pool_size})` in
  `cudf/utilities/pinned_memory.hpp` (25.02: lines 32, 45-57; 26.02: 21, 46). Implementations:
  `host_memory.cpp:256-263`, `271-277`. `config_default_…` returns false once a resource exists.

**Conclusion, as expected with one correction.** A path read's bulk H2D is already pinned: kvikio's
bounce buffers. But it moves in pieces of at most 4 MiB (`task_size`), not 16 MiB. It runs from
four kvikio pool threads, each piece after a CPU `pread` out of the page cache, and each piece is
synchronous. Those copies are off the calling thread, so `nsys_calls.py` tallies them as
`(off-thread)`. cuDF's own 100 MiB pinned pool holds `hostdevice_vector`s (chunk descriptors, page
headers), not column bytes. `make_host_vector` stays pageable. From a pinned host span, the copy is
one `cudaMemcpyAsync` per coalesced run, on the calling thread, with no host copy.

## Discrete cards

Read by `devprops` (`cudaGetDeviceProperties`; cudart 12.9 static, built with the 25.02 build
image's nvcc) and `nvidia-smi -q | grep -i 'Addressing Mode'`.

| card | VM | `pageableMemoryAccess` | `pageableMemoryAccessUsesHostPageTables` | `integrated` | Addressing Mode | discrete |
|---|---|---|---|---|---|---|
| `NVIDIA L40S` | nebius-gpu | 0 | 0 | 0 | None | yes |
| `NVIDIA H100 80GB HBM3` | nebius-h100 | 0 | 0 | 0 | None | yes |

- Both cards report `driver=13000 runtime=12090`. Neither card reports `pageableMemoryAccess = 1`,
  so the plan's stop for the human does not apply.
- Read 2026-11-04: the L40S at 07:17:58Z, the H100 at 07:20:10Z. The H100 was read in its own
  short bracket, before the CPU-VM build (the coordinator's decision, under "VM time").
- **The H100 is the kind of host the plan warns about, and still reports 0.** Its kernel is
  `6.11.0-1016-nvidia`, and its driver is the open kernel module, 580.173.02. The plan says such a
  host (open module, kernel ≥ 6.1) reports `pageableMemoryAccess = 1` through HMM. This one does
  not. Why was not checked (nothing else was run on the VM). The attribute, not the
  kernel/module pair, is what `is_discrete_for_pinning` reads, so the predicate is still right here.
- **L40S, first try: no capacity.** On 2026-11-04 the API accepted `scripts/nebius.sh start` at
  06:57:37Z, and the VM stayed `STOPPED` for the whole 900 s `START_TIMEOUT`. In that case
  `nebius.sh` exits 75 (`wait_for_state RUNNING … || exit 75`). The literal 75 was not seen: the
  outer `timeout 900` was the same length and killed the script first (rc 124). So wrap `start`
  in at least `timeout 1000`. `status` at 07:12:52Z: `STOPPED`; `stop` at 07:13:08Z: "already
  STOPPED". The coordinator's retry started it at 07:17:36Z.

## Host memory

- **nebius-h100, `free -b`:** total 211069927424 bytes (196.6 GiB), available 208810188800 at
  07:20Z on a fresh boot. No swap.
- **Read 2026-11-05 02:15Z, after `prepare` and `ship`:** `free -b` total 211069919232. `du -cbL`:
  `tpch.sf40` 42442683465 bytes of parquet (`lineitem` 9030797348), `tpcds.sf40` 15787929533.
  The run image's `memory.max` is `max` with and without `--gpus all` (`run-gpu.sh`'s `docker run`
  sets no `--memory`): no container limit. Default cache: 211069919232 × 3/4 = 158302439424 bytes
  (147.4 GiB). Each dataset fits, and both together (58230612998).
- **Note, the H100's software:** kernel `6.11.0-1016-nvidia`; driver `NVIDIA UNIX Open Kernel
  Module for x86_64 580.173.02` (built with gcc 13.3.0); glibc 2.39 (Ubuntu `2.39-0ubuntu8.9`).
  `devprops` needs `GLIBC_2.34`, so it runs there.

## Before load times

Unprofiled, 2026-11-05 02:16–02:17Z on nebius-h100, from an install of today's engine (see
"Resumed after new-hosts"): `PCK_TEST_FILTER=bench_tpch_sf40_<q>_tp1_single run-gpu.sh … run bench`,
`pull`, the VM's `records.tsv` removed after each. All four green. The results file is aside at
`/tmp/Lsurg/pl-h100/tp1_single.before.txt` on dev, not committed (sf40-baseline owns it).

| query | `run_us` | table | projections | `total_us` |
|---|--:|---|---|--:|
| q1 | 653875 | lineitem | l_quantity, l_extendedprice, l_discount, l_tax, l_returnflag, l_linestatus, l_shipdate | 403877 |
| q3 | 471863 | customer | c_custkey, c_mktsegment | 40065 |
| q3 |  | orders | o_orderkey, o_custkey, o_orderdate, o_shippriority | 52176 |
| q3 |  | lineitem | l_orderkey, l_extendedprice, l_discount, l_shipdate | 333341 |
| q6 | 96796 | lineitem | l_quantity, l_extendedprice, l_discount, l_shipdate | 91920 |
| q8 | 659854 | region | r_regionkey, r_name | 1043 |
| q8 |  | nation | n_nationkey, n_name | 866 |
| q8 |  | nation | n_nationkey, n_regionkey | 739 |
| q8 |  | customer | c_custkey, c_nationkey | 37872 |
| q8 |  | orders | o_orderkey, o_custkey, o_orderdate | 45101 |
| q8 |  | supplier | s_suppkey, s_nationkey | 32362 |
| q8 |  | part | p_partkey, p_type | 44424 |
| q8 |  | lineitem | l_orderkey, l_partkey, l_suppkey, l_extendedprice, l_discount | 447118 |

## Before profile

`create_nsys_profile.sh --trace` per query, same install, 02:17–02:20Z (`PCK_BENCH_HBM_SET=gh100
PCK_BENCH_HBM_PEAK_BW=3.35e12`). Captures in `/tmp/Lsurg/pl-h100/capture-<q>-before.sqlite`; each
`loads-<q>-before.tsv` is re-derived from them by the fixed `nsys_loads.py` (below). Ten executions
each. `host_read_source` is `kvikio` in every capture: 25.02's kvikio pushes its `libkvikio` ranges,
so the trace pass needs no `osrt`. No launch was ambiguous.

**q1** (tpch sf40, tp1-single)

| `recipe_seq` | `executions` | `calls` | `load_us` | `host_read_source` | `host_read_us` | `h2d_count` | `h2d_bytes` | `h2d_us` | `h2d_gbps` | `h2d_pinned_bytes` | `h2d_pinned_gbps` | `h2d_pageable_bytes` | `h2d_pageable_gbps` | `h2d_max_bytes` | `decompress_us` | `decode_us` | `other_device_us` | `off_thread_device_us` | `pinned_alloc_count` | `pinned_alloc_us` | `unattributed_us` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 10 | 1 | 422744 | kvikio | 284977 | 1948 | 1693633748 | 63092 | 26.8 | 1692832228 | 26.8 | 801520 | 27.5 | 4006080 | 39135 | 40146 | 11829 | 0 | 0 | 0 | 50112 |

**q3** (tpch sf40, tp1-single)

| `recipe_seq` | `executions` | `calls` | `load_us` | `host_read_source` | `host_read_us` | `h2d_count` | `h2d_bytes` | `h2d_us` | `h2d_gbps` | `h2d_pinned_bytes` | `h2d_pinned_gbps` | `h2d_pageable_bytes` | `h2d_pageable_gbps` | `h2d_max_bytes` | `decompress_us` | `decode_us` | `other_device_us` | `off_thread_device_us` | `pinned_alloc_count` | `pinned_alloc_us` | `unattributed_us` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 10 | 1 | 41359 | kvikio | 5411 | 120 | 26432232 | 1222 | 21.6 | 26427436 | 21.8 | 4796 | 0.6 | 492584 | 29430 | 4841 | 118 | 0 | 0 | 0 | 2293 |
| 3 | 10 | 1 | 53571 | kvikio | 14321 | 732 | 299761060 | 11287 | 26.6 | 299715236 | 26.6 | 45824 | 6.9 | 1254166 | 31714 | 2177 | 331 | 9832 | 0 | 0 | 5972 |
| 8 | 10 | 1 | 353267 | kvikio | 248329 | 3180 | 1375967160 | 53803 | 25.6 | 1375764664 | 25.6 | 202496 | 12.9 | 1011840 | 54087 | 5688 | 3561 | 0 | 0 | 0 | 42099 |

**q6** (tpch sf40, tp1-single)

| `recipe_seq` | `executions` | `calls` | `load_us` | `host_read_source` | `host_read_us` | `h2d_count` | `h2d_bytes` | `h2d_us` | `h2d_gbps` | `h2d_pinned_bytes` | `h2d_pinned_gbps` | `h2d_pageable_bytes` | `h2d_pageable_gbps` | `h2d_max_bytes` | `decompress_us` | `decode_us` | `other_device_us` | `off_thread_device_us` | `pinned_alloc_count` | `pinned_alloc_us` | `unattributed_us` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 10 | 1 | 98574 | kvikio | 43271 | 614 | 237274556 | 9141 | 26.0 | 237207676 | 26.0 | 66880 | 7.8 | 790232 | 25851 | 3077 | 1367 | 0 | 0 | 0 | 25788 |

**q8** (tpch sf40, tp1-single)

| `recipe_seq` | `executions` | `calls` | `load_us` | `host_read_source` | `host_read_us` | `h2d_count` | `h2d_bytes` | `h2d_us` | `h2d_gbps` | `h2d_pinned_bytes` | `h2d_pinned_gbps` | `h2d_pageable_bytes` | `h2d_pageable_gbps` | `h2d_max_bytes` | `decompress_us` | `decode_us` | `other_device_us` | `off_thread_device_us` | `pinned_alloc_count` | `pinned_alloc_us` | `unattributed_us` |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0 | 10 | 1 | 1376 | kvikio | 20 | 21 | 2351 | 18 | 0.1 | 2195 | 0.2 | 156 | 0.0 | 384 | 13 | 71 | 85 | 0 | 0 | 0 | 1171 |
| 3 | 10 | 1 | 1229 | kvikio | 14 | 21 | 2630 | 18 | 0.1 | 2474 | 0.2 | 156 | 0.0 | 398 | 21 | 74 | 84 | 0 | 0 | 0 | 1023 |
| 5 | 10 | 1 | 1131 | kvikio | 23 | 20 | 3049 | 17 | 0.2 | 2889 | 0.2 | 160 | 0.0 | 480 | 20 | 53 | 74 | 0 | 0 | 0 | 947 |
| 7 | 10 | 1 | 38832 | kvikio | 5631 | 116 | 27932945 | 1268 | 22.0 | 27928177 | 22.1 | 4768 | 1.0 | 492584 | 29445 | 2536 | 99 | 0 | 0 | 0 | 1870 |
| 9 | 10 | 1 | 46556 | kvikio | 8814 | 316 | 187401575 | 6608 | 28.4 | 187382407 | 28.4 | 19168 | 3.5 | 1254064 | 31336 | 1940 | 205 | 6039 | 0 | 0 | 5124 |
| 12 | 10 | 1 | 32230 | kvikio | 437 | 26 | 1865153 | 106 | 17.6 | 1864705 | 18.5 | 448 | 0.1 | 492584 | 28999 | 2058 | 77 | 0 | 0 | 0 | 1145 |
| 14 | 10 | 1 | 45506 | kvikio | 7894 | 154 | 40318301 | 1778 | 22.7 | 40311873 | 22.8 | 6428 | 0.9 | 492584 | 29577 | 6358 | 127 | 0 | 0 | 0 | 2348 |
| 17 | 10 | 1 | 466083 | kvikio | 280387 | 3926 | 4948172976 | 154142 | 32.1 | 4947797648 | 32.1 | 375328 | 21.0 | 1878128 | 125713 | 12912 | 7468 | 100557 | 0 | 0 | 39773 |

- **The copies are pinned, at the pinned DMA's per-copy rate for 4 MiB pieces: 25–32 GB/s** on the
  four `lineitem` loads (`h2d_pinned_gbps`). Pageable bytes are 0.01–0.05 % of `h2d_bytes` there;
  only q8's three tiny loads reach 5–7 %, of about 160 bytes. `h2d_max_bytes` is 4006080: kvikio's
  4 MiB `task_size`, as "cuDF 25.02 default" read from source. Neither the ~10 GB/s pageable rate
  nor the ~55 GB/s rate appears: the copies are pinned but small, synchronous, and each follows a
  `pread`.
- **The copy is 15 % of q1's load, 9–33 % of the four `lineitem` loads** (`h2d_us / load_us`:
  q1 0.149, q3 #8 0.152, q6 0.093, q8 #17 0.331). The most pinning could save,
  `pageable/10 GB/s − pageable/55 GB/s`, is 66 µs on q1 and under 35 µs on every other load.
- **The host read is most of a big load:** `host_read_us` is 67 % of q1's `load_us` (284977 of
  422744), 70 % of q3 #8, 44 % of q6, 60 % of q8 #17. That is the page-cache `pread` into kvikio's
  bounce buffer. It is what the cache (Tasks 3–4) removes.
- **Kernel names.** Every kernel launched in a load window was listed per capture
  (`/tmp/Lsurg/tools/load_kernels.py` over the four captures, by `nsys_loads.classify`). Every
  `nvcomp::unsnap_kernel` is `decompress`, and every `gpuDecode*`, `gpuComputePage*` and
  `gpuBuildStringDictionaryIndex` kernel is `decode`. The `other` time is two kernels:
  `cudf::detail::device_cast<long, __int128>` (the engine's decimal widening after the read, about
  7.6 ms per q1 execution) and cub's `MultiBlockBatchMemcpyKernel` over a `constant_iterator<std::byte>`
  (the reader's buffer memset, about 3.4 ms). Neither is decompression or decode, so the patterns stand.

## Readers from pinned memory (#232)

`DISABLED_ReaderProbe.ReadParquetAgainstOneChunk` on nebius-h100, 2026-11-05 02:20–02:21Z, sf40
`lineitem.parquet` (9030797348 bytes), every row group, 10 runs, `us` the second-fastest, in the
run image with `PEACOCK_RMM_POOL_BYTES=68719476736` (the binary's own 1 GiB pool fails it:
`Maximum pool size exceeded`, as "Task 2 notes" warns):

```
q6 columns (l_shipdate,l_discount,l_quantity,l_extendedprice):
[reader-probe] source=path reader=read_parquet chunks=1 rows=240012290 us=390239
[reader-probe] source=path reader=chunked chunks=1 rows=240012290 us=390298
[reader-probe] source=pinned reader=read_parquet chunks=1 rows=240012290 us=135146
[reader-probe] source=pinned reader=chunked chunks=1 rows=240012290 us=135432
q1 columns (l_quantity,l_extendedprice,l_discount,l_tax,l_returnflag,l_linestatus,l_shipdate):
[reader-probe] source=path reader=read_parquet chunks=1 rows=240012290 us=395900
[reader-probe] source=path reader=chunked chunks=1 rows=240012290 us=395530
[reader-probe] source=pinned reader=read_parquet chunks=1 rows=240012290 us=143463
[reader-probe] source=pinned reader=chunked chunks=1 rows=240012290 us=143466
```

**One chunk does not beat `read_parquet` on the H100, from the path or from pinned memory:** the two
are within 0.2 % (q6: 390239 vs 390298 µs from the path, 135146 vs 135432 from pinned memory). The
source is what moves: pinned memory is 2.9× (q6) and 2.8× (q1) faster than the path.

Under `nsys profile -t cuda,nvtx` (q6's columns; `/tmp/Lsurg/pl-h100/probe-nsys/probe-q6.{nsys-rep,sqlite}`;
per run, split at libcudf's `read_parquet`/`read_chunk` range ends, median of runs 2–10;
`/tmp/Lsurg/tools/probe_totals.py`):

| source, reader | wall µs | kvikio read µs | H2D copies | pinned bytes | pinned µs | pageable bytes | decompress µs | decode µs | other µs |
|---|--:|--:|--:|--:|--:|--:|--:|--:|--:|
| path, `read_parquet` | 427654 | 309690 | 5880 | 1554763808 | 66402 | 437824 | 48597 | 12552 | 2270 |
| path, chunked | 427609 | 309483 | 5880 | 1554763808 | 66165 | 437824 | 48573 | 12732 | 2274 |
| pinned, `read_parquet` | 151537 | 0 | 5880 | 1554763808 | 43619 | 437824 | 48599 | 12563 | 2276 |
| pinned, chunked | 156343 | 0 | 5880 | 1554763808 | 43628 | 437824 | 48569 | 12754 | 2275 |

The same bytes cross in the same 5880 copies (at most 2188480 bytes each) both ways; from the span
they run at 35.6 GB/s instead of 23.4 GB/s, and kvikio's 310 ms of reads are gone. Kernels are
identical across all four. The chunked reader adds nothing on the device.

## Decision

**`NULL_RESULT`.** Over the four queries' loads, the median `h2d_pageable_bytes` share of
`h2d_bytes` is 0.02 % (the most, 6.6 %, on q8 #0's 156 bytes), against `RAISE_THRESHOLD`'s 10 %. Every
load's `pinned_alloc_us` is 0, against `CONFIGURE_POOL`'s 5 % of `load_us`: no `cudaHostAlloc`-family
call starts in any load window. cuDF 25.02 already pins what it copies, through kvikio's bounce
buffer. The cache (Tasks 3–4) and the chunked reader (Task 5) stay in; Task 6 does not run; #149's
pool part closes as misdiagnosed. What costs a load is the page-cache read before each copy, not the
copy's memory kind.

## Task 2 notes

- **devprops** is `/tmp/pl-scratch/devprops.cu` on dev, built in the 25.02 build image with
  `/opt/nvcc/bin/nvcc -ccbin "$CXX" -O2`. cudart is static (`ldd`: libc only); it needs
  `GLIBC_2.34`. It prints `name`, `pageableMemoryAccess`, `pageableMemoryAccessUsesHostPageTables`,
  `integrated`, and the driver and runtime versions. It is a 15-line `cudaGetDeviceProperties` call
  and is rebuilt in seconds if `/tmp` is gone.
- **The reader probe** is `cpp/tests/gpu/test_reader_probe.cpp`, in `peacock_gpu_tests`. Both build
  images compiled it with no warnings from the file (`scripts/build.sh --configure --build`, each
  in its own build dir mounted over `cpp/build`). In both, `--gtest_also_run_disabled_tests
  --gtest_filter='DISABLED_ReaderProbe.*'` with no device and no `PEACOCK_PROBE_FILE` prints
  `SKIPPED … PEACOCK_PROBE_FILE is not set`. The default run does not list it.
  **For Step 6:** `peacock_gpu_tests`' `main()` installs a 1 GiB RMM pool. An sf40 `lineitem` read
  needs far more, so the run sets `PEACOCK_RMM_POOL_BYTES` (for example 60000000000 on the H100).
  `RmmPool.ReservesTheDeclaredBudget` is not in the probe's filter.

## VM time

| VM | start (UTC) | stop (UTC) | hours | why |
|---|---|---|---|---|
| nebius-gpu (L40S) | 2026-11-04 06:57:37 (requested; never RUNNING) | 2026-11-04 07:13:08 (`stop`: already STOPPED) | 0 | Task 2 Step 3, `devprops`; no capacity, see "Discrete cards" |
| nebius-gpu (L40S) | 2026-11-04 07:17:36 (coordinator's retry; RUNNING) | 2026-11-04 07:18:37 (`stop`: stopped) | 0.02 | Task 2 Step 3, `devprops` and addressing mode |
| nebius-h100 | 2026-11-04 07:18:44 (RUNNING 07:19:49) | 2026-11-04 07:20:49 (`stop`: stopped) | 0.03 | Task 2 Step 3, `devprops`, addressing mode, `free -b`, kernel/driver/glibc; its own bracket, before the CPU-VM build |
| nebius-cpu128 | 2026-11-05 01:51:00 (RUNNING 01:52:09) | 2026-11-05 02:00:20 (`stop`: stopped) | 0.16 | Task 2 Step 3, the CPU-VM build of today's engine (`436b3dd8`'s tree) |
| nebius-h100 | 2026-11-05 02:01:21 (RUNNING 02:02:21) | 2026-11-05 02:22:50 (`stop`: stopped) | 0.36 | Task 2 Steps 3–6: `prepare` (11 min), `ship`, host memory, before load times, before profile, the probe and its Nsight run |
- L40S start for Task 2 Step 3's devprops: no capacity from 2026-11-04T06:57:37Z (developer; treated as 75). The coordinator retries every 15 min up to 10 h (until 16:57Z). Decision: the H100's devprops is read in its own short bracket before the CPU-VM build exists, so Tasks 3–5 (offline) can start once both cards are read; one extra H100 start of a few minutes, against the plan's order (Step 3 reads it after the build).
- Order change (coordinator, 2026-11-04): Tasks 3, 4, 4b and 5 (offline, tested on no VM) run now, before Task 2's before profile, because the H100 path waits on new-hosts' block. The before profile and the before load times (Task 2 Steps 4–8) are then taken on an install staged from `ENS-new-hosts`'s tip (today's engine, no pinned-loads change), never from this branch. Task 6 waits on Task 2's Decision.

## Blocked (2026-11-04)

`blocked(building)`: the H100 path is not live. Task 2 Steps 3–9 (the before profile on an install
staged from `ENS-new-hosts`'s tip), Task 6 (waits on Task 2's Decision), Task 7 (the H100 and L40S
after-runs) and Task 8 need "The CPU-VM build", which waits on new-hosts' block (the human's
GitHub PAT; `new-hosts-detail.md`, "Blocked"). The analyst spent on new-hosts' obstacle named this
task's H100 work as hitting the same block. Done offline: Tasks 1, 2 Steps 1–2 (+ both cards'
attribute), 3, 4, 4b, 5. Device-only gtests and `gpu_tests` cases are compiled, not run (Task 7).
Once new-hosts' Task 10 has proven the H100 path, clear this block and resume at Task 2 Step 3.

## Rebase onto master (2026-11-04)

Chain L's rebase: 9 commits onto `ENS-new-hosts` at `4b636cc3` (itself on master with ENS-optimizer,
#165). No code conflict. The replay took HEAD's side on `build-test.md`, which lost five pinned-loads
edits. They are restored by meaning: the ffi rung's `Pinned host cache wrappers` row (module unit, 1);
the four summary rows pinned-loads changed (Calibration 41→51, C++ CPU/FFI unit 26→63, cuDF GPU smoke
3→5, Plan-executor 126→131), taken whole, since HEAD's copies still equalled the old base; and the
headers, recounted from the rows: cpu 3427 (`--lib` 930), gpu 2411 (`gpu_tests::` 742), grand total
7730 (Rust 5976, C++ 277, Python 1477). Every other pinned-loads line had survived.

Re-proven offline on this base, no VM:
- `cargo test --features rust-only -p peacockdb-core`: exit 0, no warning. `--lib` 928 + 2 ignored,
  `test_cpu_corpus` 2447, `test_corpus_goldens` 47, `test_cost_model` 3, `test_crate_privacy` 1,
  `test_golden_format` 41. `git status --short testdata/` empty before and after.
- `cargo test -p cost-report`: 49 + `test_source_formatting` 32, exit 0.
- `scripts/calibration/tests/test_*.py` in the 25.02 build image's venv: 6+3+14+10+7+11 = 51,
  `test_loads.py` 10/10. On the host `test_plot.py` dies importing matplotlib, which only the venv holds.
- C++, 25.02 build image, fresh `/tmp/pl-rb25` (`scripts/build.sh --configure --build`): 99/99, two
  warnings, both in `_deps/flatbuffers`. `ctest -L cpu` 1/1; `peacock_cpu_tests` 63/63;
  `peacock_plan_tests` lists 131. The binary reads `testdata/fixtures` by absolute path, so a
  `docker run` without the worktree mounted fails `ClampRowRange.AnswersEveryCaseInTheSharedTable`.
- Host cuDF 25.02 (`rapids-cuda-12.2`, `scripts/cargo-cudf.sh`, `LD_LIBRARY_PATH` on the env's
  `lib/`), no device: `--lib -- ffi_tests::` 5/5 and `peacockdb-ffi --test test_ffi` 4/4, both
  no-device pinned-cache cases among them; `--features gpu --no-run` builds every target, no warning,
  and its `--lib --list` holds 742 `gpu_tests::` cases.
- 26.02 not rebuilt: the build image is not on dev (only its run image is).

Unchanged: the device cases and the H100 path wait, as "Blocked" says.

## Blocked (2026-11-04, after the rebase)

Still `blocked(building)` at Task 2 Step 3: the H100 path waits on new-hosts' new block, the two
VMs' disks (`new-hosts-detail.md`, "Blocked (2026-11-04, second)"); the PAT block is cleared.
Resume at Task 2 Step 3 once new-hosts' Task 10 has proven the H100 path.

## Resumed after new-hosts (2026-11-05)

- **The second rebase**, onto `596261f8` (new-hosts done): 10 commits, no code conflict.
  `build-test.md` lost three pinned-loads rows again (C++ CPU/FFI unit, cuDF GPU smoke,
  Plan-executor), taken whole since HEAD's equalled the base, and the total. The Nsight paragraph
  keeps new-hosts' text: the metric set's default is `gh100` (`create_nsys_profile.sh:75`), so only
  the peak needs setting. Total recounted from the rows: 7739 (Rust 5976, C++ 277, Python 1486).
  Re-proven: `cargo test --features rust-only -p peacockdb-core` exit 0 (`--lib` 928 + 2 ignored,
  2447, 47, 3, 1, 41), `cargo test -p cost-report` 49 + 32, `git status --short testdata/` empty.
- **Today's engine is `436b3dd8`'s tree, not `ENS-new-hosts`' tip.** `git diff 596261f8 436b3dd8`
  touches no engine source: only scripts, the wiki and the reader probe (`test_reader_probe.cpp`,
  one `CMakeLists.txt` line adding it to `peacock_gpu_tests`). Step 6 needs that probe, so the
  install is that tree: `git archive 436b3dd8` plus the worktree's `third_party/cudf` (at the
  submodule's pinned `43505bb9`; CMake includes its `cmake/`), rsynced as `nebius-sync.sh` does to
  `~/peacockdb-pl-before` on the CPU VM, built by `stage-install.sh` (rc 0, 5.3 min; the two
  warnings are flatbuffers' own), and pulled to `/home/dmitry/build/peacock/pinned-loads/before-install`.
  Checked there: the probe's name is in `peacock_gpu_tests`; `peacock_pinned_cache_prefill` is not in
  `libpeacock_gpu.so`.
- **`nsys_loads.py` read pinned as pageable (fixed).** It took `srcKind` in CUPTI's numbering
  (1 pageable, 2 pinned). The export renumbers it: its `ENUM_CUDA_MEM_KIND` table has 0 pageable,
  1 pinned, 2 device. So every pinned copy counted as pageable and every pageable one as neither.
  Seen on the probe's pinned span, whose copies came out "pageable". Now the ids are read off that
  table by name, and a capture with copies but without it is refused. The fixture writes the table
  as an export does. Red first: `test_load_row_splits_copy_from_decode` showed the swap, and the two
  new tests, `test_memory_kinds_are_read_off_the_export_s_enum` (the ids moved) and
  `test_capture_without_the_memory_kind_enum_is_refused`, failed; then 12/12, and the calibration
  suite 53/53 in the venv. Every `loads-<q>-before.tsv` above is re-derived from its capture with the
  fix. No re-profile.
- **`nsys_calls.py` refuses any capture of a query with a join** (not this task's file; not fixed).
  Since join-backend (`4b75f649`) a join's recipe is `join_build(#N CudfJoin)`, and
  `recipe_seqs`' pattern, `execute_\w+\(#(\d+) (\w+)`, declares nothing for it. The capture names
  those calls `N.0 join_build` and `N.1 join_probe`. So q3 (`[5, 10]`) and q8 (seven seqs) fail
  "driven but not declared", and `create_nsys_profile.sh --trace` stops before `nsys_loads.py`. Their
  load breakdowns were derived from the pulled captures by hand. The refusal listed nothing missing
  and no kind differing, so the captures are of the planned queries. The fix needs a pattern for
  `join_build` and a rule mapping `join_build`/`join_probe` onto `CudfJoin`. It is calibration's
  (bp-benchmarks'), and it blocks any Nsight trace of a join query, sf40-baseline's included.
