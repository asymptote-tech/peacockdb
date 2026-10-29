# GPU-resident cache, encoding, and data-path bandwidth for peacockdb

All web links were accessed on 2026-10-29. I cloned the code shallowly under `/tmp/claude-1000/-media-data-peacockdb/ba2658c9-3cbe-4aa7-bcb8-c0a874d6fde0/scratchpad/ext/`. Commits:
- heavyai/heavydb `72c6df9f1bce2cd5b849670b38c7e7e4592c564b`
- sirius-db/sirius `b9e05ffc653c8ed031c789e5c1415ac1186e0619`
- rapidsai/gqe `50facc46fb99ecf4a955697e78803707335814b3`
- rapidsai/cudf `785ee522f5688f485fc0b1eabca7f8d6d2adea0e` (main, 2026-10-29)
- peacockdb's vendored cudf `third_party/cudf` `43505bb975f46ce4e140fdaf55192611d6e231ac`
- peacockdb `f36a68eca3704bb4add89d0292060ed545660948`

The pg-strom clone failed (empty repo), so pg-strom claims come from its docs only.

## Q1. Real-time ingestion plus a partly VRAM-resident cache: how existing engines do it

### Takeaway
Every engine studied separates the place where writes land (an append buffer or REDO log in host RAM, or a "shadow" tail fragment) from the GPU-resident, column-chunked copy that queries read.

- **Freshness:** the GPU copy is brought up to date in batches on a timer or size threshold, and again when a query starts.
- **Eviction:** chunk-granular, by LRU or priority, and never evicts a chunk a running query has pinned.
- **Invalidation:** this is the hard part. Sirius's legacy cache served stale data after UPDATE (issue #106).

For peacockdb, the cheapest correct design is:
- an immutable, chunk-granular (row-group-sized) cache of cuDF columns keyed by (file or table version, row group, column);
- plus an append-only delta tail in pinned host RAM, concatenated or scanned alongside at query time;
- with invalidation by a version/epoch bump instead of in-place mutation.

### Cited Findings

