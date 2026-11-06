# Query batching — fused vs unfused on the H100, and DuckDB on 128 cores

A batch is the first 3, 10 or 20 queries of a dataset's list, or all of them, fused into one
plan by query-fuser: each file loaded once for every scan of it, then run under the cap rule. The
unfused side is the same queries run one at a time (sf40-baseline). DuckDB is the same queries
on 128 cores.

## Reading the comparison honestly

- Every peacockdb total is **device-side**: `load + execute`, the summed node times of the run's
  tree (`device_us`). Load is included on both GPU sides. DuckDB's time is the query's wall time
  over its warm native storage, so the `DuckDB / fused` ratio compares two different clocks.
- Each peacockdb run reads its files from the pinned host cache, filled before any run; the
  prefill's disk read is in no total. DuckDB reads its buffer manager over a warm page cache.
- The accountant's peak is a lower bound on device memory.
- Both hints are estimates from models stated under their table, not measurements.
- The load split comes from profiled runs under Nsight, not the timed runs; compare its rows
  with each other, not with the timed table.

Every number between the markers below is written by `scripts/calibration/batching_report.py`
from the committed results; rerun it rather than editing them.

<!-- batching_report.py: generated below, do not edit -->
## Provenance

### Provenance: the H100

| key | values (sections) |
|---|---|
| device | NVIDIA H100 80GB HBM3 (134) |
| driver | 580.173.02 (134) |
| cuda | 12.2 (134) |
| cudf | 25.02.02 (134) |
| host | computeinstance-e00e6yena4nzjgaym0 (134) |
| pinned host cache | on cap=147.4GiB source=default (134); resident ≤ 39.5GiB; largest prefill 42442683465 B in 460368515 µs (tpch first3 fused); cache not on: none; tree sections not prefilled: none; fell back, evicted or reloaded: none |

### Provenance: DuckDB

| key | values (datasets) |
|---|---|
| duckdb | 1.5.4 (2) |
| threads | 128 (2) |
| host | computeinstance-e01t2mm4265y8x88ey (2) |
| cpu | AMD_EPYC-Genoa_Processor (2) |
| cores | 128 (2) |

### Provenance: protocol

Each reported time is the 2nd-minimum of its runs. Runs: peacockdb unfused 10 runs (126); fused, per batch: tpch first3 10, first10 10, first20 10, all 3; tpcds first3 10, first10 10, first20 10, all 3; DuckDB 6 runs (126).

Before any run, each query process (each fused batch, its union) prefilled its files once into the pinned host cache, then ran 1 warm-up: every peacockdb run reads files resident in pinned host memory, DuckDB's its buffer manager over a warm page cache. The prefill's disk read is reported apart (the cache row above).

### Provenance: skipped and wrong at sf40

- tpch anti_join — skipped at sf40: tpch/anti-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8 but found LargeUtf8 at column index 8 (declared vs exported: 8 o_comment: Utf8 vs LargeUtf8)
- tpcds q17 — skipped at sf40: tpcds/q17 at tp1-single: this mode refuses it: invalid plan: what the accumulators hold and cannot be holding less of (2593119555 bytes) is already the whole budget (2147483648), so no batch size makes this plan run
- tpcds q18 — wrong answer: #306
- tpcds q25 — skipped at sf40: tpcds/q25 at tp1-single: this mode refuses it: invalid plan: what the accumulators hold and cannot be holding less of (3146023251 bytes) is already the whole budget (2147483648), so no batch size makes this plan run
- tpcds q29 — skipped at sf40: tpcds/q29 at tp1-single: this mode refuses it: invalid plan: what the accumulators hold and cannot be holding less of (2593120563 bytes) is already the whole budget (2147483648), so no batch size makes this plan run
- tpcds q73 — wrong answer: #306

## TPC-H sf40

### TPC-H sf40: fused against unfused and DuckDB

| batch | queries | fused load (ms) | fused execute (ms) | unfused load (ms) | unfused execute (ms) | DuckDB warm (ms) | unfused / fused | DuckDB / fused |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| first3 | 3 | 585.5 | 335.3 | 666.7 | 334.1 | 1473.0 | 1.09× | 1.60× |
| first10 | 10 | 2105.4 | 802.3 | 2709.1 | 782.1 | 6307.0 | 1.20× | 2.17× |
| first20 | 20 | 3418.2 | 1481.6 | 4917.4 | 1418.9 | 12147.0 | 1.29× | 2.48× |
| all | 22 | 3702.9 | 1820.9 | 5706.9 | 1760.2 | 13401.0 | 1.35× | 2.43× |

