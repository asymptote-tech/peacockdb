# Sirius memory management: caching (host RAM / VRAM) and spilling

Scope: Sirius at `b9e05ffc653c8ed031c789e5c1415ac1186e0619` (committed 2026-10-28; abbreviated below as `@b9e05ffc`), its cuCascade submodule at `1a9281093a0fe49634594d541d7ce327129f53d6` (committed 2026-10-21; abbreviated `@1a92810`), and the legacy engine archive `f79a6f423fe69ef892ca0af03bef9c352d88cc79` (2026-09-23; abbreviated `@f79a6f42`). Code links are GitHub permalinks at those SHAs. All web pages accessed 2026-10-28.

Short names used below:
- **"Super Sirius"** or **`gpu_execution`** is the current engine.
- **"legacy"** means the `gpu_processing` engine together with its `gpu_buffer_init` call.

## 1. Cache: what, where, persistence, population, eviction, format

### Takeaway
The current engine has no automatic cross-query table cache. It offers two opt-in mechanisms:
- **`pin_table`.** An explicit SQL call that materializes chosen columns of a Parquet file or DuckDB table once. The copy lives in VRAM (`tier='gpu'`) or pinned host RAM (`tier='host'`), as decoded cuDF columns, optionally Simpatico-compressed. It lasts until `unpin_table` and is never evicted.
- **A file-byte prefetching cache.** Raw file byte ranges are read with `O_DIRECT` into pinned host blocks. It is off by default (`mode: none`), uses LRU or idle-drop eviction, and is keyed by file identity.

The legacy engine was different. It automatically cached every scanned column in a pre-allocated GPU "caching region" on first use and required the data to fit.

### Cited Findings

**Current engine: no implicit table cache**
- The README says: "Sirius reads table data from storage on every query." For hot runs you call `pin_table`, which "materializes a table's columns into memory once, and subsequent queries over that source are served straight from the pinned copy, skipping file I/O and decode entirely. Queries don't change — pinned tables are matched automatically." — [docs/README.md:150-153 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L150-L153) (accessed 2026-10-28)

**`pin_table` interface and tiers**
- Syntax, from [docs/README.md:156-173 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L156-L173) (accessed 2026-10-28):
  - Parquet: `CALL pin_table('/path/*.parquet', name='lineitem', tier='gpu', cols=[...])`.
  - DuckDB base table: `CALL pin_table(format='duckdb', name='my_table', tier='gpu')`.
  - Release: `CALL unpin_table('lineitem')`.
  - `tier='gpu'` is "for the fastest scans". `tier='host'` uses "pinned host memory instead, for tables larger than GPU memory".
