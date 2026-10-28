# Evidence in hand

What the human measured before this research began. The sources are the spark-rapids commits in
`~/repo/spark-rapids` and two spreadsheets, exported here on 2026-10-28 to [`sheets/`](sheets/).
The human ran every measurement on this page. None was re-run.

## The spark-rapids study (2025-08 to 2025-10)

- **Setup**, from [`spark-rapids-versions-sheet1.csv`](sheets/spark-rapids-versions-sheet1.csv):
  one H100 on Nebius, Spark 3.5.6 standalone, pyspark 3.5.6, OpenJDK 17, Scala 2.12.18, and
  spark-rapids 25.06.0. It was also built from source with `-Dbuildver=356`, on a tree whose base
  is the 25.08 release changelog commit `55581c19d`. Log4j was switched to millisecond
  timestamps (`SSS`), and the timing came from those timestamps.
- **What was instrumented**, from [`spark-rapids-instrumentation.patch`](spark-rapids-instrumentation.patch),
  commit `99a1d01b7` of 2025-10-15. It adds `logInfo` lines along the GPU Parquet read path:
  - `GpuMultiFileReader.scala`: thread-pool creation, `readPartFiles`, and per-file
    read start and finish with the bytes read. Some `logDebug` lines are promoted to `logInfo`.
  - `GpuParquetScan.scala`: reader factory construction, then block copy into host memory
    (`Starting to copy blocks data` / `Copied … bytes`), coalescing of remote reads, and read to
    host memory per file. Then the GPU decode (`readChunk`), schema evolution and
    `rebaseDateTime`, and the batch size on the GPU.
  - `GpuFileScanRDD.scala`: a stack trace per file read, to see who drives the reader.
- **Result** (the human's account; the logs are lost): along this path, time spent in the
  Scala/JVM reader code was about **10× the time spent executing the plan's nodes on the GPU**.
- **The one surviving number:** TPC-H SF200 Q1 on spark-rapids took **22.57 s**
  ([`tpch-results-sf200.csv`](sheets/tpch-results-sf200.csv)). Below, the same query on other
  engines.

## TPC-H SF200: the numbers that survived

Source: [`tpch-results-sf200.csv`](sheets/tpch-results-sf200.csv). Times are in seconds, wall
clock.

| | Q1 | Q3 |
|---|--:|--:|
| spark-rapids, 1×H100 | 22.57 | — |
| PoC (hand-written plan on cuDF, 1×H100): compute | 1.79 | 2.15 |
| PoC: data loading | 1.00 | 1.76 |
| DuckDB, 64 cores, 128 GB | 2.22 | 2.39 |
| DuckDB, 32 cores, 64 GB | 3.70 | 3.37 |
| DuckDB, 16 cores, 32 GB | 7.62 | 5.73 |

- DuckDB was measured on 13 queries (1, 3, 4, 5, 6, 10, 12, 13, 14, 16, 17, 19, 20), at all three
  sizes. Its CPU utilisation stays well below the core count. For example, Q1 on 64 cores used
  about 46 of them.
- **Voltron Data, from published numbers:** SF1000 on 2×A100, set against DuckDB at SF1000 on
  96 cores with 378 GB. On those 13 queries, DuckDB's time divided by Voltron's ranges from
  0.56 (Voltron slower, Q1) to 2.5 (Q6), with a mean of **1.55**. The human then scaled Voltron
  to SF200 on one GPU and compared it with DuckDB at SF200. GPU time over CPU time averages
  **0.75** against 32 cores and **0.77** against 64 cores. In other words, under the human's
  scaling, Voltron Data's GPU engine was only about 1.3× faster than one CPU box (estimate,
  scaled).

## TPC-H SF20: where the time goes on one H100

Sources: [`tpch-results-clickhouse-sf-20-64vcpu-128gb.csv`](sheets/tpch-results-clickhouse-sf-20-64vcpu-128gb.csv)
and [`tpch-results-sf-20-details.csv`](sheets/tpch-results-sf-20-details.csv). ClickHouse and
DuckDB had their data imported into their native formats, with the files paged into memory by
`vmtouch`. The PoC read Parquet.

| Query | ClickHouse, 64 vCPU | DuckDB, 64 vCPU | PoC compute, H100 | PoC load (of which copy, decode) |
|---|--:|--:|--:|--:|
| 1 | 0.382 | 0.32 | 0.21 | 0.132 (0.072, 0.038) |
| 5 | 1.70 | 0.55 | 0.174 | 0.239 (0.172, 0.035) |
| 10 | 0.55 | 0.80 | 0.085 | 0.283 (0.169, 0.072) |
| 20 | 0.17 | 0.45 | 0.048 | 0.248 (0.140, 0.064) |

- **Loading costs more than computing.** On 11 of the 12 queries with a load time, the PoC
  spent more time loading than computing; Q1 is the exception. Copying is the largest named
  part of the load. Copy plus decode does not add up to the load time, and the sheet does not
  name the rest. This
  is the human's evidence for the cache thesis: unless the data already sits in GPU or pinned
  host memory, the GPU's speed advantage goes into moving the data.
- **Load plus compute against ClickHouse:** the human's "Improvement" column averages **1.72×**.
  It ranges from 0.39× (Q14, where ClickHouse wins) to 4.7× (Q13).
- The SF20 tab also has columns for 1×H100 and 2×H200 at DataCrunch, "uncompressed from
  string". Their column labels did not survive, so they are not interpreted here.
- The tab's "Spark time" column (1.1–4.3 s) does not say whether it is CPU Spark or
  spark-rapids. Treat it as unlabelled.
- ClickHouse returned wrong answers on Q13 (it counted NULLs in `COUNT(column)`) and an empty
  result on Q20. Both are noted in the sheet.

## Hardware cost (at the time of measurement)

From [`tpch-results-hardware-cost.csv`](sheets/tpch-results-hardware-cost.csv), per hour:

- Yandex Cloud, 64× Intel Ice Lake, 128 GB: 112 RUB ≈ $1.35.
- DataCrunch, 64× AMD EPYC, 256 GB: $0.45, a fixed price; the dynamic price is usually lower.

## What is lost

- The spark-rapids logs and per-stage timings behind the "about 10×" finding.
- spark-rapids times for every query except SF200 Q1.
- Which Spark build the SF20 "Spark time" column used.

Re-measuring any of these is a follow-up experiment. It is out of this desk-only task.
