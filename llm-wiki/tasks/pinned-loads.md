# The parquet load reads into pinned host memory on a discrete GPU

Kind: production

**This task closes [#149](../tickets/performance.md#t149) (pinned host memory).** Nothing in the
engine sets cuDF's IO host memory resource, so every parquet load goes through pageable host
memory. On the H100 this chain benchmarks on, measured 2026-11-02 with a standalone CUDA probe
(4 GiB, 10 runs): H2D is 55.5 GB/s from pinned memory against 10.0 GB/s from pageable memory,
and a kernel reading mapped pinned pages directly gets 51.4 GB/s. On sf40 the load is 400-690 ms
against 19-48 ms of execute (`reports/benchmark-minimal.md`), so the load is the number every
later task in this chain measures. It has to be the fast one before the baseline is taken.

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
   its size), with the source lines.
1. Unless cuDF already pins, the engine installs a pinned host memory resource for cuDF IO
   (`cudf::set_pinned_memory_resource` / `cudf::config_default_pinned_memory_resource`, `pinned_memory.hpp`; the ticket's `cudf::io::set_host_memory_resource` does not exist in 25.02 or 26.02) on a discrete GPU, once per process, before the first
   parquet read, and leaves cuDF's default alone on an integrated part. The branch is the
   device's `pageableMemoryAccess`, as the ticket says. The test binaries and the benchmark
   binary get it through the engine, not through a copy of the call.
2. On the H100, the detail file records before and after `GpuLoadParquet` times for sf40
   tp1_single TPC-H q1, q3, q6 and q8, with the copy's share estimated as above. A gain smaller
   than that share is a finding, explained, not a failure. If cuDF already pins, or the gain is
   noise, the task records that null result, and #149 closes as misdiagnosed. Reworking the read
   path (a host-buffer source, kvikio settings, the chunked reader) is out of scope. It gets a
   ticket if the numbers point there. No device answer changes: the GPU tiers stay
   byte-identical.
3. The pinned pool's size is a bounded, named constant or option. Its host-memory cost is written
   in `architecture.md` "cuDF options". It must not pin all of host RAM. The H100 VM has 196 GB.
4. #149 moves to the archive, with the measured numbers, whichever way they fall.

## Scope

| path | change |
|---|---|
| `cpp/src/` (where the engine sets up a device, next to where #148's RMM resource would go) | install the IO host resource |
| `cpp/tests/gpu/` | a case asserting the resource is pinned on a discrete device |
| `scripts/create_nsys_profile.sh`, `scripts/calibration/` (`nsys_calls.py` or a sibling, `tests/`) | the load breakdown; the H100's HBM settings (`PCK_BENCH_HBM_SET`, `PCK_BENCH_HBM_PEAK_BW`) |
| `llm-wiki/build-test.md` | the breakdown's format |
| `llm-wiki/architecture.md`, `llm-wiki/tickets/performance.md`, `llm-wiki/archive/archived-tickets.md` | the option and its cost; #149 closed |

Component-level API: none, unless the pool size becomes a session option. If it does, that is one
additive field.

## Constraints

- No GB10 is available to this chain. The integrated branch is covered by a unit case on the
  branch condition, and the ticket's "integrated does not regress" measurement is recorded as
  deferred.
- The H100 is reached through new-hosts' tooling, which is why new-hosts comes first in the
  chain.
