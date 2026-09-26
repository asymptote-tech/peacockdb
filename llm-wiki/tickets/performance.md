
# Performance

These issues should be fixed on a pre-prod performance path.

<a id="t150"></a>
### #150 — store the embedding columns uncompressed; Snappy costs a third of a vector query to save 3%
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
### #149 — the parquet load must use pinned host memory
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
### #148 — the engine installs no RMM allocator, and `gpu_memory_limit` is accepted and ignored
**Priority: high**

Nothing under `cpp/src/` or `cpp/include/` calls `set_current_device_resource`, so every cuDF
intermediate takes rmm's default: a `cudaMalloc`/`cudaFree` driver round trip each.

Measured: TPC-H q1 over sf40 whole-table on GB10 is 76.5 s execute (2nd-min of 5, all runs
inside [75.4, 78.8], so steady state) against 3.9 s streamed through bounded batches for the
same answer. The gap was fixed three times already — `multi_gpu.cpp`, the gtest mains and the
benchmark harness, the last two sharing `cpp/include/peacock/rmm_pool.hpp`
([#151](archive/archived-tickets.md#t151)); the engine is the only one of the four that ships.
The second half is the same fix: `gpu_memory_limit` is documented as a bound, stored at
`gpu_executor.cpp:99` and never read — the #132 shape one level up. Care: install per device
before any cuDF call, and tear down on the owning thread (`set_per_device_resource(id, nullptr)`
misses the ref map). Two questions #178 did not answer for the engine: whether the limit is a
reservation or a ceiling — the test binaries take `initial == maximum` because it fails loudly —
and how an integrated part is sized, whose only implementation went with the percentages
(`archive/historical-comments.md`). Tests: the GPU tiers stay byte-identical, plus a case
asserting a small limit is honoured.