**HeavyDB (open-source heavyai/heavydb @72c6df9f)**
- **Three-level memory hierarchy.** `enum MemoryLevel { DISK_LEVEL = 0, CPU_LEVEL = 1, GPU_LEVEL = 2 }` — [DataMgr/MemoryLevel.h:10](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/DataMgr/MemoryLevel.h#L10) (accessed 2026-10-29)
- **Granularity defaults.** Tables are horizontally split into fragments of `DEFAULT_FRAGMENT_ROWS 32000000` rows. Pages are `DEFAULT_PAGE_SIZE 2097152` (2 MiB). There is a `DEFAULT_MAX_CHUNK_SIZE 2147483648` (2 GB) per chunk. A chunk is one column of one fragment. — [Fragmenter/FragmentDefaultValues.h:8-12](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/Fragmenter/FragmentDefaultValues.h#L8) (accessed 2026-10-29)
- **Buffer pool sizing.** GPU and CPU buffer pools are sized by `gpu_buffer_mem_bytes` and `cpu_buffer_mem_bytes`, and allocated in slabs (`min_cpu_slab_size`, `max_gpu_slab_size`). — [ThriftHandler/CommandLineOptions.cpp:264,498,1202,1226](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/ThriftHandler/CommandLineOptions.cpp#L498) (accessed 2026-10-29)
- **Eviction.** It is per contiguous run of pages within a slab. Buffers with `getPinCount() > 0` (in use by a running query) cannot be evicted. The candidate score is the max `lastTouched` (an LRU variant). The comment explains a switch from summed to max score: the summed score caused thrashing where "a query would evict its own current chunks". — [DataMgr/BufferMgr/BufferMgr.cpp:370-400](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/DataMgr/BufferMgr/BufferMgr.cpp#L370) (accessed 2026-10-29)
- **Append path (`InsertOrderFragmenter`).** Rows are appended into the current tail fragment's chunk buffers (`appendData`). The fragment's `shadowNumTuples` is advanced first, then published with `setPhysicalNumTuples(shadowNumTuples)` after all columns are written. This is a simple visibility fence: readers see only the published row count. A new fragment starts when `maxFragmentRows_` is reached. — [Fragmenter/InsertOrderFragmenter.cpp:1060-1090, 1225-1250](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/Fragmenter/InsertOrderFragmenter.cpp#L1225); [Fragmenter/Fragmenter.h:119](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/Fragmenter/Fragmenter.h#L119) (accessed 2026-10-29)
- **Kafka importer is micro-batching.** It consumes messages, converts rows to columns, and when `recv_rows == copy_params.batch_size` calls `row_loader.do_load(...)` then `consumer->commitSync()`. Auto-commit is disabled. The default `batch_size = 10000`. — [ImportExport/KafkaImporter.cpp:311,410-418,461](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/ImportExport/KafkaImporter.cpp#L410) (accessed 2026-10-29)

**Kinetica tiered storage**
- **Five tiers.** VRAM → RAM → Disk Cache → Persist → Cold Storage. GPU operations require their data in VRAM, and data moves up from RAM into VRAM for GPU work. On startup, table data loads from Persist into RAM, higher-priority data first. — [Kinetica 7.2 RM concepts](https://docs.kinetica.com/7.2/rm/concepts/) (accessed 2026-10-29)
- **Eviction priority.** Ranges 1-10: 1 means evicted first, 10 means unevictable. Ties are broken by LRU. Watermark eviction starts above the high watermark and runs down to the low watermark. Request-based eviction runs when a request lacks space. An object at its lowest possible tier is unevictable. — [Kinetica 7.2 RM concepts](https://docs.kinetica.com/7.2/rm/concepts/); [Kinetica RM concepts (current)](https://docs.kinetica.com/content/rm/concepts) (accessed 2026-10-29)
- **VRAM tier defaults.** The default config gives the VRAM tier 95% of VRAM, with high/low watermarks of 90%/50%. The RAM tier has no limit and no watermark eviction by default. — [Kinetica RM concepts (search summary of docs.kinetica.com)](https://docs.kinetica.com/content/rm/concepts) (accessed 2026-10-29)
  - These defaults come from a search snippet. The 7.2 page I fetched shows 95/50 only as an example.
- **Gap:** I did not fetch Kinetica's ingest path (multi-head ingest, Kafka). The concepts page does not describe how new rows enter tiers.

**PG-Strom GPU Cache**
- **Placement.** It keeps a copy of a PostgreSQL table in GPU device memory, with a REDO Log Buffer in host shared memory. It is recommended for "relatively small (about 10 GB), frequently updated" tables. — [PG-Strom GPU Cache docs](https://heterodb.github.io/pg-strom/gpucache/) (accessed 2026-10-29)
- **Write capture.** An AFTER ROW trigger `pgstrom.gpucache_sync_trigger()` on INSERT/UPDATE/DELETE appends to the REDO buffer using CPU and RAM only.
- **Apply.** A "GPU memory keeper" background worker applies the log in parallel on the GPU when unapplied data exceeds `gpu_sync_threshold` (default 25% of `redo_buffer_size`) or after `gpu_sync_interval` (default 5 s). Any unapplied log is also applied at query start, so results match the heap.
- **Defaults.** `max_num_rows` 10485760 and `redo_buffer_size` 160m.
- **Visibility.** Old row versions are released only after commit.
- **Failure mode.** Exceeding `max_num_rows` or the variable-length area corrupts the cache. Queries then stop using it, and recovery is by `pgstrom.gpucache_recovery(regclass)`. — [PG-Strom GPU Cache docs](https://heterodb.github.io/pg-strom/gpucache/) (accessed 2026-10-29)

**Sirius `pin_table` (sirius-db/sirius @b9e05ffc)**
- **API.** `CALL pin_table('/data/lineitem/*.parquet', tier => 'gpu'|'host', name=>…, cols=>[…], compression=>true)` pins Parquet-sourced chunks on the GPU or host tier, optionally "Simpatico-compressed" using per-table plan files. Scans decompress the projected columns at task-prepare time, with up to 4 column-parallel streams. — [docs/super-sirius/compressed-pinning.md:1-25](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **Measured on GB300, TPC-H SF1000, 22-query hot suite** (baseline 20.74 s, all host-pinned and uncompressed), from compressed-pinning.md:46-70:

  | Configuration | Hot time | Change |
  |---|---|---|
  | All host-pinned, compressed | 22.20 s | +7.0%, worse |
  | Dimensions on GPU raw, lineitem on GPU compressed | 17.05 s | −17.8% |
  | Plus orders on GPU compressed | 16.66 s | −19.7% |

  - The seven hottest lineitem columns are **264 GB raw, ~93 GB compressed** (≈2.84×). They carry ~2.4 TB of the suite's 2.9 TB of pin traffic.
  - Host-tier compression loses because every batch pays H2D fetch → sync → decode on the critical path, while raw host pins stream at ~370 GB/s over GB300 C2C. — same file
- **Rules of thumb in the same file:**
  - Choose plans whose decode throughput exceeds the host→GPU link.
  - Numerics, dates and keys decode at 580-1700 GB/s (bitpack/ans/delta).
  - Low-cardinality string dictionaries decode at ~250 GB/s (gather-bound). Long-text snappy decodes at ~230-270 GB/s.
  - Use a high-ratio, slow plan (e.g. `o_comment` snappy, 2.6× @ 260 GB/s) for host pins and identity for GPU pins.
  - ANS fails above 2 GiB per batch. — [compressed-pinning.md:73-87](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **Eviction cost.** Under memory pressure the executor evicted ~42 GB of concat intermediates in one episode. They were re-uploaded ~450 ms later, an ~84 GB round trip per iteration. Raising the eviction threshold when GPU-pinning large tables is advised. — [compressed-pinning.md:160-175](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **Stale cache (Sirius issue #106, legacy Sirius).**
  - After `ALTER TABLE ADD COLUMN`, a GPU query fails planning and falls back to CPU.
  - After `UPDATE my_data SET val=200`, `gpu_processing` "STILL returns 100 (stale cache)".
  - Closed 2026-06-04 by mbrobbel as "Closing (legacy sirius)", with no fix described. — [sirius-db/sirius#106](https://github.com/sirius-db/sirius/issues/106) (accessed 2026-10-29)

**NVIDIA GQE (rapidsai/gqe @50facc46)**
- **In-memory tables.** They live in a task-manager memory pool, `GQE_INITIAL_TASK_MANAGER_MEMORY` (default 10 GiB), "used across queries, e.g., for in-memory tables".
  - Optional inter-process shared memory: `GQE_IN_MEMORY_TABLE_USE_SHARED_MEMORY`.
  - Zone-map partition pruning: `GQE_USE_PARTITION_PRUNING`.
  - Zero-copy reads: `GQE_READ_USE_ZERO_COPY`.
  - Memory kinds `numa` / `numa_pinned` allocate on the NUMA node affine to the GPU. — [README.md:148-190](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md) (accessed 2026-10-29)
- **Per-column nvCOMP compression.**
  - `GQE_COMPRESSION_FORMAT` accepts `none, ans, lz4, snappy, cascaded, bitpacking, gdeflate, deflate, zstd, gzip, bitcomp`.
  - A ratio threshold (`GQE_COMPRESSION_RATIO_THRESHOLD`, default 1.0) stores incompressible columns raw.
  - A secondary format is used when its ratio is ≥1.5× the primary's.
  - Chunk size is 2^16 bytes by default.
  - `GQE_DECOMPRESSION_BACKEND` = `auto|de|sm` selects Blackwell's hardware Decompression Engine (DE) vs SMs. — [README.md:167-175](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/README.md); [include/gqe/types.hpp:790-810](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/types.hpp#L790) (accessed 2026-10-29)
- **Column layouts.** `contiguous, compressed_sliced, string_compressed_sliced, large_string_compressed_sliced`.
  - Shared tables protect `append` with an interprocess mutex, so in-memory tables are appendable. — [include/gqe/storage/in_memory.hpp:125-190](https://github.com/rapidsai/gqe/blob/50facc46fb99ecf4a955697e78803707335814b3/include/gqe/storage/in_memory.hpp#L125) (accessed 2026-10-29)

**peacockdb today**
- The scan is one `cudf::io::read_parquet(opts)` call per scan, with no cache. — `cpp/src/operators/scan.cpp:99` @f36a68ec (local repo)

### Inferences
- **Cache unit.** A row group per column (cuDF column) maps naturally onto HeavyDB's chunk = (fragment, column). Key it by (path, mtime/etag or snapshot id, row-group idx, column id). Invalidation is then a key miss rather than a mutation, which avoids the Sirius #106 class of bug.
- **Ingestion.**
  - Accept micro-batches (HeavyDB-style batch_size, e.g. 10k-1M rows) into a pinned-host Arrow/cuDF delta buffer.
  - Seal it into an immutable GPU chunk when it reaches a row-group size or a time bound (PG-Strom's 5 s / 25% thresholds are a reasonable default) (estimate).
  - Queries read sealed chunks plus the current delta. Snapshot visibility is "rows up to published count", as in HeavyDB's shadowNumTuples.
- **Eviction.** Use pin counts per chunk during query execution plus LRU or priority (Kinetica 1-10). Keep watermarks, e.g. Kinetica's 90%/50% high/low on VRAM, because cuDF operators need large transient memory. Sirius showed eviction of intermediates costing ~84 GB of round-trip traffic.
- **Tier choice (Sirius result):** compress on the GPU tier, keep the host tier raw unless the link is slow. On PCIe Gen4/Gen5 (~25-57 GB/s effective) host-tier compression likely does pay, unlike on GB300's 370 GB/s C2C (estimate). Snappy at ~117-260 GB/s decode is well above PCIe rates.

### Gaps
- HeavyDB's UPDATE/DELETE path, and how it invalidates GPU-cached chunks (chunk epochs/versioning), was not traced in code.
- Kinetica's ingest path (multi-head ingest, streaming) and default eviction priorities were not fetched.
- PG-Strom source was not cloned (clone failed), so there are no SHA/line citations for pg-strom.
- GQE's append/ingest semantics beyond the mutex-protected `append` were not traced, and neither was whether its in-memory tables live on device or host by default.

## Q2. Compressed segments in VRAM (nvCOMP) vs lightweight-encoded columns operated on directly

### Takeaway
1. **Capacity.** Lightweight encodings (bitpack, FOR/delta, dictionary) typically give ~2.5-3× on TPC-H-like data. Sirius saw 264 → 93 GB on lineitem's hot columns.
2. **Decode speed.** These encodings decode at 500-1700 GB/s on modern GPUs. General-purpose nvCOMP codecs (LZ4, Snappy, zstd, gzip) decode at only ~80-270 GB/s even with Blackwell's hardware engine. ANS, Bitcomp and Cascaded reach ~200-540 GB/s.
3. **Filtering on encoded data.** Bit-packed and dictionary forms allow filter-while-decode and late materialization. Sirius measured 2.1-2.6× gains for dictionary-coded string predicates.
4. **cuDF support.** cuDF supports DICTIONARY columns (any signed-int indices, fixed-width or string keys) in groupby (keys and many aggregations), join, gather/copy, concatenate, merge, search and some transforms. It does not support them in binary ops, and it has no run-end or bit-packed column type. The newest cuDF Parquet reader can emit strings as DICTIONARY32.
5. **Recommendation for peacockdb.** Store hot numeric columns lightly encoded (bitpack/FOR) or raw, and strings as dictionary. Decode in-kernel at scan, or keep string columns as dictionary through filter/groupby/join. Reserve nvCOMP general codecs for the host tier.

### Cited Findings

**Capacity / ratios**
- **Sirius:** seven hottest TPC-H SF1000 lineitem columns are 264 GB raw vs ~93 GB compressed (~2.84×). `o_comment` snappy gives 2.6×. — [sirius compressed-pinning.md:64-82](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **B200 Decompression Engine microbenchmark** (100 MB datasets, 64 KB chunks): measured ratios on their data:

  | Codec | Ratio |
  |---|---|
  | lz4 | 1.00× |
  | snappy | 1.91× |
  | zstd | 2.00× |
  | gzip | 2.00× |
  | bitcomp | 3.00× |

  The test data is likely synthetic, so these ratios do not represent TPC-H. — [Microbenchmarking NVIDIA's Blackwell, arXiv 2512.02189 Table I](https://arxiv.org/html/2512.02189v2) (accessed 2026-10-29)
- **GSST string compressor (A100):** ratio 2.74 at 191 GB/s decompression. It notes nvCOMP ANS and Bitcomp decompress faster but compress less. — [search summary of TU Delft GSST thesis](https://repository.tudelft.nl/file/File_80803adf-2ac6-4a3a-9c5d-9895e5b9297c) (accessed 2026-10-29; secondary)
- **Gap:** I found no primary source for a Parquet-on-disk vs in-memory Arrow size ratio for TPC-H or ClickBench.

**Decode throughput**
- **Lightweight plans (Sirius, GB300):**

  | Encoding | Decode throughput |
  |---|---|
  | bitpack / ans / delta | 580-1700 GB/s |
  | dictionary strings | ~250 GB/s |
  | snappy long text | 230-270 GB/s |

  Source: [sirius compressed-pinning.md:64-80](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **nvCOMP on B200 hardware Decompression Engine** (input / output GB/s):

  | Codec | Input GB/s | Output GB/s |
  |---|---|---|
  | lz4 | 173 | 173 |
  | snappy | 61 | 117 |
  | zstd | 77.5 | 155 |
  | gzip | 42 | 84 |
  | cascaded | — | 213 |
  | bitcomp | 154 | 462 |
  | ans | — | 539 |

  Latencies are 0.19-1.25 ms per 100 MB. — [arXiv 2512.02189 Table I](https://arxiv.org/html/2512.02189v2) (accessed 2026-10-29)
- **NVIDIA marketing:** the Blackwell DE "achieve[s] up to 600 GB/s decompression throughput for standard formats". — [NVIDIA nvCOMP page](https://developer.nvidia.com/nvcomp) (accessed 2026-10-29)
- **nvCOMP release notes (relative gains only, no absolute numbers):**
  - LZ4 decompression up to 1.4× faster on H100/A100 in v3.0.
  - Snappy 1.3× faster on H100, 1.9× on A100.
  - Bitcomp 7-8× faster for very small files in v4.1.

  Source: [nvCOMP release notes](https://docs.nvidia.com/cuda/nvcomp/release_notes.html) (accessed 2026-10-29)
- **ANS on H100:** nvCOMP ANS measured at 480 GB/s end-to-end, vs DietGPU at ~592 GB/s, in a third-party paper. — [search summary, arXiv 2608.02162](https://arxiv.org/pdf/2608.02162) (accessed 2026-10-29; secondary, not verified in PDF)
- **Old A100 nvCOMP deck:** a single headline figure of 69.46 GB/s decompression, codec unspecified. — [GTC CWE41740 slides](https://reprises.in2p3.fr/ressource/Presentation/CWE41740_Slide_Deck_1648076975326001TKn2.pdf) (accessed 2026-10-29; secondary)
- **Tile-based lightweight integer compression (Shanbhag, Yogatama, Yu, Madden, SIGMOD 2022, DOI 10.1145/3514221.3526132):**
  - Proposes GPU-FOR, GPU-DFOR and GPU-RFOR as bit-packing formats, decoded as device functions inside query kernels (tile-based, Crystal-style).
  - A secondary summary claims nvCOMP-level ratio with 2.2× faster decompression and 2.6× faster queries. I did not verify this against the paper. — [MIT DSpace record](https://dspace.mit.edu/handle/1721.1/146390); [code: anilshanbhag/gpu-compression](https://github.stncp.dns-dynamic.net/anilshanbhag/gpu-compression) (accessed 2026-10-29)
- **"Do GPUs Really Need New Tabular File Formats?" (arXiv 2602.17335; A100, TPC-H SF300 lineitem, GDS from local NVMe):**
  - Fully compliant Parquet tuned for GPUs reaches "up to 125 GB/s effective read bandwidth", defined as decoded logical bytes ÷ scan time.
  - Tuning means ≥100 pages per column chunk, because cuDF maps pages to grid size, and row groups of ~10M rows instead of DuckDB's 122,880 (~100 KB chunks).
  - A secondary summary gives a 3 GB/s baseline with DuckDB-default files.
  - It cites "up to 5700 GB/s" in-GPU decoding for GPU-friendly encodings from other work. — [arXiv 2602.17335](https://arxiv.org/html/2602.17335) (accessed 2026-10-29)
- **Polars GPU Parquet reader on H100:** a plot reached ~100 GB/s, computed as data size ÷ read time. A commenter questioned this against PCIe Gen5's ~64 GB/s per direction, suggesting the numerator is the uncompressed size. — [NVIDIA forum thread](https://forums.developer.nvidia.com/t/efficiently-scaling-polars-gpu-parquet-reader/329964) (accessed 2026-10-29)

**Operating on encoded data in Sirius (filter while decompressing)**
- **Suite result.** On GB300 SF1000 the suite went from 7.866 s to 6.918 s, and q12 improved by 35.8%.
- **What each plan root allows:**

  | Plan root | Behaviour |
  |---|---|
  | `bitpack` | evaluates ranges into a selection mask, so rejected rows are never unpacked |
  | `delta→bitpack` | stores only survivors |
  | `dictionary`+bitpack codes | answers equality/IN on keys, gathers only survivors, 2.1-2.6× at every selectivity |
  | `identity` / entropy-coded | decodes full width, then compacts by gather |

  Source: [compressed-pinning.md:99-140](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)

**cuDF dictionary support (libcudf)**
- **Type definition.** A DICTIONARY column has a keys child (any fixed-width or STRING type) and an indices child of any signed integer type (INT8-INT64).
  - `dictionary::encode()` is non-deterministic: key order may differ.
  - APIs accept non-unique keys. Outputs generally have unique keys in unspecified order. — [cudf DEVELOPER_GUIDE.md "Dictionary columns" @785ee522](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/doxygen/developer_guide/DEVELOPER_GUIDE.md) (accessed 2026-10-29)
- **Public dictionary APIs.** `encode`, `decode`, `get_index` (search), `add_keys`, `remove_keys`, `remove_unused_keys`, `set_keys`, `match_dictionaries`, `make_dictionary_column`. — [cpp/include/cudf/dictionary/*.hpp, e.g. encode.hpp:53,73; update_keys.hpp:52-160](https://github.com/rapidsai/cudf/tree/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/dictionary) (accessed 2026-10-29)
- **Binary ops do not support dictionaries.** `cudf::binary_operation` throws `cudf::data_type_error` if lhs, rhs or output type "is a dictionary type". Comparison filters on a dictionary column therefore need decode, or a key-space compare (compare keys, then gather a boolean by indices). — [cpp/include/cudf/binaryop.hpp:170,204,237](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/binaryop.hpp#L170) (accessed 2026-10-29)
- **JIT transforms accept dictionary inputs.** CUDA-supported transform inputs/outputs include "fixed-width, string, and their dictionary types". — [cpp/include/cudf/transform.hpp:247-250](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/transform.hpp#L247) (accessed 2026-10-29)
- **Test coverage** (vendored cudf @43505bb9, count of test files exercising dictionary columns):

  | Area | Files |
  |---|---|
  | groupby (keys_tests `groupby_dictionary_keys_test`; also min, count, mean, var, nunique, sum_of_squares, covariance) | 20 |
  | dictionary | 10 |
  | copying | 5 |
  | interop | 5 |
  | rolling | 3 |
  | replace | 3 |
  | io | 2 |
  | join (`JoinDictionaryTest, LeftJoinNoNulls` at join_tests.cpp:2311) | 1 |
  | search, reductions, merge, quantiles, unary, transform | 1 each |
  | sort/ | 0 |
  | stream_compaction/ | 0 |
  | binaryop/ | 0 |

  Source: local `third_party/cudf/cpp/tests/**` (grep `dictionary::encode|dictionary_column_wrapper`) (accessed 2026-10-29)
- **Parquet reader can emit DICTIONARY32.** "When true, the reader outputs STRING columns as DICTIONARY32 encoded columns… INT32 indices child and a STRING keys child." With AST/JIT filters set, "the direct transcode fast path is disabled": strings are materialized, filtered, then re-encoded. — [cpp/include/cudf/io/parquet.hpp:344-356 (`is_enabled_output_dict_columns`)](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/parquet.hpp#L344) (accessed 2026-10-29)
- **Experimental hybrid_scan reader (filter-then-materialize).** It reads in two passes:
  1. Only filter columns are read, pruning row groups via stats, bloom filters and dictionary pages, and pages via the page index, to produce a row mask.
  2. Payload columns are read with the surviving mask, pruning their pages.

  APIs include `filter_row_groups_with_stats`, `filter_row_groups_with_bloom_filters`, `filter_row_groups_with_dictionary_pages`, `build_row_mask_with_page_index_stats`, `materialize_filter_columns`, `materialize_payload_columns`, and an option "to skip decompression" of pruned pages. A multifile variant exists (`hybrid_scan_multifile.hpp`). — [cpp/include/cudf/io/experimental/hybrid_scan.hpp:27-63,157-170,500-760](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/io/experimental/hybrid_scan.hpp) (accessed 2026-10-29)
- **No run-end or bit-packed type.** No run-end-encoded type appears in cudf public headers (grep of `cpp/include/cudf` for run_end/RUN_END found nothing). Arrow REE is not a cudf type_id. — [cudf types.hpp](https://github.com/rapidsai/cudf/blob/785ee522f5688f485fc0b1eabca7f8d6d2adea0e/cpp/include/cudf/types.hpp) (accessed 2026-10-29)

### Inferences
1. **Option (a), nvCOMP general codecs in VRAM.** At ~100-270 GB/s output (snappy, zstd, lz4 even on DE), decoding a 50 GB working set costs ~0.2-0.5 s per query. That breaks a sub-second budget on its own (estimate). Only ANS, Bitcomp, Cascaded and bitpacking (~200-540 GB/s on B200; 580-1700 GB/s for Sirius's bitpack plans) are cheap enough. Even so, decode writes full-width columns into HBM, adding traffic.
2. **Option (b), lightweight encoding with direct operation.** Bitpack/FOR plus dictionary is roughly as compact on TPC-H-like integer, date and key columns (~2.5-3×) and enables predicate-on-codes and late materialization.
   - Its limit is that cuDF operators mostly need decoded fixed-width input. The exceptions are dictionary columns in groupby, join, gather and concat.
   - So "operate on encoded" in peacockdb means:
     - (i) keep strings as DICTIONARY32 through filter (via key-space evaluation), groupby keys and join keys;
     - (ii) for bit-packed numerics, use a custom fused unpack+filter kernel, or JIT transform, at scan that emits a selection mask plus survivors (Sirius-style), then hand plain cuDF columns downstream.
3. **peacockdb changes needed.**
   - **Scan** (`cpp/src/operators/scan.cpp:99`): read from cache, apply a fused decode+filter, optionally set Parquet `output_dict_columns`, or switch to hybrid_scan for selective filters on cold data.
   - **Filter / expression eval:** handle dictionary columns. binary_operation throws, so either decode or compare on keys and map indices.
   - **Join and groupby:** both sides must share key sets (`dictionary::match_dictionaries` / `set_keys`) before joining dictionary keys. Otherwise the indices mean different things.
   - **Sort:** I found no sort tests with dictionary in vendored cudf, so plan to decode before ORDER BY or sort by key-rank (estimate).
   - **Output / wire:** decode dictionaries before Arrow export, or export as an Arrow dictionary type.
   - **DataFusion planner side:** expose the cached schema as `Dictionary(Int32, Utf8)` where appropriate.
4. **Encoding-agnostic headroom.** Ingesting on the GPU via hybrid_scan with page pruning reduces both IO and decode for selective dashboard filters, even without a cache.

### Gaps
- No NVIDIA-official per-codec nvCOMP decompression GB/s on A100/H100/H200 was found. The nvCOMP developer page gives only the 600 GB/s DE headline, and the B200 table is third-party.
- FastLanes/ALP-on-GPU and Crystal numbers were not fetched. The "5700 GB/s" figure is cited second-hand via arXiv 2602.17335.
- The GPU-FOR paper's actual tables were not read (the PDF was not fetched).
- I found no primary source for Parquet vs in-memory size ratios on TPC-H or ClickBench.
- cuDF per-op dictionary support was inferred from headers and tests, not from an official support matrix. sort, stream_compaction (distinct) and reductions should be verified.

## Q3. Data-path bandwidth: does an IO-fed GPU engine need a RAM/VRAM cache?

### Takeaway
Yes. Every off-GPU path is 1-2 orders of magnitude below HBM.

| Source | Bandwidth | Relative to H200 HBM |
|---|---|---|
| H200 HBM | 4.8 TB/s | 1× |
| PCIe Gen5 H2D (measured) | ~57 GiB/s | ~80× slower |
| GH200 C2C (measured) | ~145-380 GB/s | 13-33× slower |
| 4×NVMe + GDS, effective decoded Parquet (A100 paper) | ~125 GB/s | ~38× slower |
| 400 GbE NIC (line rate) | 50 GB/s | ~100× slower |

A sub-second dashboard over billions of rows (tens of GB of touched columns) is feasible only if the touched columns sit in HBM (~10 ms per 50 GB) or, at worst, in pinned host RAM over C2C. Over PCIe or from SSD, a 50 GB scan takes ~0.4-1 s before any compute.

### Cited Findings
- **H200 SXM / NVL:** 141 GB, 4.8 TB/s HBM, NVLink 900 GB/s, PCIe Gen5 128 GB/s (bidirectional). H200 is "1.4X" H100's bandwidth, consistent with H100 SXM 3.35 TB/s. — [NVIDIA H200 page](https://www.nvidia.com/en-us/data-center/h200/) (accessed 2026-10-29)
- **L40S:** 48 GB GDDR6, 864 GB/s, PCIe Gen4 x16, 64 GB/s bidirectional. — [NVIDIA L40S page](https://www.nvidia.com/en-us/data-center/l40s/) (accessed 2026-10-29)
- **L20:** 48 GB GDDR6, 384-bit, 864 GB/s, PCIe Gen4 x16, 275 W. — [TechPowerUp L20](https://techpowerup.com/gpu-specs/l20.c4206) (accessed 2026-10-29)
- **B200:** 192 GB HBM3e, ~8 TB/s. One comparison site lists 180 GB for the SXM6 SKU. — [Jarvislabs B200 specs](https://jarvislabs.ai/ai-faqs/nvidia-b200-specs); [fpsbench B200 vs L20](https://fpsbench.com/compare/gpu/nvidia-b200-sxm6-vs-nvidia-l20) (accessed 2026-10-29)
  - Measured B200 STREAM Triad is 4.14 TB/s, about 51.8% of the 8 TB/s peak. — [arXiv 2512.02189 Table XIII](https://arxiv.org/html/2512.02189v2) (accessed 2026-10-29)
- **Measured H2D:**
  - H200 (PCIe Gen5): 57 GiB/s.
  - GH200 (NVLink-C2C): 135 GiB/s with a 500 MiB async-copy test; C2C is 900 GB/s total, 450 GB/s per direction. — [Verda/DataCrunch GH200 data movement](https://verda.com/blog/data-movement-in-the-nvidia-gh200-grace-hopper-superchip) (accessed 2026-10-29)
  - nvbandwidth on GH200 copy engines: ~377-380 GB/s H2D (~84% of 450 GB/s). — [dnhkng GH200 benchmarking](https://dnhkng.github.io/posts/gh200-benchmarking/) (accessed 2026-10-29)
  - Sirius cites ~370 GB/s host→GPU on GB300 C2C. — [sirius compressed-pinning.md:64](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md) (accessed 2026-10-29)
- **GPUDirect Storage + cuDF:** "up to 3-4× higher" read throughput, 30-50% higher on average across 48 Parquet cases. No absolute GB/s is given. — [NVIDIA blog: GDS with cuDF](https://developer.nvidia.com/blog/boosting-data-ingest-throughput-with-gpudirect-storage-and-rapids-cudf) (accessed 2026-10-29)
- **GPU-tuned Parquet via GDS (A100, local NVMe):** up to 125 GB/s effective (decoded) bandwidth, vs ~3 GB/s with DuckDB-default Parquet (the 3 GB/s figure is from a secondary summary). — [arXiv 2602.17335](https://arxiv.org/html/2602.17335) (accessed 2026-10-29)
- **Not sourced this session (estimate):**
  - NIC line rates by arithmetic: 100 GbE = 12.5 GB/s, 200 GbE = 25 GB/s, 400 GbE / NDR IB = 50 GB/s.
  - Single NVMe Gen4 ~7 GB/s and Gen5 ~14 GB/s sequential read.
  - PCIe Gen4 x16 pinned H2D ~25 GB/s, and pageable typically ~50-70% of pinned.
  - A100 HBM 2.0 TB/s (80 GB) and 1.55 TB/s (40 GB).

### Inferences
**Worked example (estimate).** A dashboard over 2B rows touches 5 columns × 8 B ≈ 80 GB uncompressed. Per-query read time by source:

| Source | Bandwidth | Time for 80 GB |
|---|---|---|
| H200 HBM | 4.8 TB/s (~2.4 TB/s realistic, given B200's ~52% STREAM efficiency) | ~17-35 ms |
| GH200 pinned host via C2C | ~380 GB/s | ~210 ms |
| PCIe Gen5 pinned | ~61 GB/s | ~1.3 s |
| PCIe Gen4 | ~25 GB/s | ~3.2 s |
| 4×NVMe + GDS (tuned Parquet, effective) | ~125 GB/s | ~0.64 s, if the data is on local NVMe at all |
| 400 GbE | 50 GB/s | ~1.6 s |

Implications:
- **The thesis holds.** IO-fed, the GPU's 4.8 TB/s advantage collapses to the link rate, which a 96-core CPU on local DRAM (~300-460 GB/s, estimate) can match.
- **Compression raises the ceiling** by about the ratio (~2.8×) on narrow links only if decode exceeds the link rate. Every lightweight codec, and even snappy, exceeds PCIe rates. On C2C, only the fast codecs do (Sirius).
- **VRAM capacity bounds the hot set:**

  | GPU | Raw | At ~2.8× compression |
  |---|---|---|
  | L20 / L40S | 48 GB | ~130 GB |
  | H100 | 80 GB | ~220 GB |
  | H200 | 141 GB | ~390 GB |
  | B200 | 192 GB | ~540 GB |

  Leave ~30-50% headroom for operator intermediates (Kinetica's 90%/50% VRAM watermarks; Sirius's eviction episodes). Billions of rows × a few columns fit on one H200-class GPU. The L20/L40S class needs compression plus a host-RAM tier.

### Gaps
- No NVIDIA-official measured PCIe Gen4/Gen5 pinned vs pageable H2D numbers were fetched.
- NVMe Gen5 drive spec sheets and NIC/IB spec sources were not fetched; those values above are marked estimates.
- GB200 NVLink-C2C measured numbers were not fetched. Only Sirius's ~370 GB/s on GB300 is cited.

## Q4. Keeping results GPU-resident for downstream consumers

### Takeaway
The standard handoffs are:
- the Arrow C Device Data Interface (`ArrowDeviceArray`, device_type CUDA=2 with a `cudaEvent_t*` sync event), in-process;
- DLPack (same device-type enum) for tensors;
- CUDA IPC handles for cross-process sharing, which HeavyDB used for `sql_execute_gdf` (GPU Arrow over a CUDA IPC handle).

The cudf-polars issue shows real demand: Graphistry (lmeyerov) wants query results to stay on the device and chain D→D into the next query. It is open, and cudf-polars maintainers point to Polars `LazyFrame.execute(engine=…)` work as the path. For peacockdb, the in-process FFI could export `ArrowDeviceArray(Stream)` directly. An out-of-process server needs CUDA IPC/VMM handles plus lifetime management (HeavyDB's `deallocate_df`).

### Cited Findings
- **Arrow C Device Data Interface.**
  - `ArrowDeviceArray` has `device_type`, `device_id` and an optional `sync_event`. If `sync_event` is non-null, the consumer "MUST" sync on it before touching buffers.
  - Device types: `ARROW_DEVICE_CUDA (2)`, `CUDA_HOST (3)` (pinned), `CUDA_MANAGED (13)`. The sync event is `cudaEvent_t*`.
  - Device-type values match DLPack's `DLDeviceType`.
  - `ArrowDeviceArrayStream` provides chunked streaming with a single device type.
  - Exported data is immutable, and only the buffers live on the device. — [Arrow C Device Data Interface](https://arrow.apache.org/docs/format/CDeviceDataInterface.html) (accessed 2026-10-29)
- **HeavyDB GPU result export.**
  - The Thrift API has `sql_execute_df(..., device_type, device_id, first_n, transport_method)`, `sql_execute_gdf(session, query, device_id, first_n)` and `deallocate_df(session, df, device_type, device_id)`. — [heavy.thrift:550-552](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/heavy.thrift#L550)
  - `TDataFrame` carries `sm_handle`, `sm_size`, `df_handle`, `df_size`, `execution_time_ms`, `arrow_conversion_time_ms`, `df_buffer`. — [heavy.thrift:168-176](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/heavy.thrift#L168)
  - `sql_execute_gdf` simply calls `sql_execute_df` with `TDeviceType::GPU`. — [ThriftHandler/DBHandler.cpp:1491-1503](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/ThriftHandler/DBHandler.cpp#L1491)
  - Tests consume the GPU result with `arrow::cuda::CudaIpcMemHandle::FromBuffer(...)` (CUDA IPC). — [Tests/ArrowIpcIntegrationTest.cpp:182-184](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/Tests/ArrowIpcIntegrationTest.cpp#L182)
  - For GPU output, DATE is emitted as `date64` because "support for date32() is missing in cuDF". — [QueryEngine/ArrowResultSetConverter.cpp:1116-1124](https://github.com/heavyai/heavydb/blob/72c6df9f1bce2cd5b849670b38c7e7e4592c564b/QueryEngine/ArrowResultSetConverter.cpp#L1116) (all accessed 2026-10-29)
- **cudf issue #23039 (rapidsai/cudf, also reachable as NVIDIA/cudf; open; filed 2026-06-30 by lmeyerov).** Title: "[FEA] cudf-polars: keep collect() results or intermediate persist() values on-device, especially for single-GPU workflows". — [rapidsai/cudf#23039](https://github.com/NVIDIA/cudf/issues/23039) (accessed 2026-10-29)
  - **Context.** PyGraphistry's GFQL property-graph and Cypher engine has a polars-cudf backend. cudf → polars-cudf handoff stays on the device, but `LazyFrame.collect()` and `.sink_*()` always copy D→H. So polars-cudf → cudf and polars-cudf → polars-cudf chaining is impossible.
  - **Two needs:**
    1. Benchmark accuracy: GPU timings include a D→H "host-transfer tax".
    2. The real goal: true on-device chaining for iterative workloads ("a graph already resident on the GPU, queried repeatedly (e.g. a crossfilter / multi-hop traversal loop)"), paying only command plus D→D copy cost.
  - **Ask.** A supported public way, like dask-cudf `.persist()` / `.compute()`, to keep the materialized result on the GPU and feed it as input to a subsequent polars/cudf-polars query without a host round-trip.
  - **Rejected alternatives:**
    - `limit(1)`: the optimizer may prune work.
    - A private recipe: Translator over `q._ldf.visit()` with `GPUEngine(executor="in-memory")`, `translate_ir()`, `ir.evaluate()`. It is private API ("caveat emptor"), in-memory executor only (not the default streaming executor), exposes few methods, and cannot feed a later query.
  - **Scope.** Single-node single-GPU is primary. "The lack of D->D… eliminates utility for many of our non-trivial use cases".
  - **Responses:**
    - wence- (2026-06-30): partial progress via supporting Polars `PythonScan` (cudf #22867).
    - Matt711 (2026-06-30): Polars-side work remains, tracked in #22917, "specifically the gap around extension support for `pl.LazyFrame.execute(engine=...)`".
    - No resolution as of 2026-10-29.

### Inferences
- **The GPU-resident result is a differentiator** for the Graphistry/cuGraph/PyTorch crowd. peacockdb could expose it in three ways:
  - (i) In-process (Python/Rust FFI): return `ArrowDeviceArrayStream` built from `cudf::to_arrow_device` (cudf has interop support; 5 interop test files use dictionary). Zero copy, with the result's lifetime tied to the release callback.
  - (ii) Cross-process: CUDA IPC (`cudaIpcGetMemHandle`) or VMM fabric handles per buffer, plus an explicit `deallocate` RPC like HeavyDB's `deallocate_df`. Results must come from a non-pooled or exportable allocation, because RMM pool sub-allocations need care with IPC (estimate).
  - (iii) Results as cache entries: a downstream query can reference a prior result table by id, D→D chaining as in the cudf issue, reusing the cache machinery from Q1.
- Dictionary-encoded result columns map directly to Arrow dictionary arrays, which keeps exported results small.

### Gaps
- I did not verify whether current HeavyDB server code still produces CUDA IPC handles: no `cudaIpcGetMemHandle` was found outside tests in the open-source tree. The GPU IPC path may live in Arrow's CUDA library calls via `ArrowResultSetConverter`, which was not traced.
- cuGraph and PyTorch ingestion specifics (`__cuda_array_interface__`, `torch.from_dlpack`) were not fetched.