- Pinned-table metadata (`pinned_entry` in [src/scan_manager/sirius_scan_manager.hpp:203-257 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/scan_manager/sirius_scan_manager.hpp#L203-L257), accessed 2026-10-28):
  - Entries are keyed by table name in the scan manager.
  - GPU tier, uncompressed: "one chunk vector per pinned column name".
  - HOST tier: "one chunk per emitted batch … each holding all pinned columns".
  - GPU tier with compression: `device_pin_chunk`s.
  - A `cucascade::memory::Tier tier` field selects which storage is used.
  - Each entry also carries a cache identity (`cache_entry_info`), zone maps, and an MVCC mask cache for DuckDB-native pins.
- Host-tier pins bound peak GPU residency to about one batch while pinning ("Peak GPU residency is bounded to ~one batch on the host tier") — [src/pin_table.cpp:213-217 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/pin_table.cpp#L213-L217) (accessed 2026-10-28)

**Pinned GPU data is not spillable**
- A code comment states that the downgrade path "cannot evict the pinned GPU entry, so the query deadlocks" if a compressed pinned batch is under-reserved. Pinned GPU entries are therefore outside the spill candidate set — [src/op/scan/sirius_gpu_scan_operator_data.cpp:555-560 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/op/scan/sirius_gpu_scan_operator_data.cpp#L555-L560) (accessed 2026-10-28)
- Consistent with that, spill candidates come only from query-scoped data repositories and the pipeline task queue. Pins are not among them — [docs/super-sirius/memory-management.md:120-127 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L120-L127) (accessed 2026-10-28)

**Cached format and compression**
- Pins are decoded columns stored as cuDF tables. They can optionally be Simpatico-compressed (bitpack, ANS, delta, dictionary, snappy via nvCOMP), driven by per-table plan files. Compression is decided per chunk and falls back to uncompressed when the plan or savings are missing. Scans decompress at task-prepare time — [docs/super-sirius/compressed-pinning.md:1-25 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md#L1-L25) (accessed 2026-10-28)
- Retention gates are `pin_table_compression_min_batch_size_bytes` (1 MiB) and `pin_table_compression_max_compressed_fraction` (0.75) — [docs/super-sirius/configuration.md:197-202 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L197-L202) (accessed 2026-10-28)

**Measured effect of pin tier and compression** (GB300, TPC-H SF1000, 22-query hot suite; Sirius's own numbers)

| Configuration | Hot-run total | vs. all-host-pinned uncompressed (20.74 s) |
|---|---|---|
| All host-pinned, compressed | 22.20 s | +7.0% |
| Lineitem GPU-compressed (dimensions GPU raw) | 17.05 s | −17.8% |
| Also orders GPU-compressed | 16.66 s | −19.7% |

- Explanation given: host-tier compressed loses because every batch pays fetch → sync → decode on the critical path. Raw host pins stream at "~370 GB/s on GB300 C2C".
- Compression is what makes GPU residency fit: "seven hottest lineitem columns are 264 GB raw but ~93 GB compressed".
- Source: [docs/super-sirius/compressed-pinning.md:46-69 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/compressed-pinning.md#L46-L69) (accessed 2026-10-28)

**Pins over DuckDB-native tables**
- Deletes and committed inserts are reconciled per query.
- `UPDATE`, `MERGE … UPDATE` and `INSERT … ON CONFLICT DO UPDATE` are rejected while the table is pinned.
- An explicit `CHECKPOINT` makes the pin ineligible: "subsequent queries fall back or error until the table is unpinned and pinned again".
- Source: [docs/README.md:175-179 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L175-L179) (accessed 2026-10-28)
- Parquet pins need no visibility reconciliation ("immutable sources") — [src/scan_manager/sirius_scan_manager.hpp:284-293 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/scan_manager/sirius_scan_manager.hpp#L284-L293) (accessed 2026-10-28)

**Pin zone maps**
- Per-chunk min/max statistics are captured at pin time and used to skip cached chunks. `enable_pinned_zone_map_pruning` defaults to true — [docs/super-sirius/configuration.md:899-906 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L899-L906) (accessed 2026-10-28)

**File-byte prefetching cache (`scan_manager.cache`)**
- Modes (default `none`), from [src/io/cache/config.hpp:26-34, 104-115 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/io/cache/config.hpp#L26-L115) (accessed 2026-10-28):
  - `none`: "O_DIRECT reads, no cache anywhere".
  - `os`: buffered reads through the kernel page cache.
  - `sirius`: "O_DIRECT reads into Sirius's own pinned prefetching cache".
- Eviction policy:
  - `lru` (default) keeps idle chunks for reuse and "evict[s] least-recently-used ones once the pool fills past `eviction_threshold_fraction`" (default 0.8).
  - `idle` drops chunks as soon as they go idle, so the cache acts as a prefetch staging area.
  - `min_prefetching_budget_fraction` defaults to 0.05.
- Backing memory: the pool is carved from cuCascade HOST-tier memory spaces (pinned, NUMA-aware). Chunk size equals the host memory resource's block size (default 1 MiB). It takes an up-front reservation of `min_prefetching_budget_fraction` × host capacity per NUMA arena and a budget cap of `eviction_threshold_fraction` × capacity — [src/io/cache/types.cpp:36-70 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/io/cache/types.cpp#L36-L70) (accessed 2026-10-28)
- Cache key: an identity-aware key built from `raw_file_cache_id()` (path, versioned S3 key or URL) plus object identity (size and validation tag or open generation) — [src/io/types.hpp:105-128 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/io/types.hpp#L105-L128) (accessed 2026-10-28)
- Backend limit: the kvikIO backend "runs without the prefetching cache" — [docs/super-sirius/configuration.md:446-447 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L446-L447) (accessed 2026-10-28)
- Open work: PR #2155, "fix(io): fuse S3 GETs, bound readahead and host memory used for cache". PR #1933, "feat(cache): engage eviction loop in downgrade executor" (still open, not merged), would let the downgrade executor reclaim pinned-host memory from the prefetching cache. Without it, `request_free_memory` "previously ignored" cache memory — [PR #1933](https://github.com/sirius-db/sirius/pull/1933) (accessed 2026-10-28)

**Memory prefetcher for host pins**
- An optional background uploader converts queued host-pinned scan splits to the GPU tier ahead of execution, gated on GPU headroom (`min_free_fraction` 0.4).
- It is off by default and runs only when a query has exactly one eligible GPU.
- Source: [docs/super-sirius/configuration.md:557-573 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L557-L573) (accessed 2026-10-28)

**Legacy engine: automatic GPU caching region**
- `gpu_buffer_init("1 GB", "2 GB")` sets a GPU caching region and a GPU processing region:
  - The caching region is "where the raw data is stored in GPUs".
  - The processing region holds "intermediate results … (hash tables, join results .etc)".
  - An optional `pinned_memory_size` can be given.
  - The cold run is slow because of "conversion from DuckDB format to Sirius native format. Subsequent runs would be faster since it benefits from caching on GPU memory."
  - Source: [docs/gpu_processing.md:37-45, 74 @f79a6f42](https://github.com/sirius-db/sirius/blob/f79a6f423fe69ef892ca0af03bef9c352d88cc79/docs/gpu_processing.md#L37-L74) (accessed 2026-10-28)
- The paper says "Once data is read, the buffer manager automatically caches it into the pre-allocated caching region for future reuse." The caching region sits "either in device memory or pinned host memory" and "is pre-allocated in advance". The processing region uses the RMM pool allocator — [arXiv 2508.04701v2 §3.2.3](https://arxiv.org/pdf/2508.04701v2) (accessed 2026-10-28)
- Legacy `GPUBufferManager` details, from [src/legacy/gpu_buffer_manager.hpp:109-120 @f79a6f42](https://github.com/sirius-db/sirius/blob/f79a6f423fe69ef892ca0af03bef9c352d88cc79/src/legacy/gpu_buffer_manager.hpp#L109-L120) and [src/legacy/gpu_buffer_manager.cpp:356 @f79a6f42](https://github.com/sirius-db/sirius/blob/f79a6f423fe69ef892ca0af03bef9c352d88cc79/src/legacy/gpu_buffer_manager.cpp#L356) (accessed 2026-10-28):
  - Fields `gpuCache`, `cpuCache` ("`cpuCache` will be used if `gpuCache` is full") and `gpuProcessing`.
  - Bump pointers `gpuCachingPointer` and `gpuProcessingPointer`.
  - `checkIfColumnCached(table, column)` and `ResetCache()`.
  - It throws `"Out of caching memory"` when the region is exhausted.
- The NVIDIA ClickBench blog (2025-12-15, GH200) says "Sirius can cache frequently accessed tables on the GPU"; timings are hot runs — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)

### Inferences
- **What is cached:**
  - Current engine: decoded columns (pins) and raw file bytes (prefetch cache). Not Parquet pages decoded into a page cache.
  - Not intermediate results across queries: data repositories are query-scoped ("unique *within one query*", [docs/super-sirius/data-management.md:58-70 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/data-management.md#L58-L70), accessed 2026-10-28).
- **Persistence:** pins persist across queries within one process, until `unpin_table`. I found no on-disk persistence of pins across restarts (estimate: in-memory only, since `pinned_entry` holds in-memory batches).
- **Operational model:** pins are a manual, DBA-managed hot set rather than a buffer pool. They have no eviction and no capacity governor. A GPU pin permanently removes VRAM from the spill-managed working set.
- **Relevance to peacockdb:** the closest analogues of what peacockdb lacks are a pinned-host file-byte cache (the prefetching cache) and explicit column pinning. Sirius's ~370 GB/s C2C numbers come from GB300. On PCIe hosts the case for host-tier pins over GPU-tier pins will be weaker (estimate).

### Gaps
- I could not confirm from code whether a `pin_table` call checks capacity (or reserves through cuCascade) before materializing, or what happens if a GPU pin exceeds free VRAM. No explicit `make_reservation` call appears in `src/pin_table.cpp`.
- I did not verify whether the prefetching cache survives across queries in practice (LRU keeps idle chunks, so likely yes) or how it is invalidated when a local file changes beyond the size/tag identity.

## 2. Spilling to host and disk

### Takeaway
The current engine spills at data-batch granularity through cuCascade's tiers (GPU → pinned HOST → DISK).
- **Mechanism.** Tasks reserve memory before they run. A downgrade executor per memory space converts idle `cudf::table` batches sitting in inter-pipeline repositories, or in queued tasks, to lower tiers. It works both on threshold pressure (trigger 0.8, stop 0.6 for GPU) and on demand when a reservation falls short.
- **Operators.** Joins, aggregates and sorts become out-of-core by hash or range partitioning into roughly 768 MiB partitions whose batches are spillable. Operator-internal state built within a task (hash tables, Bloom arrays) is not spilled.
- **OOM handling.** An OOM inside a task triggers rescheduling with a larger reservation, up to 100 retries. After that the query fails, and the transparent path can replay it on DuckDB CPU (`enable_duckdb_fallback`, default true).
- **Disk.** Disk spilling is off by default.

### Cited Findings

**Tiers and thresholds**
- Three tiers: GPU (Tier 0), Host Pinned (Tier 1, "Pinned pools per NUMA … Used for caching, GPU↔CPU transfer"), and Disk (Tier 2, "Spill files on mount … Last resort"). Default thresholds:

| Tier | Trigger | Stop |
|---|---|---|
| GPU | 0.8 | 0.6 |
| Host | 0.9 | 0.8 |

  Source: [docs/super-sirius/memory-management.md:7-28 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L7-L28) (accessed 2026-10-28)
- In code: GPU trigger and stop are 0.8 and 0.6 ([src/sirius_config.cpp:540-543 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_config.cpp#L540-L543)); host trigger and stop are 0.9 and 0.8 ([src/sirius_config.cpp:600-605 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_config.cpp#L600-L605)) (accessed 2026-10-28)

**Reservations**
- Pipeline tasks call `memory_space.make_reservation(estimated_size)` before executing. Allocations go through `reservation_aware_resource_adaptor`, which wraps the RMM device resource. If the reservation is exhausted, it "fails gracefully, triggering `oom_reschedule_exception`" — [docs/super-sirius/memory-management.md:77-93 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L77-L93) (accessed 2026-10-28)
- Reservation sizes come from a per-pipeline history: a ring buffer of 64 records of estimated, peak and output bytes. A weighted average of peak/estimate ratios is used. On OOM the higher peak is kept, "so each retry reserves more" — [docs/super-sirius/memory-management.md:165-185 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L165-L185) (accessed 2026-10-28)
- Known weakness: issue #2301 (open, 2026-10-22). With no history, the scan's estimate multiplies input bytes by 8×. On an RTX Pro 6000 at TPC-H SF100 this gave about 12 GiB reservations against about 2 GiB actual peak, which needlessly spilled 3.7 GiB in Q9. Disabling the monitor cut Q9 from 898 ms to 659 ms (≈26%) — [issue #2301](https://github.com/sirius-db/sirius/issues/2301) (accessed 2026-10-28)

**Downgrade executor**
- There is one executor per memory space. Its parts:
  - A processing thread on an MPMC queue.
  - An optional monitor thread (`monitor_period`, default 10 ms) with a viability gate: it probes HOST with `make_reservation_or_null` and backs off when no lower tier can accept data.
  - A worker pool.
- Source: [docs/super-sirius/memory-management.md:99-116 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L99-L116) (accessed 2026-10-28)
- Two triggers ([docs/super-sirius/memory-management.md:118-133 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L118-L133), accessed 2026-10-28):
  - **On demand:** when `gpu_pipeline_executor` gets a partial reservation, it issues `request_downgrade(predicate)`. The predicate retries `make_reservation_or_null(bytes_needed)` and stops "as soon as the reservation succeeds".
  - **Monitor:** crossing the trigger issues a request sized to reach the stop fraction.
  - Byte-targeted requests pick the "smallest candidate that still covers the remaining deficit". This fixed a "q9-class partition-spill cliff, where +0.1% of input rows doubled the spilled set".
- Candidate selection ([docs/super-sirius/memory-management.md:156-163 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L156-L163), accessed 2026-10-28):
  - First, data repositories, iterating partitions back-to-front and then batches back-to-front, choosing idle batches only.
  - Second, tasks extracted from the pipeline task queue (`mutable_pop_if`).
  - Conversion goes through `convertible_data::convert()`, which handles "state locking, memory reservation, tier conversion, and failure rollback atomically".
- Batch lifecycle under pressure: "downgrade executor upgrades to mutable, converts representation to HOST, releases back to idle". Upgrade back to the GPU happens lazily in `prepare_for_processing` / `lock_or_prepare_batch` — [docs/super-sirius/data-management.md:7-19 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/data-management.md#L7-L19) (accessed 2026-10-28)
- Open PR #2361 (2026-10-28) proposes a preload executor because "Today a spilled batch stays down there until the task that reads it is already running. That task then pays the full copy back to the GPU inside its own critical path" — [PR #2361](https://github.com/sirius-db/sirius/pull/2361) (accessed 2026-10-28)

**Copy granularity and spill compression**
- Sirius replaces cuCascade's monolithic GPU→HOST batch copy, which saw "18–28 GB single submissions", with a chunked converter. `executor.downgrade.copy_chunk_bytes` defaults to 1 GiB and overlaps DMA with prep — [docs/super-sirius/memory-management.md:135-139 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L135-L139) (accessed 2026-10-28)
- Optional spill compression (`spill_compression`, default false) encodes batches of at least `spill_min_batch_bytes` (64 MiB) on the GPU with Simpatico. They are written to pinned host blocks or to `.hpln` disk files. The batch falls back to uncompressed if the result is above 0.75 of its size or if a 0.5× encode reservation cannot be obtained — [docs/super-sirius/memory-management.md:141-154 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L141-L154); [docs/super-sirius/configuration.md:207-220 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L207-L220) (accessed 2026-10-28). The feature was merged as PR #2204 on 2026-10-27 — [PR #2204](https://github.com/sirius-db/sirius/pull/2204) (accessed 2026-10-28)

**Disk tier**
- Disk spilling is "**disabled by default** (empty `downgrade_root_dirs`)". Capacity defaults to 1 TiB; "NVMe preferred" — [docs/super-sirius/configuration.md:178-186 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L178-L186) (accessed 2026-10-28)
- When no disk tier is configured, the executor warns that "data cannot be spilled to disk. Consider configuring a disk memory space" — [src/downgrade/downgrade_executor.cpp:500-507 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/downgrade/downgrade_executor.cpp#L500-L507) (accessed 2026-10-28)

**Which operators are covered**
- Spillable state is batches in repositories at pipeline breakers: PARTITION, CONCAT, SORT_PARTITION, MERGE_GROUP_BY / MERGE_SORT inputs, and exchange or streaming queues. The streaming sink notes that "A queued batch stays spillable in the repository until pulled" — [docs/super-sirius/operators.md:150-160 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/operators.md#L150-L160) (accessed 2026-10-28)
- Partition sizing knobs: `hash_partition_bytes`, `concat_batch_bytes`, `max_build_hash_table_bytes` and `scan_task_batch_size` all default to 768 MiB. `max_sort_partition_bytes` defaults to 0, meaning 33% of GPU memory — [docs/super-sirius/configuration.md:110-116 (example config) @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L110-L116) (accessed 2026-10-28)
- Hash join has three modes. STANDARD is "multi-partition". BUILD_PROBE is used when the per-partition build is under `max_build_hash_table_bytes` and folds to one batch — [docs/super-sirius/operators.md:221-232 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/operators.md#L221-L232) (accessed 2026-10-28)
- Grouped aggregate runs as local `cudf::groupby` per batch, then PARTITION, then MERGE_GROUP_BY per partition. Sort runs as SORT_SAMPLE, then SORT_PARTITION (range), then MERGE_SORT per partition — [docs/super-sirius/operators.md:392-406, 502-531 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/operators.md#L392-L531) (accessed 2026-10-28)
- Dynamic-filter Bloom arrays "cannot spill" — [docs/super-sirius/memory-management.md:86 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L86) (accessed 2026-10-28)
- Exchange-data spilling is still in progress: PR #2061 was closed unmerged, and PR #2376 "spill early arrivals, ordered-gather batches and held stream batches" is open (2026-10-28) — [GitHub issue search](https://github.com/sirius-db/sirius/pulls?q=spill) (accessed 2026-10-28)

**When spilling cannot help**
- An OOM is rescheduled with a raised reservation floor. `MAX_RETRIES` defaults to 100 (raised from 10), with backoff. On exhaustion the query reports `late_failure_cause::oom_exhausted` — [src/pipeline/gpu_pipeline_executor.cpp:464-494 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/pipeline/gpu_pipeline_executor.cpp#L464-L494) (accessed 2026-10-28)
- `enable_duckdb_fallback` (default true) gates "both plan-time fallback (unsupported operator/type) and runtime fallback (GPU execution failure)" to DuckDB CPU — [docs/super-sirius/configuration.md:975 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L975) (accessed 2026-10-28)
- CPU replay is subject to `admit_cpu_replay`: read-only statement, transaction checks and so on — [src/transparent/replay_admission.hpp:46-61 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/transparent/replay_admission.hpp#L46-L61) (accessed 2026-10-28)
- Fragmentation recovery: `defragmenter_oom_policy` trims the CUDA async pool (`cudaMemPoolTrimTo`) when reserved > used + 10× request, then retries — [docs/super-sirius/memory-management.md:193-204 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L193-L204) (accessed 2026-10-28)

**cuCascade (NVIDIA)** at `@1a92810`
- Self-description: "Tiered Memory Management … GPU (fastest), pinned host (medium), and disk", a "Memory Reservation System", "cuDF Integration … spilling to Host or Disk", and "Pluggable Policies" for OOM, over-reservation and spill selection — [cuCascade README.md:9-14 @1a92810](https://github.com/NVIDIA/cuCascade/blob/1a9281093a0fe49634594d541d7ce327129f53d6/README.md#L9-L14) (accessed 2026-10-28)
- Per-tier allocators ([cuCascade docs/memory-management.md:37-61 @1a92810](https://github.com/NVIDIA/cuCascade/blob/1a9281093a0fe49634594d541d7ce327129f53d6/docs/memory-management.md#L37-L61), accessed 2026-10-28):

| Tier | Allocator | Upstream / backing |
|---|---|---|
| GPU | `reservation_aware_resource_adaptor` | RMM `cuda_async_memory_resource` |
| HOST | `fixed_size_host_memory_resource` | NUMA-pinned 1 MB blocks |
| DISK | `disk_access_limiter` | "reservation tracking only" |

- The default `throw_on_oom_policy` "simply rethrows"; "Custom policies could spill data to a lower tier and retry" — [cuCascade docs/memory-management.md:290-305 @1a92810](https://github.com/NVIDIA/cuCascade/blob/1a9281093a0fe49634594d541d7ce327129f53d6/docs/memory-management.md#L290-L305) (accessed 2026-10-28)
- Built-in converters ([cuCascade docs/data-management.md:321-332 @1a92810](https://github.com/NVIDIA/cuCascade/blob/1a9281093a0fe49634594d541d7ce327129f53d6/docs/data-management.md#L321-L332), accessed 2026-10-28):
  - GPU table → host via `cudaMemcpyBatchAsync`, column buffers to pinned blocks, "~99% PCIe bandwidth".
  - A packed variant via `cudf::pack`.
  - Cross-GPU via pack, peer copy, unpack.
- Disk I/O goes through a pluggable `idisk_io_backend` registry — [cuCascade include/cucascade/data/disk_io_backend.hpp:43-47 @1a92810](https://github.com/NVIDIA/cuCascade/blob/1a9281093a0fe49634594d541d7ce327129f53d6/include/cucascade/data/disk_io_backend.hpp#L43-L47) (accessed 2026-10-28)
- Repo facts: GitHub repo NVIDIA/cuCascade was created 2025-12-15, has 63 stars, is Apache-2.0, and is described as "GPU Memory Reservation Library" — [gh api repos/NVIDIA/cuCascade](https://github.com/NVIDIA/cuCascade) (accessed 2026-10-28)
- Third-party framing: a GTC 2026 recap calls cuCascade the composable building blocks behind Sirius handling datasets larger than GPU memory — [GTC 2026 recap blog](https://jazracherif.github.io/nvidia/gtc/analytics/gpu/2026/04/09/accelerated-analytics-at-gtc-2026-part1-technical-deep-dives.html) (accessed 2026-10-28; secondary source, not verified in detail)

### Inferences
- **Granularity:** spilling works on whole `data_batch` objects (cuDF tables, roughly 768 MiB by default) that are idle between pipelines. There is no page-level or operator-callback spill. A running task's working set (an in-flight hash table, groupby state) is never spilled. Instead the task fails its reservation and is rescheduled.
- **Out-of-core design:** out-of-core comes from partitioning plus spilling idle partitions, the Grace-hash / external-sort style. This is a large structural difference from peacockdb's executor, which has no spilling (estimate of the porting effort: substantial; it needs a batch abstraction with tier-movable representations, reservation-aware allocation, and partitioned operators).
- **Maturity:** the spill path is young and still being tuned. Evidence: issues #727/#728 ("Improve Spill To Disk Performance", "Improved data spilling policies", open since 2026-04-28), #2301 (oversized reservations causing needless spills), and PR #2361 (no preload yet).

### Gaps
- No public benchmark quantifies spill throughput or slowdown for larger-than-VRAM workloads. Disk spill performance is acknowledged as an open item (#727 has an empty body).
- I did not confirm whether the pinned-host prefetching cache and host pins are themselves downgradable to disk. PR #1933, which would evict cache chunks under pressure, is unmerged.

## 3. Memory configuration knobs, pinned pools, RMM resources

### Takeaway
Memory is configured in YAML (`SIRIUS_CONFIG_FILE`, then `./sirius.yaml`, then `~/.sirius/sirius.yaml`) under `sirius.memory.{gpu,host,disk}`, which maps onto cuCascade memory spaces. There is a low-level `sirius.space.*` alternative, plus a few DuckDB `SET` variables for compression.
- Defaults: GPU 95% of VRAM. Pinned host 90% of each NUMA node's RAM, allocated lazily in 1 MiB-block pools (4 × 128 initially). Disk off.
- RMM: the GPU resource is a CUDA async pool wrapped by cuCascade's reservation-aware adaptor, and Sirius swaps it in as cuDF's current device resource.

### Cited Findings

**Config file and initialization**
- Search order is `SIRIUS_CONFIG_FILE`, then `./sirius.yaml`, then `~/.sirius/sirius.yaml` — [docs/README.md:216-222 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L216-L222) (accessed 2026-10-28)
- `SIRIUS_DISABLE=1` prevents initialization, because "Super Sirius claims most GPU and pinned host memory on startup" — [docs/super-sirius/configuration.md:36-38 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L36-L38) (accessed 2026-10-28)

**GPU knobs**
- `usage_limit_fraction` defaults to 0.95; the alternative is `usage_limit_bytes`.
- `reservation_limit_fraction` defaults to 1.0; the alternative is `reservation_limit_bytes`.
- `downgrade_trigger_fraction` / `downgrade_stop_fraction` default to 0.8 / 0.6.
- Source: [docs/super-sirius/configuration.md:145-160 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L145-L160) (accessed 2026-10-28)

**Host knobs**
- `capacity_fraction` defaults to 0.9 "of **each backing NUMA node's total RAM**"; the alternative is `capacity_bytes` per NUMA node via `cudaMallocHost`.
- Trigger / stop default to 0.9 / 0.8. Spill to disk "requires a configured `downgrade_root_dirs`".
- `block_size` is 1 MiB, `pool_size` is 128 and `initial_number_pools` is 4, giving an initial footprint of 512 MiB per node (my arithmetic). More pools are added on demand.
- Source: [docs/super-sirius/configuration.md:162-176 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L162-L176) (accessed 2026-10-28)
- Code agrees: the host capacity default is `0.9` ([src/sirius_config.cpp:600 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_config.cpp#L600)) and the GPU usage default is `0.95` ([src/sirius_config.cpp:540 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/src/sirius_config.cpp#L540)) (accessed 2026-10-28)
- **Conflict:** the README says the no-config defaults are "95% GPU memory, 8 GiB pinned host memory per NUMA node" ([docs/README.md:222 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L222)). configuration.md says "90% of each NUMA node's RAM" ([docs/super-sirius/configuration.md:25 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L25)). The code matches 90%, so the README figure looks stale. (accessed 2026-10-28)

**Threshold semantics**
- Below stop: normal operation. Between stop and trigger: hysteresis. Above trigger: evict. Above `reservation_limit`: "new reservations are denied (triggers OOM retry)".
- `0 < stop < trigger ≤ 1` is enforced.
- Source: [docs/super-sirius/configuration.md:222-240 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L222-L240) (accessed 2026-10-28)

**Other knobs**
- `sirius.executor.downgrade.num_threads` defaults to 1 and `monitor_period` to 10 ms (0 disables the monitor) — [docs/super-sirius/configuration.md:333-339 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L333-L339) (accessed 2026-10-28)
- `scan_manager.cache.{mode,eviction,eviction_threshold_fraction,min_prefetching_budget_fraction}` — [docs/super-sirius/configuration.md:461-498 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L461-L498) (accessed 2026-10-28)
- Compression-related DuckDB `SET` variables: `pin_table_compression`, `pin_table_input_compression_plan_dir`, `pin_table_compression_min_batch_size_bytes`, `pin_table_compression_max_compressed_fraction`, `spill_compression` — [docs/super-sirius/configuration.md:916-936 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L916-L936) (accessed 2026-10-28)

**RMM integration**
- `sirius_memory_reservation_manager` inherits cuCascade's `memory_reservation_manager`. It "Initializes all GPU memory spaces and sets cuDF device resources" and restores the previous cuDF resources on destruction — [docs/super-sirius/memory-management.md:30-39 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L30-L39) (accessed 2026-10-28)
- Device memory is "freed stream-ordered (RMM async pool)". The doc spells out a stream-ordering invariant for safe spilling under concurrency: "No owner of device memory may die … until every stream that may still read or write that memory is ordered before the free" — [docs/super-sirius/memory-management.md:217-245 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L217-L245) (accessed 2026-10-28)
- Reservation tracking defaults to per-thread. `per_stream_reservation: true` is available only via the low-level `sirius.space.gpu[]` — [docs/super-sirius/configuration.md:158-160 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/configuration.md#L158-L160); [docs/super-sirius/memory-management.md:187 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/super-sirius/memory-management.md#L187) (accessed 2026-10-28)

**Legacy knobs**
- `gpu_buffer_init(cache_size, processing_size[, pinned_memory_size])` was the only memory API. Users had to set `SIRIUS_DISABLE=1` so that Super Sirius did not grab the memory — [docs/gpu_processing.md:23-45 @f79a6f42](https://github.com/sirius-db/sirius/blob/f79a6f423fe69ef892ca0af03bef9c352d88cc79/docs/gpu_processing.md#L23-L45) (accessed 2026-10-28)

### Inferences
- **Footprint:** with defaults, Sirius aims to own 95% of VRAM and up to 90% of each NUMA node's RAM as pinned memory (grown on demand). That is an aggressive footprint for co-tenancy with other processes on the host (estimate).
- **Pattern worth copying:** the knob surface is compact and maps 1:1 onto cuCascade. A peacockdb adoption of cuCascade would inherit the same knobs almost for free (estimate).

### Gaps
- I did not confirm whether host pools are allocated with `cudaHostRegister` or `cudaMallocHost`, or whether they are NUMA-bound via libnuma, in code. The docs say "NUMA-pinned" and "`cudaMallocHost`" in different places.

## 4. Documented limits and how they changed over time

### Takeaway
- **Aug 2025 (paper):** Sirius assumed inputs fit the pre-allocated GPU caching region and intermediates fit the processing region. Spilling and batch execution were future work.
- **Dec 2025 – Jan 2026:** the new engine gained a downgrade executor (2025-12-12) and adopted cuCascade (submodule 2026-01-09).
- **May 2026:** `pin_table` replaced implicit caching (2026-05-01).
- **30 Sep 2026:** the legacy in-GPU-memory engine was deleted.
- **27 Oct 2026:** spill compression merged.

Today the docs advertise "out-of-core execution with tiered memory management (GPU/host/disk), automatic data partitioning, and spilling".

### Cited Findings

**Before: data had to fit (2025)**
- Paper (arXiv 2508.04701, v1 2025-08-06, v2 2025-08-08, CIDR'26), Future Work, "Out-of-Core Execution": "Currently, Sirius operates under the assumption that the input data fits in the caching region and the intermediate results fit in the processing region. To support larger-than-memory workloads, we plan to extend the buffer manager to support spilling to pinned memory and disk. We will also enhance the execution engine to support batch execution by partitioning input data into batches and pipelining them into the GPU." — [arXiv 2508.04701v2](https://arxiv.org/pdf/2508.04701v2); [arXiv abs page](https://arxiv.org/abs/2508.04701v2) (accessed 2026-10-28)
- The same paper's introduction: "In a traditional GPU database, data must fit in GPU memory to fully realize acceleration benefits" and argues GH200 C2C (>400 GB/s) lets an engine "support data larger than the GPU device memory" — [arXiv 2508.04701v2](https://arxiv.org/pdf/2508.04701v2) (accessed 2026-10-28)
- The NVIDIA ClickBench blog (2025-12-15) lists as a future priority "seamless spilling of data beyond physical GPU limits" — [NVIDIA blog](https://developer.nvidia.com/blog/nvidia-gpu-accelerated-sirius-achieves-record-setting-clickbench-record/) (accessed 2026-10-28)
- Legacy doc (until 2026-09): "`gpu_processing` is the in-memory execution path. It works with DuckDB's native storage format and requires the dataset to fit in GPU memory." — [docs/gpu_processing.md:3 @f79a6f42](https://github.com/sirius-db/sirius/blob/f79a6f423fe69ef892ca0af03bef9c352d88cc79/docs/gpu_processing.md#L3) (accessed 2026-10-28)

**Milestone commits**

| Date | Commit / PR | Change |
|---|---|---|
| 2025-12-12 | `505a434e5e` (#97) | "Downgrade Task, Downgrade Queue, and Downgrade Executor" |
| 2026-01-09 | `bf570fd4e3` (#144) | "Add cuCascade as a submodule" |
| 2026-05-01 | `cdd6864cab` (#721) | "Adding ability to pin tables in gpu memory" |
| 2026-09-30 | `9f30b9cacd` (#1055) | "chore: remove the legacy `gpu_processing` execution path" |
| 2026-10-27 | PR #2204 merged | Spill compression |

- Sources: [gh api commits?path=src/downgrade/downgrade_executor.cpp](https://github.com/sirius-db/sirius/commits/master/src/downgrade/downgrade_executor.cpp), [gh api commits?path=.gitmodules](https://github.com/sirius-db/sirius/commits/master/.gitmodules), [gh api commits?path=src/pin_table.cpp](https://github.com/sirius-db/sirius/commits/master/src/pin_table.cpp), [gh api commits?path=src/legacy/gpu_buffer_manager.cpp](https://github.com/sirius-db/sirius/commits/master/src/legacy/gpu_buffer_manager.cpp), [PR #2204](https://github.com/sirius-db/sirius/pull/2204) (accessed 2026-10-28)
- Caveat: the 2025-12-12 and 2026-05-01 dates are the oldest commits returned in the first page of 100 for those paths. They are probably, but not certainly, the first commits (estimate).
- The ClickBench results came from the legacy engine. The current README points to "the legacy Sirius implementation used for the ClickBench results … `legacy-dev` archive (pinned at `f79a6f42`)" — [docs/README.md:31-32 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L31-L32) (accessed 2026-10-28)

**Now (2026-10)**
- Current claim: "Execution is out-of-core with tiered memory management (GPU/host/disk), automatic data partitioning, and spilling, and works with both **Parquet** and **DuckDB-native** storage." — [docs/README.md:99 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/README.md#L99) (accessed 2026-10-28)
- **Conflict:** gpu_execution.md still says "It currently works with **Parquet** data format" — [docs/gpu_execution.md:3 @b9e05ffc](https://github.com/sirius-db/sirius/blob/b9e05ffc653c8ed031c789e5c1415ac1186e0619/docs/gpu_execution.md#L3) (accessed 2026-10-28). DuckDB-native scans and pins exist in code (`duckdb_native_gpu_ingestible`, `pin_table(format='duckdb')`), so that doc line appears stale.
- Remaining non-spillable or limiting items:
  - Pinned GPU entries cannot be evicted.
  - Bloom arrays cannot spill.
  - No preload of spilled data (PR #2361 open).
  - Exchange spill is in progress (PR #2376 open).
  - Prefetch-cache memory is not reclaimable by the downgrade executor (PR #1933 open).
  - Disk is off by default.
  - The open roadmap includes issues #727/#728 (spill-to-disk performance, spill policies).
  - Sources: see the citations in sections 1–2.

### Inferences
- **Timeline:** the "data must fit in GPU memory" constraint applied to the 2025 paper and ClickBench-era legacy engine. The current engine was designed around cuCascade spilling from late 2025. Out-of-core support is therefore about 10 months old, and much of its tuning (chunked copies, byte-targeted spills, spill compression, reservation estimate fixes) landed in Sep–Oct 2026.
- **Benchmark claims:** published headline benchmarks (ClickBench, paper TPC-H) predate the spill engine. The current TPC-H hot-run numbers use host or GPU pins. I found no published end-to-end benchmark of Sirius running out-of-core with active spilling.

### Gaps
- I could not find NVIDIA blog posts or papers specifically on cuCascade beyond its repo docs and a third-party GTC recap.
- I could not determine the exact first-release date of user-visible spilling, or whether any tagged release notes describe it. I did not inspect docs/RELEASING.md or GitHub releases.
