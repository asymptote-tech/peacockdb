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
- **Not read yet:** the bytes of `tpch.sf40` and `tpcds.sf40` parquet (`du -cb`), the run
  container's limit (`memory.max` inside `run-gpu.sh`'s `docker run`), the default cache and
  whether each dataset fits. They wait for the H100 path: new-hosts' fetch and the CPU-VM build.
- **Note, the H100's software:** kernel `6.11.0-1016-nvidia`; driver `NVIDIA UNIX Open Kernel
  Module for x86_64 580.173.02` (built with gcc 13.3.0); glibc 2.39 (Ubuntu `2.39-0ubuntu8.9`).
  `devprops` needs `GLIBC_2.34`, so it runs there.

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
- L40S start for Task 2 Step 3's devprops: no capacity from 2026-11-04T06:57:37Z (developer; treated as 75). The coordinator retries every 15 min up to 10 h (until 16:57Z). Decision: the H100's devprops is read in its own short bracket before the CPU-VM build exists, so Tasks 3–5 (offline) can start once both cards are read; one extra H100 start of a few minutes, against the plan's order (Step 3 reads it after the build).
- Order change (coordinator, 2026-11-04): Tasks 3, 4, 4b and 5 (offline, tested on no VM) run now, before Task 2's before profile, because the H100 path waits on new-hosts' block. The before profile and the before load times (Task 2 Steps 4–8) are then taken on an install staged from `ENS-new-hosts`'s tip (today's engine, no pinned-loads change), never from this branch. Task 6 waits on Task 2's Decision.
