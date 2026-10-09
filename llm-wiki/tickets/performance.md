
# Performance

These issues should be fixed on a pre-prod performance path.

## Contents

- [#150 (uncompressed embeddings) store the embedding columns uncompressed; Snappy costs a third of a vector query to save 3%](#t150)
- [#149 (pinned host memory) the parquet load must use pinned host memory](#t149)
- [#148 (RMM allocator) the engine installs no RMM allocator, and `gpu_memory_limit` is accepted and ignored](#t148)
- [#231 (26.02 big aggregate) one grouped aggregate call past about 100M rows is several times slower on libcudf 26.02](#t231)
- [#232 (chunked parquet reader) the scan reads through `read_parquet`, which is several times slower than the chunked reader](#t232)
- [#242 (26.02 join paths) the join session calls only the cuDF API that 25.02 and 26.02 share, and leaves 26.02's faster joins unused](#t242)
- [#248 (join parallelism gaps) three join shapes run on one lane or skewed after the rewrite](#t248)
- [#286 (offset row-group skip) an OFFSET that reaches a scan still reads every row group it skips](#t286)

<a id="t150"></a>
### #150 (uncompressed embeddings) store the embedding columns uncompressed; Snappy costs a third of a vector query to save 3%
The sf40 embedding columns are written SNAPPY and do not compress: `ps_image_embedding`
12306/12661 MB and `p_text_embedding` 3205/3293 MB, both 1.03x against ~1.6x elsewhere.

Float32 embeddings are high-entropy, so that is the data rather than the writer — and the GPU
decompresses them anyway. On q11v (`nsys`, share of GPU kernel time) `nvcomp::unsnap_kernel` is
564.7 ms / 37.9% on H200 and 419.5 ms / 24.9% on GB10, against 60.5 ms / 4.0% for the cuVS
distances and top-k the query exists to do; loading is 93.8% of H200 kernel time.

The change is `compression=NONE` for those two columns in `testdata/generate_testdata.sh`.
Parquet compression is lossless, so no value changes and no golden moves — only file size (~440
MB more) and the load path. Not free to do, though: sf40 is generated, uploaded and mirrored to
shad-gpu, so it means re-uploading 40 GB and re-verifying the 16 sf40 goldens. Measure with
`load_ms` per vector probe and the `unsnap` line from `nsys stats`.


<a id="t149"></a>
### #149 (pinned host memory) the parquet load must use pinned host memory
**Priority: high**

Nothing in the engine sets a host memory resource for IO, so parquet loads from pageable host
memory — 10.6 GB/s H2D on the H200 against 47.3 GB/s pinned, 2 GiB buffers, 2nd-min of 5.

A discrete GPU's DMA engine transfers by physical address and cannot be handed a page the OS may
move, so a pageable source is bounced through an internal pinned staging buffer: a host memcpy
of every byte, which is what bounds the rate — H200's 10.6 GB/s sits just under its 11.8 GB/s
single-core memcpy, nowhere near its link rate. That is 4.4x on every byte the loader moves, and
the load dominates: 400-690 ms against 19-48 ms of execute on sf40. cuDF exposes
`cudf::io::set_host_memory_resource` and defaults to pageable — [#148](#t148)'s shape one side
over. Condition it on the device: GB10 shows 59.5 vs 59.2 because it has one physical pool, so
`pageableMemoryAccess` is the branch. Tests: compare the existing `[bench] … load_ms=` on both
hosts, asserting the discrete host improves and the integrated one does not regress.

<a id="t148"></a>
### #148 (RMM allocator) the engine installs no RMM allocator, and `gpu_memory_limit` is accepted and ignored
**Priority: high**

Nothing under `cpp/src/` or `cpp/include/` calls `set_current_device_resource`, so every cuDF
intermediate takes rmm's default: a `cudaMalloc`/`cudaFree` driver round trip each.

Measured on GB10, whole-table q6 load (`reports/benchmark-minimal.md`, `reports/dgx-spark.md`):
337-353 ms over a pool against 850-857 ms with none, so the pool is worth about 2.5x; a pool
forced to grow mid-query costs 1762-1777 ms, 5x against. Reserving 60% up front changes nothing
at the default size. The q1 figure this ticket used to cite, 76.5 s whole-table against 3.9 s
streamed, ran over a pool and is libcudf 26.02's groupby cliff, #231. The gap was fixed three
times already — `multi_gpu.cpp`, the gtest mains and the
benchmark harness, the last two sharing `cpp/include/peacock/rmm_pool.hpp`
([#151](archive/archived-tickets.md#t151)); the engine is the only one of the four that ships.
The second half is the same fix: `gpu_memory_limit` is documented as a bound, stored at
`gpu_executor.cpp:99` and never read — the #132 shape one level up. Care: install per device
before any cuDF call, and tear down on the owning thread (`set_per_device_resource(id, nullptr)`
misses the ref map). Two questions #178 did not answer for the engine: whether the limit is a
reservation or a ceiling — the test binaries take `initial == maximum` because it fails loudly,
and the 5x growth cost says reserve it up front — and how an integrated part is sized, whose only implementation went with the percentages
(`archive/historical-comments.md`). Tests: the GPU tiers stay byte-identical, plus a case
asserting a small limit is honoured.

<a id="t231"></a>
### #231 (26.02 big aggregate) one grouped aggregate call past about 100M rows is several times slower on libcudf 26.02

libcudf 26.02's hash groupby is linear up to 100M rows and then jumps: 100M to 200M rows costs
4.62x on H200 and 61.4x on GB10, where a linear cost is 2.0x and 25.02 gives 1.86x.

Measured in `reports/benchmark-minimal.md` (the cliff section) and `reports/dgx-spark.md`, on an
8-aggregate groupby over the same data:

| configuration | 100M | 200M | ratio |
|---|--:|--:|--:|
| H200, libcudf 25.02 | 74.9 ms | 139.4 ms | 1.86x |
| H200, libcudf 26.02 | 60.5 ms | 279.2 ms | 4.62x |
| GB10, libcudf 26.02 | 199.6 ms | 12262.1 ms | 61.4x |

Both report four output groups at 200M, so it is not a change in group count; 26.02 is faster
than 25.02 on some shapes at 50M, so it reads as a changed strategy. Whole-table q1 on GB10 takes
73.5 s, 98.9% of it one `single_pass_shmem_aggs_kernel` launch; streamed in 29 batches of about
8M rows it takes 1.0 s. One axis stays confounded: shad-gpu runs CUDA 12.2, sparkdgx CUDA 13.0.
The engine reaches it: `Batching::Off` (`plan/mod.rs`) hands a lane its whole chunk, so an sf40
lineitem aggregate at a tp1 mode is one call over 240M rows. CI builds a 26.02 leg. This is what
#148's q1 figure measured, not the missing pool.

**Corpus queries:** none at sf1, where no lane holds 100M rows. At sf40, `tpch/q1` at `tp1-single`
on a 26.02 build.

**Fix proposed:** keep every grouped aggregate call below the step on 26.02: cap the rows per
init call, splitting a larger batch or sizing the source's batches under it, since the merge
above already folds several partial states. The cap is a measured number per library version,
named where it is set, not a batch-size heuristic. Or pin 25.02 by decision and record why.
First, a re-measure on H200 with the same CUDA on both versions, to separate the library from
the toolkit.

<a id="t232"></a>
### #232 (chunked parquet reader) the scan reads through `read_parquet`, which is several times slower than the chunked reader

`scan.cpp` loads every row-group batch with `cudf::io::read_parquet`. On GB10 the same bytes
through `cudf::io::chunked_parquet_reader` with one chunk load 5.6x faster.

Measured in `reports/dgx-spark.md` §2 (GB10, libcudf 26.02, CUDA 13.0, 2nd-minimum of repeated
runs, idle host), whole-table q6 load:

| reader | batches | q6 load |
|---|--:|--:|
| `cudf::io::read_parquet` | — | 340 ms |
| `chunked_parquet_reader`, 16 GiB chunk | 1 | 60.3-61.6 ms |
| same, 4 GiB | 4 | 60.7-61.0 ms |
| same, 1 GiB | 15 | 62.5-64.4 ms |
| same, 512 MiB | 29 | 81.3-82.2 ms |
| same, 128 MiB | 115 | 240.1-245.2 ms |

One chunk has the same single-shot semantics, so the gap is the reader, not chunking. Nor is it
the allocator: reserving 60% of memory up front, which makes pool growth impossible, changes
nothing (#148). The host floor says the fast number is right: `dd` reads the 9.0 GB file from
page cache at 23.6 GB/s, so q6's ~1.5 GB of compressed columns needs ~64 ms. `KVIKIO_NTHREADS`
explains part of the gap (1 thread 480 ms, 16 threads 323 ms) and not the rest; the mechanism is
unproven. Load dominates a single query: q6 is 19.2 ms execute against 399.9 ms load
(`reports/benchmark-minimal.md`). A cost, not a wrong answer.

**Corpus queries:** every device scan. The difference shows in the benchmark tree's load
column, not in any corpus cell.

**Fix proposed:** first re-measure on H200, since every number above is GB10 on CUDA 13.0 and
shad-gpu runs CUDA 12.2: `read_parquet` against a one-chunk `chunked_parquet_reader` over the
same row groups, with NVTX ranges to name the mechanism. If it holds, `scan.cpp` builds a
`chunked_parquet_reader` over the call's row groups, sets `chunk_read_limit` above the batch the
plan already sized (one chunk), and concatenates only if it returns several. The read limit is
the plan's, not a second size: 1-4 GiB was the sweet spot and 128 MiB cost 4x, so a batch sized
below 1 GiB wants re-measuring too. Tests: the device tiers stay byte-identical; a benchmark
section for the load.

<a id="t242"></a>
### #242 (26.02 join paths) the join session calls only the cuDF API that 25.02 and 26.02 share, and leaves 26.02's faster joins unused
**Priority: low** — nothing measured; the one real candidate may be slower.

The join rewrite (`tasks/join-rewrite-design.md` §3.10) calls nothing that only one cuDF version
has, so one code path runs on shad-gpu (25.02), the CI image (25.10a) and verda-gpu (26.02). Four
26.02 APIs could replace a portable step, each as a second path selected by
`CUDF_VERSION_MAJOR`/`MINOR` (`<cudf/version_config.hpp>`):

- `filtered_join(build, set_as_build_table::LEFT).semi_join(batch)` for LeftSemi, LeftAnti and
  LeftMark, in place of `hash_join::inner_join(distinct(probe keys))`. It saves the per-batch
  `distinct`, but cuDF documents the LEFT mode, a `static_multiset`, as the slower one. The
  likeliest win: tpch q4 q16 q21 q22 and tpcds q10 q16 q35 q69 q94 are build-side semi joins.
- `filtered_join(build, RIGHT)` for RightSemi and RightAnti, in place of `distinct_hash_join`
  over the build's distinct keys. Saves one `distinct` per join, at build.
- `hash_join` plus `filter_join_indices` for Inner, Left and Full with an AST-able residual, in
  place of gathering the condition's columns and masking the pairs. No corpus query has such a
  join today.
- `hash_join(build, nullable_join::NO, cmp, load_factor)` when the build keys hold no NULL.

A second path is tested only where 26.02 runs, so each wants a measured win first: the
corpus benchmark's join nodes on verda-gpu, each path against the portable one.

<a id="t248"></a>
### #248 (join parallelism gaps) three join shapes run on one lane or skewed after the rewrite
The chain-J join rewrite answers all three correctly; each costs parallelism.

- **`CollectLeft` is merged, not broadcast.** A join DataFusion plans `CollectLeft` has both sides
  merged to one lane (`translator/nodes.rs`, the not-co-partitioned arm). The join session is the
  broadcast's foundation; the planner and driver work is [#140](optimizer.md#t140).
- **A Full join's NULL keys are not dropped.** #137 filters NULL keys only on a side whose
  unmatched rows are never emitted; a Full join emits both sides', so every NULL-key row of both
  sides still hashes to one lane.
- **A keyless condition outside the hoistable form** (`fact JOIN tiny ON abs(f_qty - t_v) < 3`:
  `abs` takes the column path) runs as a chunked cross product, |build| × |probe| pairs, on one lane.

**Corpus queries:** pbench's collapse readings (`tasks/pbench.md`) and `dim FULL JOIN fact ON d_k = f_k`
at tp4.

<a id="t286"></a>
### #286 (offset row-group skip) an OFFSET that reaches a scan still reads every row group it skips
After chain K's limits task, DataFusion pushes `skip + fetch` into a scan and keeps the skip in
its limit above. `source()` (`planner/translator/nodes.rs`) trims the scan's row groups to the
shortest prefix whose metadata row counts reach `skip + fetch`, and the `GpuLimit` above (or the
unload's interval) drops the first `skip` rows after they are read. Only the tail is trimmed, so
every row group that lies wholly inside the skip is read, decoded and thrown away.
`SELECT * FROM lineitem OFFSET 5000000 LIMIT 10` at sf1 reads about 41 of lineitem's 49 row groups
to answer ten rows.

**Corpus queries:** none; `tpch/scan-limit` has no offset.

**Fix proposed:** in `source()`, read the skip from the limit directly above the scan. Drop the
leading row groups whose cumulative row count is at most the skip, and lower the limit's skip
by the rows they held. This holds only where the trim already holds: one lane, row groups in
index order, and metadata row counts that are the rows the scan emits. Test: the example above
reads one row group and answers the same ten rows as today.