Totals are `load + execute` per side; ratios above 1 mean fused is faster. `(n/N)`: the sum covers n of the batch's N queries.

### TPC-H sf40: the two hints

| batch | cap | estimate / budget (GiB) | accountant's peak (GiB) | merge saves (ms, estimate) | entries | pipelining saves (ms, estimate) | pipelined estimate (ms) | folded scans (a query's own) |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| first3 | 3 | 40.8 / 62.1 | 39.5 | 0.0 | 0 | 295.0 | 625.8 | 4 (3) |
| first10 | 1 | 84.1 / 62.1 (over budget) | 43.1 | 0.0 | 0 | 677.3 | 2230.4 | 15 (2) |
| first20 | 1 | 80.0 / 62.1 (over budget) | 51.8 | 0.0 | 1 | 1314.1 | 3585.6 | 35 (5) |
| all | 1 | 85.0 / 62.1 (over budget) | 53.5 | 0.0 | 1 | 1655.5 | 3868.4 | 41 (7) |

- Merge, an estimate: identical subtrees above a shared load, with members from two or more queries, run once and the dearest copy stays: Σ t − max t over each group, t the queries' own unfused node times (sf40-baseline).
- Pipelining, an estimate: one loader and one executor, each sequential, side by side; a root starts once its last source is loaded and the executor is free; cheapest root's loads first; the saving is serial − makespan, the pipelined estimate the fused measured total less it.
- The accountant's peak is a lower bound on device memory; the estimate and budget are the cap rule's (query-fuser).
- Folded scans: per shared load, its scans less one, from `shared=`; in brackets those that are a query's own repeat of a scan it already makes, which a batch of one would fold too. A count of loads, not of time.

### TPC-H sf40: load against decode

| batch | side | Σ load | host read | H2D copy | decompress+decode | other device | unattributed |
|---|---|--:|--:|--:|--:|--:|--:|
| first3 | unfused | 745.1 | 0.0 | 95.2 | 496.1 | 17.7 | 147.8 |
| first3 | fused | 621.2 | 0.0 | 89.4 | 444.0 | 14.7 | 84.2 |
| first10 | unfused | 3015.7 | 0.0 | 539.9 | 1877.1 | 54.2 | 580.9 |
| first10 | fused | 2286.5 | 0.0 | 457.6 | 1494.0 | 44.1 | 324.5 |
| first20 | unfused | 5438.7 | 0.0 | 902.1 | 3475.4 | 89.6 | 1045.2 |
| first20 | fused | 3643.9 | 0.0 | 687.6 | 2477.9 | 61.5 | 473.4 |
| all | unfused | 6334.0 | 0.0 | 1105.4 | 3976.4 | 96.5 | 1235.7 |
| all | fused | 3874.5 | 0.0 | 750.2 | 2654.8 | 63.5 | 469.2 |

ms, from one profiled run per query process (per fused batch) under Nsight: each number is a per-execution median, summed over the batch's loads, and is not the timed tables' time. Unfused `host_read_source`: none. With `kvikio` the host read holds kvikio's bounce copies, which H2D counts too, so the two are not added.

## TPC-DS sf40

### TPC-DS sf40: fused against unfused and DuckDB

| batch | queries | fused load (ms) | fused execute (ms) | unfused load (ms) | unfused execute (ms) | DuckDB warm (ms) | unfused / fused | DuckDB / fused |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| first3 | 3 | 280.8 | 304.8 | 406.9 | 308.0 | 586.0 | 1.22× | 1.00× |
| first10 | 10 | 1488.8 | 2758.9 | 3458.0 | 2757.5 | 3355.0 | 1.46× | 0.79× |
| first20 | 20 | 5296.6 | 5757.5 | 8167.8 | 5748.0 | 8084.0 | 1.26× | 0.73× |
| all | 79 | 17196.0 | 14529.9 | 24066.1 | 14454.6 | 25602.0 | 1.21× | 0.81× |

