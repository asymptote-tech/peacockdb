
# Performance

These issues should be fixed on a pre-prod performance path.

## Contents

- [#150 (uncompressed embeddings) store the embedding columns uncompressed; Snappy costs a third of a vector query to save 3%](#t150)
- [#148 (RMM allocator) the engine installs no RMM allocator, and `gpu_memory_limit` is accepted and ignored](#t148)
- [#231 (26.02 big aggregate) one grouped aggregate call past about 100M rows is several times slower on libcudf 26.02](#t231)
- [#242 (26.02 join paths) the join session calls only the cuDF API that 25.02 and 26.02 share, and leaves 26.02's faster joins unused](#t242)
- [#248 (join parallelism gaps) three join shapes run on one lane or skewed after the rewrite](#t248)
- [#286 (offset row-group skip) an OFFSET that reaches a scan still reads every row group it skips](#t286)
- [#290 (stacked limits) a limit DataFusion pushed down and kept above becomes two adjacent `GpuLimit`s](#t290)
- [#305 (snappy batch floor) a parquet load's Snappy decompression takes about 29 ms however few bytes it holds](#t305)

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


<a id="t290"></a>
### #290 (stacked limits) a limit DataFusion pushed down and kept above becomes two adjacent `GpuLimit`s
**Priority: low** — no answer is wrong; the extra node slices at most `skip + fetch` rows again.

DataFusion pushes `skip + fetch` into an operator below a limit and keeps the limit above it
for the skip, or at tp4 for the `CoalescePartitionsExec` between them. The translator turns each
into its own node: `CoalesceBatchesExec`'s fetch (`node()`) and the `GlobalLimitExec`
(`mid_plan_limit`, `planner/translator/nodes.rs`). The coalesce between them leaves no node on a
one-lane input. So the plan carries a `GpuLimit` directly over a `GpuLimit`. Only a
root-adjacent limit folds today, into the unload's interval. After chain K's limits task, a
limited scan's cut is a `GpuLimit` too, and every limited scan under a kept limit stacks the
same way. Examples, sf1:

- `SELECT k FROM (SELECT p_partkey AS k FROM part WHERE p_size > 10 LIMIT 40 OFFSET 5) x, region`
  plans `GpuLimit 5..+40` over `GpuLimit 0..+45` at every mode, on master.
- The same without `OFFSET 5` plans `GpuLimit 0..+40` over an identical `GpuLimit 0..+40` at
  tp4.
- `SELECT p_partkey FROM part WHERE p_size > 10 LIMIT 10 OFFSET 2`, at the root: the
  projection above the limit stops the fold, so `GpuLimit 2..+10` sits over `GpuLimit 0..+12`.

**Corpus queries:** `tpch/nested-limits`, once the limits task lands: `GpuLimit 5..+23` over
the part scan's `GpuLimit 0..+28`, at every mode.

**Fix proposed:** in `mid_plan_limit`, when the translated input is a `GpuLimit`, replace the
pair with one `GpuLimit` whose interval is `RowInterval::over(outer, inner)`, the composition
the limits task adds for the root fold. Test: the three examples above plan one `GpuLimit`
each and answer as today. Their plan goldens and `nested-limits`' lose a line.


<a id="t305"></a>
### #305 (snappy batch floor) a parquet load's Snappy decompression takes about 29 ms however few bytes it holds
On the H100 (tpch sf40, tp1-single, from the pinned cache) each load's decompression is one
`nvcomp::unsnap_kernel` launch, and it takes about 29 ms whatever the load holds: q8's `supplier`
(1.9 MB copied) 29.0 ms, q3's `customer` 29.3 ms, against 125.8 ms for q8's 4.9 GB `lineitem`.

It is now the largest part of a small load: 81 % of `customer` (29.4 of 36.0 ms), and q8's four
mid-size loads pay 119 ms of it together. The before profile shows the same 29 ms, so the cache
did not cause it; it only stopped hiding it behind the host read.

One launch decompresses every page of the load as a batch, so its time follows its slowest page,
not its bytes. Next: the page sizes per column chunk from the footers, and the launch's per-chunk
time. If one large page sets it, it is a writer setting in `testdata/generate_testdata.sh`, as
#150 is. The numbers are a `create_nsys_profile.sh --trace` capture's kernels inside each load's
window, as `nsys_loads.py` draws it; the loads' times are in [#149](../archive/archived-tickets.md#t149).
