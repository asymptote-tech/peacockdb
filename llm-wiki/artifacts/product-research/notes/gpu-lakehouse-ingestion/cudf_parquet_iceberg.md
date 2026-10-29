# GPU Parquet decoding with libcudf and Iceberg metadata handling for a GPU scan tier

All URLs accessed 2026-10-29. Code claims refer to shallow clones made on 2026-10-29:
- NVIDIA/cudf (repo moved from rapidsai/cudf) @ `785ee522f5688f485fc0b1eabca7f8d6d2adea0e` (main, 2026-10-29)
- apache/iceberg-rust @ `beba39c8d62810fa101740bb2dbb03ab0d8f07ac` (main, 2026-10-28)
- facebookincubator/velox @ `bcdab35c839ef3dd4af8b75d5b38ae84b9c51fa0` (main, 2026-10-29)

## Q1. libcudf Parquet reader architecture, throughput, and known slow paths

### Takeaway
No public source explains a 5.6x gap between `read_parquet` and a one-chunk `chunked_parquet_reader`. At HEAD both paths run the same `reader_impl` code with different `read_mode` flags. The public record does document the costs that sit around decode: host-side footer/metadata parsing, a page-index over-read bug fixed in Oct 2026, a KvikIO default of 4 threads in compat (POSIX) mode, and decode kernels the cuDF team measures at under 50 GB/s on H100 for many types. Snappy goes to nvCOMP by default. Blackwell's hardware decompression engine is skipped for `cudaMallocAsync`/pinned-host buffers. The experimental hybrid_scan reader is where page-index, bloom-filter, dictionary-page and two-pass filter/payload pruning live, and it leaves I/O to the caller.

### Cited Findings