Totals are `load + execute` per side; ratios above 1 mean fused is faster. `(n/N)`: the sum covers n of the batch's N queries.
- q18: counted, though its answer is wrong (#306); in first20, all.
- q73: counted, though its answer is wrong (#306); in all.

### TPC-DS sf40: the two hints

| batch | cap | estimate / budget (GiB) | accountant's peak (GiB) | merge saves (ms, estimate) | entries | pipelining saves (ms, estimate) | pipelined estimate (ms) | folded scans (a query's own) |
|---|---|--:|--:|--:|--:|--:|--:|--:|
| first3 | 3 | 9.9 / 62.1 | 3.5 | 0.0 | 0 | 168.3 | 417.3 | 8 (6) |
| first10 | 2 | 61.6 / 62.1 | 38.0 | 0.3 | 1 | 1373.2 | 2874.5 | 51 (36) |
| first20 | 2 | 61.3 / 62.1 | 28.7 | 1.5 | 3 | 4523.5 | 6530.5 | 97 (64) |
| all | 1 | 53.3 / 62.1 | 40.3 | 92.8 | 31 | 11109.3 | 20616.6 | 329 (175) |

- Merge, an estimate: identical subtrees above a shared load, with members from two or more queries, run once and the dearest copy stays: Σ t − max t over each group, t the queries' own unfused node times (sf40-baseline).
- Pipelining, an estimate: one loader and one executor, each sequential, side by side; a root starts once its last source is loaded and the executor is free; cheapest root's loads first; the saving is serial − makespan, the pipelined estimate the fused measured total less it.
- The accountant's peak is a lower bound on device memory; the estimate and budget are the cap rule's (query-fuser).
- Folded scans: per shared load, its scans less one, from `shared=`; in brackets those that are a query's own repeat of a scan it already makes, which a batch of one would fold too. A count of loads, not of time.

### TPC-DS sf40: load against decode

| batch | side | Σ load | host read | H2D copy | decompress+decode | other device | unattributed |
|---|---|--:|--:|--:|--:|--:|--:|
| first3 | unfused | 468.8 | 0.0 | 46.4 | 312.2 | 5.4 | 123.5 |
| first3 | fused | 303.4 | 0.0 | 36.2 | 214.1 | 3.8 | 62.5 |
| first10 | unfused | 4027.8 | 0.0 | 506.1 | 2583.5 | 56.4 | 1079.4 |
| first10 | fused | 1698.5 | 0.0 | 293.4 | 1003.2 | 31.8 | 436.7 |
| first20 | unfused | 9170.2 | 0.0 | 1048.1 | 6283.1 | 114.6 | 2092.1 |
| first20 | fused | 6068.1 | 0.0 | 872.6 | 3686.7 | 91.9 | 1641.8 |
| all | unfused | 27330.7 | 0.0 | 3158.6 | 18437.2 | 334.1 | 6217.2 |
| all | fused | 19861.1 | 0.0 | 2808.2 | 12140.7 | 273.0 | 5211.3 |

ms, from one profiled run per query process (per fused batch) under Nsight: each number is a per-execution median, summed over the batch's loads, and is not the timed tables' time. Unfused `host_read_source`: none. With `kvikio` the host read holds kvikio's bounce copies, which H2D counts too, so the two are not added.
<!-- batching_report.py: generated above -->

## Reading

- **The saving is load.** In every batch fused execute is within 5 % of the unfused sum (the
  fused-against-unfused tables), and fused load is lower: TPC-H all 3.7 s against 5.7 s,
  TPC-DS all 17.2 s against 24.1 s.
- **Where it stops growing.** `unfused / fused` rises through TPC-H's batches, 1.09× to 1.35×
  at all, though by less each step. TPC-DS's peaks at first10, 1.46×, and falls to 1.21× at all.
- **Part of it is not batching.** In the hints tables' folded scans, 175 of TPC-DS all's 329
  are a query repeating its own scan (first3: 6 of 8), against 7 of 41 in TPC-H all. A batch
  of one would fold those too. The tables do not split the load saving's milliseconds between
  the two kinds.
- **Decodes more than copies.** In the load-split tables (profiled runs) host read is 0 on
  every row. From unfused to fused, decompress+decode falls 1.3 s in TPC-H all and 6.3 s in
  TPC-DS all, the H2D copy 0.36 s and 0.35 s.
- **Most batches ran at cap 1 or 2**: 6 of 8 (the hints tables). TPC-H first10, first20 and
  all ran at cap 1 over budget, an estimate of 80-85 GiB against 62.1 GiB. Only the two first3
  batches ran at cap 3. No batch ran out of memory.
- **What to build next.** The pipelining estimate is larger than the merge estimate in every
  batch: 11.1 s against 92.8 ms in TPC-DS all, 1.7 s against 0.0 ms in TPC-H all. Both are
  estimates from their models; merging identical subtrees across queries has little to save.
- **Against DuckDB** (different clocks, see above): fused TPC-H is 1.60× to 2.48× DuckDB's
  warm sum. Fused TPC-DS is level at first3 (1.00×) and slower from first10, 0.73× to 0.81×.
