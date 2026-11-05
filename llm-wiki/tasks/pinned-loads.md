# Parquet loads read table files kept resident in pinned host memory, through the chunked reader

Kind: production

**This task closes [#149](../tickets/performance.md#t149) (pinned host memory) and [#232](../tickets/performance.md#t232) (chunked parquet reader).** Nothing in the
engine sets cuDF's IO host memory resource, so every parquet load goes through pageable host
memory. On the H100 this chain benchmarks on, measured 2026-11-02 with a standalone CUDA probe
(4 GiB, 10 runs): H2D is 55.5 GB/s from pinned memory against 10.0 GB/s from pageable memory,
and a kernel reading mapped pinned pages directly gets 51.4 GB/s. On sf40 the load is 400-690 ms
against 19-48 ms of execute (`reports/benchmark-minimal.md`), so the load is the number every
later task in this chain measures. It has to be the fast one before the baseline is taken.

**Why cuDF's pinned resource alone cannot do it.** `scan.cpp` reads file paths. Without GDS (none
on the Nebius VMs), cuDF reads through kvikio: the file's bytes come from the kernel's page cache,
which is pageable and cannot be pinned, and are copied by the CPU into pinned bounce buffers before
the DMA. So a pinned resource still leaves a host copy of every byte. Reading pinned host pages
means the file's bytes must already sit in pinned host memory when the read starts. This is also
the fair comparison: DuckDB's warm numbers read from its own buffer manager in RAM.

**And the reader is the second known cost.** #232: `cudf::io::read_parquet` loaded q6 in 340 ms
where `chunked_parquet_reader` with one chunk took about 60 ms (GB10, 26.02). Both fixes land here,
before any baseline, so no later number carries a known load bug.