**Reader variants (code, cudf @785ee522)**
- `reader_impl::read()` (what `read_parquet` uses) calls `prepare_data(read_mode::READ_ALL)` then `read_chunk_internal(READ_ALL)`. `read_chunk()` does the same with `CHUNKED_READ`. In `compute_input_passes`, READ_ALL puts every row group into one pass. CHUNKED_READ sizes passes by `pass_read_limit` and also splits passes by the cudf column-size limit. READ_ALL fails if the total rows exceed the `size_type` max. — [reader_impl.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/parquet/reader_impl.cpp), [reader_impl_chunking.cu](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/parquet/reader_impl_chunking.cu), [reader_impl_preprocess.cu](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/parquet/reader_impl_preprocess.cu) (accessed 2026-10-29)
- Experimental `hybrid_scan_reader` ("optimized for highly selective filters") reads in two passes. Pass 1 reads only the filter columns and prunes row groups and filter-column pages with the filter expression. Pass 2 reads the payload columns and uses the surviving row mask to prune payload pages. Row-group pruning can use byte range, column-chunk statistics, bloom filters (`bloom_filters_byte_ranges`, `filter_row_groups_with_bloom_filters`) and dictionary pages. Page-level pruning uses the page index (`page_index_byte_range`, `setup_page_index`, then a row mask built from page stats). An optional `mask_data_pages` skips decompressing and decoding pruned pages. **The caller does the I/O:** the reader returns byte ranges, the caller fetches them into device buffers and passes device spans back. Chunked variants (`setup_chunking_for_filter_columns`/`..._payload_columns`) and a shareable pre-parsed footer metadata object exist. — [hybrid_scan.hpp @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/hybrid_scan.hpp); [libcudf class reference](https://docs.rapids.ai/api/libcudf/legacy/classcudf_1_1io_1_1parquet_1_1experimental_1_1hybrid__scan__reader) (accessed 2026-10-29)
- A multi-file hybrid scan exists (`hybrid_scan_multifile.hpp`), with pylibcudf bindings (PR #22795) and cudf-polars adoption in progress (PR #24854). — [issue #22583](https://github.com/NVIDIA/cudf/issues/22583), [PR #22795](https://github.com/NVIDIA/cudf/pull/22795), [PR #24854](https://github.com/NVIDIA/cudf/pull/24854) (accessed 2026-10-29)
- Hybrid scan bug: in cudf-polars, bloom-filter pruning drops every row because the raw byte ranges include the filter header. libcudf's `fetch_bloom_filters_to_device` strips the headers, but pylibcudf did not expose it. Filed late Oct 2026. — [issue #24851](https://github.com/NVIDIA/cudf/issues/24851) (accessed 2026-10-29)
- Page-level I/O work is ongoing for hybrid scan. The reader can already selectively decompress and decode pages. — [issue #23519](https://github.com/NVIDIA/cudf/issues/23519) (accessed 2026-10-29)
- Recent chunked-path PRs: PR #24607 skips pages outside each output chunk, for performance. PR #24608 fixes chunked reads of pages outside the row bounds. — [PR #24607](https://github.com/NVIDIA/cudf/pull/24607), [PR #24608](https://github.com/NVIDIA/cudf/pull/24608) (accessed 2026-10-29)
- Release cadence: v26.08.00 (2026-08-05), v26.08.01 (2026-08-25), v26.10.00 (2026-10-08), v26.10.02 (2026-10-20, latest). — `gh release list -R NVIDIA/cudf`, [releases](https://github.com/NVIDIA/cudf/releases) (accessed 2026-10-29)

**Host-side overhead / metadata**
- Issue #18890: on TPC-H SF100 lineitem (600M rows, 61 row groups, 16 columns, written by parquet-rs), "metadata reading alone takes 8 seconds" when reading one small column whose total kernel time is about 10 ms. SF300 takes about 30 s. The reader rebuilds `aggregate_reader_metadata` from the footer on every reader construction. `read_parquet` had no metadata cache. The chunked reader reads metadata once per instance. Closed 2025-12-17 by PR #20693, "Allow parquet readers to use existing `datasource`s and `metadata`s". — [issue #18890](https://github.com/NVIDIA/cudf/issues/18890), [PR #20693](https://github.com/NVIDIA/cudf/pull/20693) (accessed 2026-10-29)
- Issue #24000 (reported on **GB10**, cudf 26.08 nightly): when a file has offset indexes but no column indexes (PyArrow `write_page_index=True, write_statistics=False`, or parquet-rs), reader construction reads from byte 0 to the end of the last offset index. A 121.73 MB file read 129.46 MB to fetch 1 of 16 row groups (9.99 MB with no page index). A 67.8 MB file read 69.66 MB in 4 MiB preads. This happens on every `read_parquet` call and every new chunked reader. It was introduced in v24.04 (PR #14973), so it is present in 26.02. Fixed 2026-10-09 by PR #24001, commit 4f4de11 ("Fix Parquet page index byte range computation"). — [issue #24000](https://github.com/NVIDIA/cudf/issues/24000) (accessed 2026-10-29)
- A story issue (closed 2026-05-20) describes `read_parquet` as three sequential, blocking steps on one stream: footer metadata, then I/O through KvikIO, then GPU decompression and decode kernels. It reports unstable pipelining ("pipeline bubbles") also seen in Velox-cuDF TableScan. — [issue #18892](https://github.com/NVIDIA/cudf/issues/18892) (accessed 2026-10-29)
- Polars/Dask: parallel footer reading in cuDF 25.04 made S3 Parquet reads more than 3x faster. — [NVIDIA blog, 2025-05-29](https://developer.nvidia.com/blog/rapids-brings-zero-code-change-acceleration-io-performance-gains-and-out-of-core-xgboost/) (accessed 2026-10-29)

**KvikIO / I/O configuration (code, cudf @785ee522)**
- `set_up_kvikio` sets `KVIKIO_COMPAT_MODE` (default `ON`, i.e. POSIX reads rather than GDS) and the KvikIO thread pool size from `KVIKIO_NTHREADS` (default **4**). It runs once per process (`std::call_once`). — [config_utils.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/utilities/config_utils.cpp) (accessed 2026-10-29)
- Other env knobs in the same file and in datasource.cpp:
  - `LIBCUDF_NVCOMP_POLICY` (OFF/STABLE/ALWAYS, default STABLE)
  - `LIBCUDF_INTEGRATED_MEMORY_OPTIMIZATION` (AUTO/ON/OFF). AUTO turns on when `has_integrated_memory()`, which is relevant to GB10/DGX Spark, and makes `hostdevice_vector` keep a single copy.
  - `LIBCUDF_MMAP_ENABLED` (default OFF)
  — [config_utils.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/utilities/config_utils.cpp), [datasource.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/utilities/datasource.cpp) (accessed 2026-10-29)

**Decode throughput**
- Issue #18350 (open, by the cuDF team): "many data types show <50 GB/s of throughput on H100" in the decode kernels. The team targets at least 100 GB/s.
  - Nsight profiles show `gpuDecodePageDataGeneric` at about 50% peak warp occupancy.
  - For STRING: 10 ms unsnap, 0.7 ms `gpuComputeStringPageBounds`, 9.6 ms decode.
  - For STRUCT: 29 ms in `superimpose_nulls_no_sanitize`.
  - For DECIMAL/TIMESTAMP: 8.3 ms unsnap against 1.3–1.7 ms decode, so decompression dominates.
  — [issue #18350](https://github.com/NVIDIA/cudf/issues/18350) (accessed 2026-10-29)
- Issue #18349 (closed 2026-05-20): the `parquet_read_decode` benchmark was switched to uncompressed files because "some of the files show high latency in nvCOMP". — [issue #18349](https://github.com/NVIDIA/cudf/issues/18349) (accessed 2026-10-29)
- Luo, Chen, Binnig (TU Darmstadt), "Do GPUs Really Need New Tabular File Formats?", Feb 2026. On A100 with GDS, TPC-H SF300, rewriting Parquet with GPU-aware settings reaches up to 125 GB/s effective bandwidth. Page count strongly affects cuDF kernel parallelism ("avoid small values"; they use about 100 pages per chunk). Row groups of about 4M+ rows are needed (DuckDB's 122,880-row default gives about 100 KB chunks). They also recommend skipping compression when it saves less than 10%. — [arXiv 2602.17335](https://arxiv.org/html/2602.17335v1) (accessed 2026-10-29)
- Bauer et al. (IBM/NVIDIA/HeavyDB), "Accelerating Presto with GPUs", Jun 2026. cuDF 25.04's Parquet reader was "10× slower than the theoretical I/O rate" on 8x A100 with GPFS (170 GB/s). Their Arrow-like custom format reached 22 GB/s (95% of theoretical) on SF-1000 lineitem. They attribute the gap to nested metadata that "requires continuous interpretation" and to interleaved data and decode. cuDF 25.10 is described as "more efficient", with no number given. — [arXiv 2606.24647](https://arxiv.org/html/2606.24647) (accessed 2026-10-29)
- Issue #16130: in the chunked reader, zstd greatly increases decompression temp memory, which produces small chunks on nested/string-heavy data. — [issue #16130](https://github.com/rapidsai/cudf/issues/16130) (accessed 2026-10-29)
- Chunked-path correctness bugs:
  - #24002: hang or `cudaErrorIllegalAddress` on zstd DataPageV2 files when `pass_read_limit` is non-zero.
  - #17158: large string columns differ from the non-chunked reader.
  - #17692: nested large strings with a small `chunk_read_limit`.
  — [#24002](https://github.com/NVIDIA/cudf/issues/24002), [#17158](https://github.com/NVIDIA/cudf/issues/17158), [#17692](https://github.com/rapidsai/cudf/issues/17692) (accessed 2026-10-29)

**Decompression path (code, cudf @785ee522)**
- `device_decompress` sends SNAPPY/ZSTD/LZ4/etc. to `nvcomp::batched_decompress` unless nvCOMP is disabled. Only then does Snappy fall back to cuDF's own `gpu_unsnap` (`unsnap_kernel_no_racecheck<128>` in `unsnap.cu`). — [decompression.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/comp/decompression.cpp), [unsnap.cu](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/comp/unsnap.cu) (accessed 2026-10-29)
- Hybrid host/device decompression: `LIBCUDF_HOST_DECOMPRESSION` (default OFF; AUTO available) sorts chunks by size. It then sends `[0, split_idx)` to a host thread pool and the rest to the device on forked streams, tuned by `LIBCUDF_HOST_DECOMPRESSION_THRESHOLD` and `LIBCUDF_HOST_DECOMPRESSION_RATIO`. — [decompression.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/comp/decompression.cpp) (accessed 2026-10-29)
- `LIBCUDF_HW_DECOMPRESSION` selects the nvCOMP backend for Snappy/Zstd/Deflate/LZ4/Gzip. The default is `NVCOMP_DECOMPRESS_BACKEND_DEFAULT`, which prefers the DE. — [nvcomp_adapter.cpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/src/io/comp/nvcomp_adapter.cpp) (accessed 2026-10-29)
- nvCOMP release notes:
  - 3.0.0 (2023-07): Snappy up to 1.3x faster on H100, LZ4 1.4x, zstd 2.2x
  - 4.0.0: zstd about 20% faster on H100
  - 4.2.0: added the Blackwell HW Decompress Engine for Snappy, Gzip, Deflate
  - 5.0.0: "Up to 100% speedup in Snappy decompression throughput on devices where Decompress Engine not available"
  - 5.2.0: fixed DE offload in the Python API
  - 5.3.0: LZ4 5–12% faster
  - Latest is 6.0.0
  - No absolute H100 GB/s are published there.
  — [nvCOMP release notes](https://docs.nvidia.com/cuda/nvcomp/release_notes.html) (accessed 2026-10-29)
- nvCOMP 2.0 announcement (old): Snappy/LZ4 "up to 100 GB/s" depending on dataset. — [NVIDIA blog](https://developer.nvidia.com/blog/nvcomp-v2-0-0-now-available-with-new-compressors) (accessed 2026-10-29)

**Blackwell Decompression Engine (DE)**
- The DE is fixed-function hardware for Snappy, Deflate/Gzip and LZ4, rated "up to 600 GB/s". It runs on B200, B300, GB200 and GB300. Max chunk size is 4 MiB on B200 (`CU_DEVICE_ATTRIBUTE_MEM_DECOMPRESS_MAXIMUM_LENGTH`).
  - **Uses the DE:** `cudaMalloc`, `cudaMallocPitch`, `cudaMallocFromPoolAsync` with `cudaMemPoolCreateUsageHwDecompress`, and `cuMemCreate` with `CU_MEM_CREATE_USAGE_HW_DECOMPRESS`.
  - **Falls back to SMs:** `cudaMallocAsync`, `cudaMallocHost`/`cudaHostAlloc` pinned memory, `cudaMallocManaged`, and pools without the flag.
  - The DE is fault-intolerant: an over-stated compressed size counts as non-compliant.
  - `sort_before_hw_decompress=1` improves load balance.
  — [nvCOMP DE FAQ](https://docs.nvidia.com/cuda/nvcomp/decompression_engine_faq.html) (accessed 2026-10-29)
- The NVIDIA DE blog: the DE is integrated with the copy engine. nvCOMP's default backend prefers the DE and silently falls back to SM kernels. — [NVIDIA blog](https://developer.nvidia.com/blog/speeding-up-data-decompression-with-nvcomp-and-the-nvidia-blackwell-decompression-engine/) (accessed 2026-10-29)
- cuDF 25.04 + nvCOMP 4.2.0.11 on **B100**, PDS-H SF100, Snappy: 35% faster end to end with hardware decompression than with SM kernels. That run used an RMM pool MR; the kernel run used a CUDA async MR. — [NVIDIA blog, 2025-05-29](https://developer.nvidia.com/blog/rapids-brings-zero-code-change-acceleration-io-performance-gains-and-out-of-core-xgboost/) (accessed 2026-10-29)
- Independent B200 microbenchmark: DE throughput varies from 42 to 462 GB/s depending on algorithm. GZIP is 42.0 GB/s in and 83.8 GB/s out. LZ4 output is about 160–220 GB/s. Snappy trades peak throughput for latency (0.894 ms). — [arXiv 2512.02189](https://arxiv.org/pdf/2512.02189) (accessed 2026-10-29)
- The nvCOMP product page also says "up to 600 GB/s" on Blackwell DE and lists standard codecs Snappy, ZSTD, Deflate, LZ4. ZSTD is listed as a supported format, not as DE-accelerated. — [nvCOMP page](https://developer.nvidia.com/nvcomp.md) (accessed 2026-10-29)

### Inferences
- The H200 Snappy cost (38% of kernel time) fits #18350, where unsnap time is equal to or larger than decode for fixed-width types on H100. H200 has no DE, so Snappy always runs on SMs. Rewriting files uncompressed, or with LZ4/ZSTD, plus larger pages and row groups (2602.17335) is the documented lever (estimate).
- The "nvcomp unsnap_kernel" name in peacockdb's profile is probably nvCOMP's own Snappy kernel. cuDF's `unsnap_kernel_no_racecheck` runs only when `LIBCUDF_NVCOMP_POLICY=OFF` (estimate: kernel naming inside nvCOMP not verified).
- On B200, if peacockdb allocates through RMM `cuda_async_memory_resource` (`cudaMallocAsync`), nvCOMP silently uses SM kernels instead of the DE. A pool created with `cudaMemPoolCreateUsageHwDecompress` or plain `cudaMalloc` is required (inference from the DE FAQ).
- Candidate explanations for the GB10 5.6x gap, none confirmed publicly:
  - #24000 over-read, if the files have offset-only page indexes. 26.02 is affected.
  - Different footer/metadata handling per call.
  - Single-pass READ_ALL decompression scratch allocation pressure on unified memory.
  - KvikIO 4-thread default.
  Each would need a profile (estimate).
- For a GPU scan tier the hybrid_scan reader fits well. Iceberg planning gives files and row groups, the caller owns the I/O (its own thread pool or GDS), and page-index and bloom pruning happen before payload I/O.

### Gaps
- I found no public issue that reports `read_parquet` being slower than a one-chunk `chunked_parquet_reader` on the same bytes. The root cause of peacockdb #232 is not publicly documented.
- I found no published absolute end-to-end libcudf Parquet GB/s for H100, H200 or B200 in a 2025–26 RAPIDS blog. I also found no GTC 2026 talk with decode GB/s.
- I found no published nvCOMP Snappy GB/s on H100/H200 for current versions (5.x/6.0).
- Whether GB10 (DGX Spark) has a DE is not covered by the DE FAQ, which lists only B200/B300/GB200/GB300.
- I found no public data on pinned vs pageable host-buffer effects for KvikIO compat mode in 26.x.

## Q2. cuDF / RAPIDS Iceberg and Delta support: GPU vs CPU

### Takeaway
libcudf has no Iceberg metadata layer. It does have an experimental Parquet reader that applies serialized 64-bit roaring-bitmap deletion vectors and prepends a row-index column, which is the building block for both Delta DVs and Iceberg v3 DVs. Spark RAPIDS reads Iceberg v1/v2 on GPU, including position and equality deletes, with planning and metadata on the JVM/CPU. Its docs list v3 DV reads as unsupported, while DV writes were merged in Sep 2026. Velox-cuDF has an Iceberg connector with GPU deletion-vector and equality-delete readers. cudf-polars Iceberg scan is still an open feature request.

### Cited Findings
- libcudf `cudf::io::parquet::experimental` has `deletion_vector_info` (`serialized_roaring_bitmaps`, which are host spans of 64-bit roaring bitmaps in "portable" format, plus per-DV row counts, `row_group_offsets`, `row_group_num_rows`, and `are_retention_vectors`). It also has a `chunked_parquet_reader` that prepends a row index column and filters each chunk by the DV ("Delta Lake style deletion vectors"). — [deletion_vectors.hpp @785ee522](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/deletion_vectors.hpp) (accessed 2026-10-29)
- Issue #23736 (closed 2026-08-24): adds prepending a row index without a DV, with per-row-group start rows. This is needed for Iceberg `_pos`, position deletes and row lineage. — [issue #23736](https://github.com/NVIDIA/cudf/issues/23736) (accessed 2026-10-29)
- Issue #23793 (open): verify DV metadata such as cardinality and CRC. — [issue #23793](https://github.com/NVIDIA/cudf/issues/23793) (accessed 2026-10-29)
- Issue #21732 (open, 2026-03): asks for Iceberg/Delta/Hudi support in cuDF and notes "read deletes is a big challenge for cudf". — [issue #21732](https://github.com/NVIDIA/cudf/issues/21732) (accessed 2026-10-29)
- cudf-polars:
  - #22743 (open, 2026-06): basic `scan_iceberg` (read data files, read delete files, filter).
  - #24247 (open, 2026-09): support `polars.scan_delta`/`scan_iceberg`. Polars IR represents them as a Parquet Scan node with extra attributes (Polars 2.0.0-rc2). This needs hybrid scan to support hive-partitioned Parquet (#17832).
  — [#22743](https://github.com/NVIDIA/cudf/issues/22743), [#24247](https://github.com/NVIDIA/cudf/issues/24247) (accessed 2026-10-29)
- Spark RAPIDS (docs updated 2026-10-23), Iceberg is "experimental":
  - Spark 3.5/4.0/4.1 with Iceberg 1.6.1–1.11.0, spec **v1 and v2**.
  - GPU reads cover data files, equality delete files and position delete files. **Deletion vectors are "Not supported."**
  - Metadata tables stay on CPU. Only Parquet is supported (no ORC/Avro).
  - struct/list/map types fall back to CPU.
  - Partition transforms identity/bucket/void/truncate/year/month/day/hour.
  - GPU writes cover INSERT, MERGE, DELETE, UPDATE, CTAS and RTAS.
  - Catalogs: S3Tables/REST/Hadoop (Hive and JDBC experimental).
  - Split sizing uses Iceberg `read.split.target-size`.
  — [Spark RAPIDS Iceberg docs](https://docs.nvidia.com/spark-rapids/user-guide/latest/additional-functionality/iceberg-support.html) (accessed 2026-10-29)
- NVIDIA/cudf-spark PR #15717, "Support Iceberg v3 deletion vector writes on GPU", merged 2026-09-22. GPU UPDATE/MERGE also depends on row-lineage work (#15441). PR #15964 re-enables mixed position and equality delete tests on Iceberg 1.10.2+. — [PR #15717](https://github.com/NVIDIA/cudf-spark/pull/15717), [PR #15964](https://github.com/NVIDIA/cudf-spark/pull/15964) (accessed 2026-10-29)
- Velox-cuDF, on main, has `velox/experimental/cudf/connectors/hive/iceberg/` with:
  - `CudfIcebergDataSource`
  - `CudfIcebergSplitReader`
  - `CudfDeletionVectorReader`, which reads Iceberg V3 DVs in 64-bit Roaring64 or legacy 32-bit format
  - `CudfEqualityDeleteFileReader`, which loads delete keys into a CPU or GPU table (Parquet only) and lazily builds a `cudf::distinct_hash_join` to probe base rows
  - `CudfIcebergDeletionHelpers.cu`
  Recent PRs fix Iceberg timestamp types (#19653 merged 2026-10-28; #18852 open). — [velox tree @bcdab35c](https://github.com/facebookincubator/velox/tree/bcdab35c839ef3dd4af8b75d5b38ae84b9c51fa0/velox/experimental/cudf/connectors/hive/iceberg), [PR #19653](https://github.com/facebookincubator/velox/pull/19653) (accessed 2026-10-29)
- Velox-cuDF Hive data source issue #19277 (open): `CudfHiveDataSource`/`CudfSplitReader` ignore several Hive reader options. — [velox #19277](https://github.com/facebookincubator/velox/issues/19277) (accessed 2026-10-29)
- In the Presto-on-GPU paper, about 50% of Velox operators have GPU versions. Missing ones go through CudfToVelox/CudfFromVelox copies, which "can dominate execution time". The paper discusses only the Hive connector. — [arXiv 2606.24647](https://arxiv.org/html/2606.24647) (accessed 2026-10-29)

### Inferences
- The industry pattern: catalog, manifests and planning run on CPU (JVM or C++). Data files decode on GPU through cuDF. Position deletes and DVs become a row mask or roaring bitmap applied on GPU. Equality deletes become a GPU hash anti-join (Velox `distinct_hash_join`). peacockdb could copy this split directly (estimate).

### Gaps
- I found no published throughput numbers for GPU delete application (DV or equality-delete anti-join) in Spark RAPIDS or Velox-cuDF.
- I did not verify whether the Spark RAPIDS docs page reflects DV *read* support added after PR #15717.

## Q3. Iceberg metadata pruning mechanics and Rust/C++ libraries

### Takeaway
Iceberg planning works in stages:
1. Snapshot, then the manifest list. Each manifest entry carries partition field summaries.
2. Manifests, pruned by partition summaries.
3. Data-file entries, pruned by partition values and by per-column lower/upper bounds and null/NaN counts.
4. Delete-file matching by sequence number.

v3 replaces position-delete files with one Puffin DV per data file. iceberg-rust main handles position deletes, equality deletes and v3 DVs in its Arrow reader, but its latest release is 0.10.1 and 0.11 had not shipped as of Sep 2026. iceberg-cpp 0.4.0 (2026-09-26) has MOR readers for v2 deletes and DV blobs, but is pre-1.0.

### Cited Findings
- Spec mechanics (Iceberg table spec):
  - The manifest list has one entry per manifest, with `partitions` field summaries (`contains_null`, `contains_nan`, `lower_bound`, `upper_bound`) and added/existing/deleted file counts.
  - Manifest `data_file` entries carry the partition tuple, `record_count`, `file_size_in_bytes`, `column_sizes`, `value_counts`, `null_value_counts`, `nan_value_counts`, `lower_bounds`/`upper_bounds` (per field id, single-value serialized), and `split_offsets`.
  - Partition transforms: identity, bucket[N], truncate[W], year/month/day/hour, void.
  - v2 delete scoping: position deletes apply to data files with data sequence number ≤ the delete file's. Equality deletes apply to data files with data sequence number strictly < the delete file's, within the same partition (global if unpartitioned).
  - v3 deletion vectors are Puffin `deletion-vector-v1` blobs (roaring bitmaps of positions), referenced by `referenced_data_file`, `content_offset` and `content_size_in_bytes`, with at most one DV per data file.
  - v3 tables must not add new position delete files. v4 tables must not add equality delete files.
  — [Iceberg spec](https://iceberg.apache.org/spec/) (accessed 2026-10-29; field names from the spec as I know it; the search summary confirmed only the v3/v4 delete-file rules)
- Iceberg 1.12 / Rust 0.11 preview (Dremio, 2026-09-18):
  - iceberg-rust 0.11.0 "decodes deletion-vector Puffin blobs and applies V3 deletion vectors during scans" and adds `_pos`, `_spec_id`, `_partition`, row-lineage columns and encryption.
  - `iceberg-datafusion` moves to a new `apache/datafusion-iceberg` repo after 0.11.
  - The 0.11.0 vote had failed twice (RC1 on a circular dev-dependency, RC2 on encryption tamper checks, fix PR #3236).
  - Iceberg 1.12 adds a Flink pipeline converting equality deletes to DVs and blocks writing position deletes with row data.
  — [Dremio blog](https://www.dremio.com/blog/preview-of-upcoming-apache-iceberg-1-12-and-iceberg-rust-0-11-new-features-and-breaking-changes/) (accessed 2026-10-29)
- iceberg-rust releases: v0.8.0 (2026-01-16), v0.9.0 (2026-03-18), v0.9.1, v0.10.0 (2026-07-21), **v0.10.1 (2026-08-01, latest)**. No 0.11 release exists as of 2026-10-29. — [releases](https://github.com/apache/iceberg-rust/releases) (accessed 2026-10-29)
- 0.8.0 release notes: "Support for both position and equality deletes on the same FileScanTask". — [Iceberg Rust 0.8.0 blog](https://iceberg.apache.org/blog/apache-iceberg-rust-0.8.0-release/) (accessed 2026-10-29)
- 0.10.0 notes include:
  - a fix for a lost-wakeup hang in `DeleteFileIndex::get_deletes_for_data_file` (#2696)
  - a fix where `filter_row_groups_by_byte_range` duplicated rows for sub-row-group splits (#2615)
  - rejection of negative positional delete positions (#2631)
  - Puffin magic validation (#2416)
  - `ScanMetrics::bytes_read` documented to include delete files (#2379)
  — [v0.10.0 release](https://github.com/apache/iceberg-rust/releases/tag/v0.10.0) (accessed 2026-10-29)
- iceberg-rust main (@beba39c8): `CachingDeleteFileLoader::load_file_for_task` treats a PositionDeletes entry with Puffin format as a v3 DV (`load_deletion_vector`, with cardinality validation). Otherwise it streams Parquet position deletes, and it handles EqualityDeletes separately. Recent PRs: #3145 "validate deletion vector delete tasks" (merged 2026-10-09), #3461 make `delete_vector` public (merged 2026-10-15), #3433 resolve unknown delete-file sizes lazily (open). — [caching_delete_file_loader.rs @beba39c8](https://github.com/apache/iceberg-rust/blob/beba39c8d62810fa101740bb2dbb03ab0d8f07ac/crates/iceberg/src/arrow/caching_delete_file_loader.rs), [PR #3145](https://github.com/apache/iceberg-rust/pull/3145), [PR #3461](https://github.com/apache/iceberg-rust/pull/3461) (accessed 2026-10-29)
- Third-party fork JanKaul/iceberg-rust PR #412 (Sep 2026) adds v2 Parquet position-delete reads in DataFusion scans and says DVs remain a separate follow-up. That refers to the fork, not apache. — [JanKaul PR #412](https://github.com/JanKaul/iceberg-rust/pull/412) (accessed 2026-10-29)
- iceberg-cpp releases: 0.2.0 (2026-01-26), 0.3.0 (2026-06-10), **0.4.0 (2026-09-26, latest)**. — [releases](https://github.com/apache/iceberg-cpp/releases) (accessed 2026-10-29)
- iceberg-cpp 0.3.0 added a merge-on-read file scan task reader, delete filters, and a DeleteLoader for v2 position and equality deletes, plus Roaring position bitmaps, a position delete index and range coalescing. — [0.3.0 blog](https://iceberg.apache.org/blog/apache-iceberg-cpp-0.3.0-release/) (accessed 2026-10-29)
- iceberg-cpp 0.4.0 added:
  - lazy scan planning streams and parallel manifest reading
  - an LRU cache
  - `deletion-vector-v1` blob read/write and DV merging
  - write ops (RowDelta, OverwriteFiles, RewriteFiles, ...)
  - REST catalog SigV4/OAuth2 and vended-credential FileIO
  - v3 types, defaults and row lineage
  - Arrow 25.0.0, CMake only
  Hive Metastore is "groundwork". The 0.5.0 roadmap covers "remaining Iceberg v3 feature support". — [0.4.0 blog](https://iceberg.apache.org/blog/apache-iceberg-cpp-0.4.0-release/), [roadmap #226](https://github.com/apache/iceberg-cpp/issues/226) (accessed 2026-10-29)
- Java Iceberg 1.11 added REST server-side scan planning: the catalog returns file scan tasks with their delete files. — [Google OSS blog](https://opensource.googleblog.com/2026/05/announcing-apache-iceberg-1110.html), [REST scan planning article](https://ingestthis.com/posts/2026/2026-08-24-inside-iceberg-rest-scan-planning) (accessed 2026-10-29)

### Inferences
- For a C++ executor, the options are iceberg-cpp 0.4.0 (native C++, Arrow-based, has delete loaders and DV blobs, pre-1.0) or iceberg-rust through FFI (more mature scan and delete code on main, but the release lags main). Either way, the planning output to feed cuDF is a list of (data file, row-group byte ranges, position bitmap/DV, equality-delete files) (estimate).
- REST server-side planning could let a GPU engine skip manifest parsing entirely when the catalog supports it (estimate).

### Gaps
- I found no public measurement of how many bytes Iceberg manifest/stats pruning saves on real workloads. Savings depend on clustering and sort order. No primary benchmark was found within the search budget.
- pyiceberg delete support status was not checked.
- The iceberg-datafusion pushdown and row-group/page-filter specifics were not verified.

## Q4. delta-kernel-rs, delta-rs, Hudi, Paimon Rust readers (brief)

### Takeaway
delta-kernel-rs is at 0.30.x, delta-rs Rust reached 1.0.0, hudi-rs is at 0.5.0, and paimon-rust is at 0.5.0-rc. Only the Delta stack is near production for a native reader. libcudf's experimental DV-aware Parquet reader targets Delta-style roaring DVs.

### Cited Findings
- delta-kernel-rs latest tags are v0.30.1, v0.30.0, v0.29.0. It does not use GitHub Releases. — [tags](https://github.com/delta-io/delta-kernel-rs/tags) (accessed 2026-10-29)
- delta-rs: `rust-v1.0.0` released 2026-09-28 (latest) and `python-v1.6.6` released 2026-09-24. — [releases](https://github.com/delta-io/delta-rs/releases) (accessed 2026-10-29)
- apache/hudi-rs: `release-0.5.0` released 2026-09-10. — [releases](https://github.com/apache/hudi-rs/releases) (accessed 2026-10-29)
- apache/paimon-rust: latest release object is `bindings/go/v0.5.0-rc1` (2026-10-26). — [releases](https://github.com/apache/paimon-rust/releases) (accessed 2026-10-29)
- libcudf DV API doc: "APIs for applying Delta Lake style deletion vectors when reading Parquet data". — [deletion_vectors.hpp](https://github.com/NVIDIA/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/deletion_vectors.hpp) (accessed 2026-10-29)

### Inferences
- A Delta path could be delta-kernel-rs for the log, file list and DV bytes, then libcudf's experimental DV reader. That combination looks the least glue-heavy of the lakehouse formats for a cuDF engine (estimate).

### Gaps
- I did not verify the feature matrix of delta-kernel-rs, hudi-rs or paimon-rust: DV support, column mapping, MOR log files, and the Paimon LSM merge.
