# Log/observability niche: data sizes, query shapes, hot-window economics for a GPU (cuDF) DataFusion plugin

All accessed 2026-10-29. Research conditions: the session's WebSearch budget ran out after 1 search, and several doc sites (docs.greptime.com, docs.influxdata.com) failed DNS/fetch. Most findings therefore come from vendor code fetched at pinned SHAs and a handful of WebFetches. Commit SHAs used:
- openobserve/openobserve `2f3d893b56015f4598f8860f9d2e586666639266`
- parseablehq/parseable `bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898`
- GreptimeTeam/greptimedb `d33c66418379463c5e7c928f3a132517d2a1fa9b`
- influxdata/influxdb `185688a5a813016c0d26ec10abbad2ca770670ed`

## Q1. Ingest volumes and compression (how many hours fit in VRAM)

### Takeaway
The best primary data point is OpenObserve's own Aug-2026 billion-log benchmark. Its realistic Kubernetes logs compress only about 3.3× against raw NDJSON (Parquet+zstd, indexes included), not the 10–30× of folklore. At 1 TB/day of raw ingest, an H100 holds roughly 2–5 hours of logs; at 10 TB/day it holds under an hour. A pinned hot window is therefore a "last few hours" proposition only for small and mid deployments.

### Cited Findings
- OpenObserve vs ClickHouse benchmark (vendor, published 2026-08-22): 1,000,000,000 records, 27-column Kubernetes logs, 2,199 GB raw NDJSON, about 4h13m of event time. Ingest ran at 60,002 rec/s and 128.1 MB/s. — [OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md)
- Same post, on-disk sizes including indexes: ClickHouse 1,026.5 GB (2.14×), OpenObserve Parquet 673.5 GB (3.26×), OpenObserve Vortex 710.7 GB (3.09×). — [OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md)
- Same post, hardware: 3× EC2 i8g.2xlarge with local NVMe and 64 GiB RAM. The dataset is far larger than RAM, and the page cache was dropped before passes. — [OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md)
- OpenObserve's default Parquet codec is zstd (`ZO_PARQUET_COMPRESSION` default "zstd", config.rs:2164 @2f3d893b). — [config.rs](https://github.com/openobserve/openobserve/blob/2f3d893b56015f4598f8860f9d2e586666639266/src/config/src/config.rs#L2164)
- Datadog Husky intro (2022-05-17): gives no events/day or bytes/day figures. It says Husky sometimes must "search PiBs of events" for one query and that log events are KB-sized. Median latency was "a few hundred ms" higher than the old system, while p95/p99/max were dramatically lower. — [Datadog Husky intro](https://www.datadoghq.com/blog/engineering/introducing-husky/)
- The Husky deep dive (2023-02-22) also gives no volume numbers; "100 trillion events" appears only in the title of a linked part-4 post, which I did not fetch. Husky is "built for large scans and aggregations rather than point lookups". — [Husky deep dive](https://www.datadoghq.com/blog/engineering/husky-deep-dive/)

### Inferences (all estimates)
- Benchmark per-row sizes: about 2.2 KB raw and about 0.67 KB Parquet+index per row. Its rate is about 520 GB raw/hour, or about 12.5 TB/day, which is the "large" end.
- The low 3.26× ratio partly reflects realistic high-entropy fields such as trace_id, span_id and message bodies, plus index overhead. NDJSON key repetition inflates raw size, so decoded Arrow is probably about 1.5–2.5× smaller than raw NDJSON (estimate, unverified).
- The worked examples below assume about 60 GB usable on an 80 GB H100 and about 110 GB on a 141 GB H200, leaving the rest for working memory. "Decoded" assumes Arrow ≈ raw / 1.5. The 10× column is an optimistic compression case.

```
ingest/day   raw GB/h   H100 60GB: @3.26×   @10×      decoded   | H200 110GB: @3.26×  @10×    decoded
100 GB       4.2        47 h                144 h     22 h      | 86 h                 264 h   40 h
1 TB         42         4.7 h               14 h      2.2 h     | 8.6 h                26 h    4 h
10 TB        417        28 min              1.4 h     13 min    | 52 min               2.6 h   24 min
OO bench     520        22 min              69 min    10 min    | 40 min               2.1 h   19 min
```
- L40S (48 GB, about 36 usable) gives about 0.6× the H100 figures. GB10 (128 GB unified) holds about as much as an H200 but has far lower bandwidth (see Q4).
- Implication: for 1 TB/day, a GPU can hold "last 2–5 hours" (H100) or "last 4–9 hours" (H200), which matches typical dashboard and alert windows (15 min to 24 h). Wide-range searches (7–30 days) would still hit object storage.

### Gaps
- No primary source for Parseable or Greptime customer ingest volumes. OpenObserve's "140× cheaper / PB-scale" claims were not re-fetched because of the search outage.
- Datadog Husky absolute volumes are unknown (part-4 post not fetched).
- No measured Arrow-in-memory size for log data was found; the 1.5× decoded figure is a guess.

## Q2. Query shapes and how full-text search is implemented (are brute-force scans needed?)

### Takeaway
The major stores have all moved to index-first designs. OpenObserve uses a Tantivy inverted index, Greptime uses bloom (default) or Tantivy fulltext indexes, and Quickwit is Tantivy. Token searches and trace_id needle lookups already return in about 25–80 ms over a billion rows, so a GPU adds little there. The remaining slow cases are non-indexable predicates (`LIKE '%x%'`, regex, `body = 'x'`) and aggregations such as top-N and filtered histograms, which take 0.6–1.5 s. The GPU's addressable niche is those.

### Cited Findings
- OpenObserve uses Tantivy: full-text indexes on body/message fields and secondary indexes on exact-value fields. `match_all()` uses the full-text index; `=` and `IN` use secondary indexes. **`LIKE '%error%'` is not indexable**, `body = 'error'` bypasses the token index, and one non-indexable branch in an OR sends the whole OR to DataFusion. `ZO_ENABLE_INVERTED_INDEX` defaults to true. — [OpenObserve Tantivy docs](https://openobserve.ai/docs/user-guide/advanced/query-tuning/tantivy-index/)
- OpenObserve benchmark, hot latencies over 1B rows on 3 nodes (ms):
  - trace_id count 26; span_id count 25; rare token 24; common token 25.
  - High-cardinality pod_name count 27; 1h histogram 63.
  - **Top-N namespaces 1,500 (Parquet) / 1,204 (Vortex).**
  - **Filtered histogram by token 595.**
  - pod_name to 100 rows 654.
  - ClickHouse took 1,891 ms on the common token and 2,949 ms on pod_name.

  The benchmark did not test `LIKE`/regex, concurrency or time-range pruning. — [OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md)
- GreptimeDB fulltext index backend enum `FulltextBackend { #[default] Bloom, Tantivy }` (column_schema.rs:786 @d33c6641). — [column_schema.rs](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/src/datatypes/src/schema/column_schema.rs#L786)
- GreptimeDB datanode config has separate `[region_engine.mito.fulltext_index]` and `[region_engine.mito.bloom_filter_index]` sections, each defaulting to create-on-flush, create-on-compaction and apply-on-query "auto". — [datanode.example.toml](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/config/datanode.example.toml)
- Datadog Husky is designed for large scans and aggregations rather than point lookups. — [Husky deep dive](https://www.datadoghq.com/blog/engineering/husky-deep-dive/)
- Performance complaints found on GitHub (none are about raw scan throughput on recent data):
  - OpenObserve "PromQL queries >20x slower than SQL" (2024-03) — [#3091](https://github.com/openobserve/openobserve/issues/3091)
  - OpenObserve "file_list query slow case with deleted" (2025-10) — [#8864](https://github.com/openobserve/openobserve/issues/8864)
  - OpenObserve "Improve scan file for wal parquet search" (2025-08) — [#7876](https://github.com/openobserve/openobserve/issues/7876)
  - OpenObserve "Metrics: size/row-count based file sizing … fix tiny files" (2026-09) — [#14846](https://github.com/openobserve/openobserve/issues/14846)
  - OpenObserve "alarm task 50+ queried very slowly" (2023-11) — [#2084](https://github.com/openobserve/openobserve/issues/2084)
  - Parseable "Slow performance when using ORDER BY" (2023-06) — [#430](https://github.com/parseablehq/parseable/issues/430)
  - Parseable "v0.4.4 Slow performance" (2023-05) — [#418](https://github.com/parseablehq/parseable/issues/418)
  - GreptimeDB "`v1/labels` returns slowly, causing grafana pending" (2025-09) — [#6980](https://github.com/GreptimeTeam/greptimedb/issues/6980)
  - GreptimeDB "Arrow Flight resource exhaustion under concurrent queries on high-series-cardinality table" (2026-04) — [#7939](https://github.com/GreptimeTeam/greptimedb/issues/7939)
  - Quickwit "Slow query performance when using sort_by_field on queries with few matches" (2026-05) — [#6440](https://github.com/quickwit-oss/quickwit/issues/6440)
  - Quickwit "phrase_prefix on random value causes slow queries and memory spike" (2024-06) — [#5086](https://github.com/quickwit-oss/quickwit/issues/5086)
  - Quickwit "Add optimization for pure count and count aggregation" (2024-05) — [#5032](https://github.com/quickwit-oss/quickwit/issues/5032)

### Inferences
- Many complaints are metadata or planning bottlenecks (file_list, labels API, tiny files, Flight resource exhaustion) or UI problems, not compute over decoded data. A GPU does not fix these. (estimate)
- The GPU-relevant residue is aggregation-heavy dashboard panels (top-N, group-by, percentiles over hours of data: 0.6–1.5 s in the OO benchmark) and ad-hoc `LIKE`/regex that indexes cannot serve. Both scale with the scanned window. (estimate)

### Gaps
- No data on what fraction of real queries are FTS vs aggregation vs needle; no vendor publishes query-mix telemetry that I could reach.
- Parseable's FTS implementation and InfluxDB 3's (assumed: no inverted index; plain DataFusion `LIKE`/regex) were not verified because docs fetches failed.

## Q3. GPU string processing speed and GPU log-search prior art

### Takeaway
The only published libcudf string figures I obtained show the generic strings API at about 40 GB/s on an RTX A6000 (768 GB/s HBM), with hand-written kernels reaching 320–340 GB/s. Library-level cuDF string operations therefore reach only about 5% of memory bandwidth unless custom kernels are written. I found no regex/contains/LIKE throughput figure or CPU baseline.

### Cited Findings
- NVIDIA "Mastering string transformations in RAPIDS libcudf" (RTX A6000, 768 GB/s theoretical). The libcudf strings API reached about 40 GB/s, a pre-allocated kernel about 150 GB/s, and an optimized two-pass kernel 320–340 GB/s. Up to about 600M elements/s. Rows averaged about 37.9 B; datasets were 600K–10M rows. No CPU baseline. — [NVIDIA blog](https://developer.nvidia.com/blog/mastering-string-transformations-in-rapids-libcudf/)
- NVIDIA's RAPIDS JSON blog gives only the "up to 150× pandas accelerator" claim, with no string or regex GB/s. — [NVIDIA blog](https://developer.nvidia.com/blog/gpu-accelerated-json-data-processing-with-rapids/)

### Inferences (estimates)
- Even at a pessimistic 20–40 GB/s, a GPU scans a 60 GB hot window in about 1.5–3 s brute force, and 1 hour of 1 TB/day logs (about 28 GB decoded) in about 1 s. That beats a non-indexed CPU scan on a few cores, but it is slower than an inverted-index token lookup (25 ms). The GPU is the tool for `LIKE`/regex/aggregation, not for token search.
- Simple `contains`/`LIKE` on Arrow string columns could approach bandwidth with custom kernels; libcudf regex (NFA per row) is likely much slower than 40 GB/s (unverified).

### Gaps
- No sourced numbers for cuDF regex/contains, Hyperscan, ripgrep or ClickHouse `LIKE` GB/s (search budget exhausted).
- Morpheus, Graphistry and academic GPU-grep papers (2023–2026) were not reached.

## Q4. Bandwidth facts

### Takeaway
Unlike every other number in these notes, these are spec-sheet figures from training knowledge: I did not re-fetch them this session, and the NVIDIA spec-page fetches timed out. Verify them before citing. The key ratio: HBM (3.35–4.8 TB/s) is about 50–100× faster than PCIe H2D, which is about 2–5× faster than a node's S3 throughput. A pinned window avoids the PCIe and S3 hops on every query.

### Cited Findings (spec values not verified this session)
- H100 SXM 80 GB HBM3 3.35 TB/s; H100 PCIe 2.0 TB/s — [NVIDIA H100](https://www.nvidia.com/en-us/data-center/h100/) (not re-verified)
- H200 141 GB HBM3e 4.8 TB/s — [NVIDIA H200](https://www.nvidia.com/en-us/data-center/h200/) (not re-verified)
- L40S 48 GB GDDR6 864 GB/s — [NVIDIA L40S](https://www.nvidia.com/en-us/data-center/l40s/) (not re-verified)
- GB10 (DGX Spark) 128 GB LPDDR5x unified, about 273 GB/s (training knowledge, not verified)
- PCIe Gen4 x16 about 32 GB/s per direction theoretical (about 25 achievable); Gen5 x16 about 64 GB/s (about 50 achievable) (training knowledge, not verified)
- S3: up to about 100 Gb/s (about 12.5 GB/s) per instance on network-optimized instances; per-prefix limits of 5,500 GET/s (training knowledge, AWS S3 performance guidelines, not re-fetched)
- Measured ingest in the OO benchmark was 128 MB/s across the cluster, far below any of these limits. — [OpenObserve blog](https://openobserve.ai/blog/openobserve-vs-clickhouse-one-billion-logs-benchmark.md)

### Inferences (estimates)
- Full scan of a 60 GB H100 window at HBM speed: about 20 ms (bandwidth-bound ideal). At libcudf-string speed (40 GB/s): about 1.5 s.
- Reloading the same 60 GB over PCIe Gen5: about 1.2 s. Over S3 at 10 GB/s: about 6 s. Pinning wins only if queries repeat over the same window (dashboards and alerts do).
- GB10's about 273 GB/s is roughly the same as a strong CPU socket (DDR5, about 300–500 GB/s), so it gives little bandwidth edge over CPU. (estimate)

### Gaps
- No primary-source verification of any spec figure this session.

## Q5. GPU demand in these communities; existing in-memory hot caches

### Takeaway
I found no user requests for GPU acceleration in any of the six repos. All of the vendors already cache recent data in host RAM or on local disk, sized at 20–50% of RAM or capped at a few GB to 500 GB. A GPU hot window would compete with, or sit on top of, these existing caches.

### Cited Findings
- `gh search issues "GPU"` (2026-10-29) returned nothing relevant:
  - openobserve/openobserve: hits only unrelated recent issues (PromQL, dashboards), e.g. [#15762](https://github.com/openobserve/openobserve/issues/15762).
  - parseablehq/parseable: zero hits.
  - GreptimeTeam/greptimedb: zero hits.
  - quickwit-oss/quickwit: zero hits.
  - influxdata/influxdb: only CPU-usage issues, e.g. [#24305](https://github.com/influxdata/influxdb/issues/24305).
  - apache/datafusion: unrelated hits, e.g. [#15582](https://github.com/apache/datafusion/issues/15582).

  Search via [GitHub issue search](https://github.com/search?q=GPU&type=issues).
- **OpenObserve** (config.rs @2f3d893b):
  - The memory cache (`ZO_MEMORY_CACHE_ENABLED`, default false) defaults to 50% of the memory limit in cluster mode and 25% in local mode, with skip_size defaulting to 50% of max_size (config.rs:3390, 4583–4599).
  - The disk cache (`ZO_DISK_CACHE_ENABLED`, default true, strategy `time_lru`) defaults to 50% of free disk, capped at 500 GB (config.rs:3427–3436, 4807–4815).
  - The memtable is sized by `ZO_MEM_TABLE_MAX_SIZE` (config.rs:2714), and `ZO_MAX_FILE_RETENTION_TIME` defaults to 600 s (config.rs:2683).
  - The Tantivy result cache is off by default (10,000 entries × 20 KB ≈ 195 MB).

  Sources: [config.rs](https://github.com/openobserve/openobserve/blob/2f3d893b56015f4598f8860f9d2e586666639266/src/config/src/config.rs#L3390); [Tantivy docs](https://openobserve.ai/docs/user-guide/advanced/query-tuning/tantivy-index/)
- **GreptimeDB** (datanode.example.toml @d33c6641):
  - The write cache is `enable_write_cache = false` in the example, though the comment says it is enabled by default with object storage; `write_cache_size = "5GiB"`, `write_cache_ttl = "8h"`.
  - In-memory caches default to fractions of OS memory with caps: `page_cache_size`, `selector_result_cache_size` and `range_result_cache_size` at 512 MB, `prefilter_result_cache_size` at 128 MB.

  Source: [datanode.example.toml](https://github.com/GreptimeTeam/greptimedb/blob/d33c66418379463c5e7c928f3a132517d2a1fa9b/config/datanode.example.toml)
- **InfluxDB 3** (serve_all.txt @185688a5):
  - `--file-cache-size` "In-memory data file cache size [default: 20%]" (env `INFLUXDB3_FILE_CACHE_SIZE`), with a recency window `INFLUXDB3_FILE_CACHE_RECENCY` (alias `parquet-mem-cache-query-path-duration`).
  - `--gen1-duration` defaults to 10m.
  - Last-N-value and distinct-value caches have 10 s eviction intervals.

  Source: [serve_all.txt](https://github.com/influxdata/influxdb/blob/185688a5a813016c0d26ec10abbad2ca770670ed/influxdb3/src/help/serve_all.txt)
- **Parseable** (hottier.rs @bbbb79ec): a per-stream local-disk hot tier with `MIN_STREAM_HOT_TIER_SIZE_BYTES = 10 GiB`, bounded by a `max_disk_usage` percentage of the disk. — [hottier.rs](https://github.com/parseablehq/parseable/blob/bbbb79ec52ecd023e5654f9fc5cf6c2f27b85898/src/hottier.rs#L244)

### Inferences (estimates)
- On a 512 GB-RAM node, OpenObserve's default memory cache (256 GB) is already 2–3× an H200's VRAM. A GPU window is a subset of what is already hot in RAM, so its benefit is compute (aggregation/regex), not avoiding I/O.
- InfluxDB 3's "recency window plus 20% RAM" file cache is architecturally the closest analogue to a pinned GPU hot window, and the cleanest integration point.
- No community pull for GPUs has shown up; any adoption would have to be vendor-led, e.g. OpenObserve Enterprise or Datadog internal.

### Gaps
- No sourced information on Quickwit's split cache sizes, MARIS Beacon, HoraeDB or Datadog's internal caching tiers.