**What the existing results say (H200, `h200/tpch.sf40/tp1_single.benchmark.txt`, sizes scaled from
sf1's parquet footers ×40):**

| case | compressed bytes read | `GpuLoadParquet` | rate | the copy at 10 GB/s pageable | at 55 GB/s pinned |
|---|--:|--:|--:|--:|--:|
| aggregate-groupby (`l_quantity`, `l_returnflag`) | ~212 MB | 101 ms | ~2.1 GB/s | ~21 ms | ~4 ms |
| filter-project (`l_orderkey`, `l_quantity`) | ~1.35 GB | 347 ms | ~3.9 GB/s | ~135 ms | ~25 ms |

The load runs at 2-4 GB/s of compressed parquet, well below even the pageable link rate. Most of it
is decoding and per-row-group work across 1953 row groups, not the copy. So pinned memory can save at
most the copy's share: about a fifth to a third of the load, if the copy is not already overlapped
with decoding. cuDF may also already default its IO host resource to a pinned pool in 25.02. That
has to be checked in the cuDF source before anything is changed.

## Done when

**First, before any engine change: profile the load and see where its time goes.** The Nsight
tooling (`scripts/create_nsys_profile.sh`, `scripts/calibration/nsys_calls.py`) today gives each cuDF
call one `device_us`. That sums kernels and memcpys, so a load's copy is indistinguishable from its
decode. This task adds the split, and every later profile in the chain uses it.

A. **A load breakdown from the trace capture**, one row per `GpuLoadParquet` call per case:
   - host file read: OS-runtime or kvikio ranges, whichever the capture can see;
   - H2D copies: bytes, time, GB/s, and pageable or pinned, from CUPTI's memcpy source kind;
   - decompression kernels and decode kernels, each with its time;
   - the rest, as unattributed.

   The parquet reader's device work may launch off the calling thread, which `nsys_calls.py`
   currently tallies as `(off-thread)`. The breakdown attributes it to the load. It is written
   to a new derived file beside `calls.tsv` (`loads.tsv`, or columns in `calls.tsv`; the
   builder's choice), is documented in `build-test.md` "Benchmark data flow", and has a test over
   a synthetic capture, as `scripts/calibration/tests/` does.
B. **The before profile:** a trace of sf40 tp1_single TPC-H q1, q3, q6 and q8 on the H100, with
   today's engine. The detail file carries its breakdown, and says whether the copies run at the
   pageable rate (~10 GB/s) or at the pinned rate (~55 GB/s).
C. **The after profile:** the same four, after item 1. **The H2D copies must reach close to the
   probe's 55 GB/s pinned rate**, read from the profile, not inferred from the load's total. If they
   do not, the gap is explained before the task moves on. A copy too small to reach link rate is an
   explanation, with its size stated.

0. The task's detail file states what cuDF 25.02 does by default (pageable or a pinned pool, and
   its size, and how a path read reaches the device: kvikio, compat mode, bounce buffers), with
   the source lines.
0a. **A pinned host file cache, the engine's default.** The engine keeps each parquet file it reads
   resident in pinned host memory (`cudaHostAlloc`, the whole file), loaded on first use and
   reused by every later read in the process. Scans read it through a host-buffer source
   (`cudf::io::source_info` over the pinned span), not the path. It is on by default on a discrete
   GPU and off on an integrated one (the `pageableMemoryAccess` branch of item 1), and off when
   the process sees no CUDA device. A session option
   bounds it: `pinned_host_cache_bytes`, default **three quarters of min(host RAM, the cgroup
   memory limit when one is set)** (the human's decision, 2026-11-03; about 147 GB on the H100 VM),
   `0` = off. It is a hard ceiling: the cache never grows past it. A file
   that does not fit is read from its path as today, and the run says so once. Eviction is by
   least recent use, and only between calls: no read sees its buffer freed. A changed file (size
   or mtime) is reloaded. The cache's first load of a file is host IO; its time is reported apart
   from the load the device does (bench-rows' `load_us` is the device-side load).
   **The benchmark prepares the pinned buffers once, for every round** (the human's decision,
   2026-11-03). Before its first case, a benchmark process fills the cache with every file its
   cases read, through an explicit engine call (prefill), and keeps it for the whole process: all
   warm-up and measured rounds of all cases read the same pinned buffers. No measured number
   depends on the page cache: the prefill is the only read from disk, and the run records its
   bytes and time. If the files do not fit `pinned_host_cache_bytes`, the run refuses at prefill,
   naming the size, rather than measuring some loads from the page cache.
   **The prefill call** (the human's choice, 2026-11-03). The cache lives on the C++ side, behind
   one additive C symbol:

   ```c
   typedef struct { uint64_t files, bytes, us; uint64_t needed, limit; } peacock_prefill_stats_t;
   int peacock_pinned_cache_prefill(peacock_executor_t* executor,
                                    const char* const* paths, size_t n_paths,
                                    peacock_prefill_stats_t* out);
   ```

   - It makes every named file resident, whole, from disk.
   - A file already resident and unchanged costs nothing, so the call is idempotent.
   - It refuses before pinning anything when the total exceeds `pinned_host_cache_bytes`, with
     `needed` and `limit` set.

   The Rust side wraps it as `prefill_pinned_cache(&self, files: &[PathBuf]) -> Result<PrefillStats,
   PrefillError>`, where `PrefillError` is one of `DoesNotFit { needed, limit }`,
   `Io { path, error }` or `Off`. The files come from the plans: `plan_files(roots) ->
   BTreeSet<PathBuf>` collects every `GpuLoadParquet`'s file over a tree or a fused DAG. The
   benchmark plans its cases, prefills their union once, then runs every round. The trailer
   records `prefill_files`, `prefill_bytes` and `prefill_us`. A refusal is the case's
   `status=failed reason=prefill: needs <needed> > limit <limit>`. The cache is per process, not
   per device.
0b. **The scan reads through `cudf::io::chunked_parquet_reader`** with one chunk, its read limit
   above the batch the plan already sized, concatenating only if several come back (#232's
   proposed fix). It applies to both sources, the cached span and the path fallback. The H100
   re-measure #232 asks for comes first: `read_parquet` against the one-chunk chunked reader over
   the same row groups, from the pinned cache, in the before and after profiles.
1. Unless cuDF already pins, the engine installs a pinned host memory resource for cuDF IO
   (`cudf::set_pinned_memory_resource` / `cudf::config_default_pinned_memory_resource`, `pinned_memory.hpp`; the ticket's `cudf::io::set_host_memory_resource` does not exist in 25.02 or 26.02) on a discrete GPU, once per process, before the first
   parquet read, and leaves cuDF's default alone on an integrated part. The branch is the
   device's `pageableMemoryAccess`, as the ticket says. An HMM host can report 1 on a discrete
   card, so the attribute is first recorded on the H100 and the L40S; if a discrete card reports
   1, the human decides the branch before any engine change. The test binaries and the benchmark
   binary get it through the engine, not through a copy of the call.
2. On the H100, the detail file records before and after `GpuLoadParquet` times for sf40
   tp1_single TPC-H q1, q3, q6 and q8, with the copy's share estimated as above. A gain smaller
   than that share is a finding, explained, not a failure. If cuDF already pins, or the gain is
   noise, the task records that. The cache and the chunked reader (items 0a, 0b) are measured each
   on its own as well as together, so each fix's share is known. No device answer changes: the GPU
   tiers stay byte-identical, with the cache on and with it off (`pinned_host_cache_bytes = 0`).
3. The pinned pool's size is a bounded, named constant or option. Its host-memory cost is written
   in `architecture.md` "cuDF options". It must not pin all of host RAM. The H100 VM has 196 GB.
4. **The completeness signoff reports the load rate with the new reader**, on the H100 from the
   pinned cache, for sf40 tp1_single TPC-H q1, q3, q6 and q8 at least. Per query:
   - the compressed bytes read and the decoded bytes produced;
   - `load_us`;
   - the rate in GB/s of each;
   - the H2D copy rate read from the profile.

   Beside them: the same rows for `read_parquet` from the path (before), for `read_parquet` from
   the cache, and for the chunked reader from the path, so each fix's share shows. Plus the
   prefill's bytes and time per dataset.
5. #149 and #232 move to the archive, with the measured numbers, whichever way they fall. The
   benchmark harness and the fused runs both get the cache through the engine; neither installs
   its own.

## Scope

| path | change |
|---|---|
| `cpp/src/` (where the engine sets up a device, next to where #148's RMM resource would go) | install the IO host resource; the pinned host file cache |
| `cpp/src/operators/scan.cpp` | the host-buffer source and the one-chunk chunked reader |
| `peacockdb-core/src/` session options, the C ABI | `pinned_host_cache_bytes`, additive |
| `cpp/tests/gpu/` | a case asserting the resource is pinned on a discrete device |
| `scripts/create_nsys_profile.sh`, `scripts/calibration/` (`nsys_calls.py` or a sibling, `tests/`) | the load breakdown; the H100's HBM settings (`PCK_BENCH_HBM_SET`, `PCK_BENCH_HBM_PEAK_BW`) |
| `llm-wiki/build-test.md` | the breakdown's format |
| `.gitignore` | `testdata/calibration/loads.tsv`, one host's derived file |
| `llm-wiki/architecture.md`, `llm-wiki/tickets/performance.md`, `llm-wiki/archive/archived-tickets.md` | the option and its cost; #149 and #232 closed |

Component-level API: one additive session option, `pinned_host_cache_bytes`, through the existing
options path to the C side; three additive C symbols and their Rust wrappers:
- `peacock_pinned_cache_prefill`;
- `peacock_set_pinned_host_cache_bytes`, which hands the option to C++ before the first prefill or
  read;
- `peacock_pinned_host_cache_stats`, the counters the per-section check and the trailer's
  `pinned_cache=` read.

No other symbol changes.

## Constraints

- No GB10 is available to this chain. The integrated branch is covered by a unit case on the
  branch condition, and the ticket's "integrated does not regress" measurement is recorded as
  deferred.
- Host memory: the cache and cuDF's pinned pool together stay well under the host's RAM. On the
  H100 VM (196 GB) the TPC-H and TPC-DS sf40 files a batch reads must fit, and the detail file
  records the bytes pinned per dataset.
- The H100 is reached through new-hosts' tooling, which is why new-hosts comes first in the
  chain.

## Signoff (2026-11-05)

Solved under its constraints: the pinned host file cache and the one-chunk reader, measured alone and
together on the H100 (`lineitem` loads 40–64% faster with the cache; readers equal within 0.2%); the
L40S list green with the cache on and off; #149 and #232 closed, #305 opened; the load-rate table is
in #149's archive entry. Shortcuts: Task 6 skipped (NULL_RESULT: cuDF already pins); the
`prefill_*` trailer keys belong to bench-rows (stderr only here); `pinned_host_cache_bytes` is
applied by the benchmark harness only, since no engine path opens a device run yet; the device tiers
never ran under `PEACOCK_SCAN_READER=read_parquet`; the signoff's compressed bytes are the profiles'
`h2d_bytes`; q3/q8 load breakdowns derived by hand (`nsys_calls.py` join gap, routed to sf40-baseline).
