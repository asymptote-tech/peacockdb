# Which product to build on peacockdb

Research report for the product decision, 2026-10-29. Spec:
[`tasks/product-research.md`](../tasks/product-research.md). All intermediate work is in
[`artifacts/product-research/`](../artifacts/product-research/). Each section below summarises an
artifact and links to it. The artifact carries the sources, with access dates (2026-10-28 or
2026-10-29). External links quoted here were accessed on those dates too. Figures marked
*(estimate)* are the researchers' estimates, not measurements. This was desk research only:
nothing was benchmarked.

## Recommendation

**Build a GPU backend for Apache Spark SQL, on peacockdb.** Its shape is not a second
spark-rapids:
- a native GPU engine behind Apache Comet's Spark-plan serializer;
- a native Parquet reader, with no JVM in the read path;
- a decoded-column cache in VRAM and pinned host RAM that lasts across queries.

The alpha is read-only SQL for one design partner's queries, with CPU fallback for everything
else. Cost: about **1.6× the current engine** *(estimate)*, the top of the budget
([option file](../artifacts/product-research/options/spark-plugin.md)).

Why this option:
- It is the only one with **strong demand that is about SQL**: 13 posters, most of them
  evaluating. One (dogman123) has a large shuffle-and-join bill, and one (Lin) serves BI over a
  9.6B-row table.
- It has the only **medium funding story**: the biggest budgets in the space, and a pitch
  ("no licence, plus a cache") that a buyer can check.
- NVIDIA has **not fixed** the two things your study of a year ago found.
  - Its reader is still the Scala/JVM reader in the 26.10 release candidate.
  - It has no cross-query cache of decoded columns. Its cache stores Snappy-compressed Parquet
    bytes on the JVM heap and re-copies and re-decodes them on every query, and no roadmap issue
    proposes more ([cache notes](../artifacts/product-research/notes/spark-gpu-plugin-research/cache.md)).
- Those two gaps are exactly where peacockdb's cache thesis applies.

The risks are plain. It goes head-on against a product NVIDIA sells. NVIDIA's top-priority
scale-up epic #16073 includes a native Parquet reader (#16087) and GPUDirect Storage (#16088)
among its items. It also sits at the top of the budget, mostly
because of Spark semantics and a DataFusion upgrade.

**Runner-up: a single-box GPU SQL engine for workstations and DGX Spark**
([option file](../artifacts/product-research/options/workstation-gpu-sql.md)).
- It has the most first-person pain of any option (about 29 posters).
- But that pain is about installing RAPIDS, and it is aimed at dataframe APIs more than at SQL.
- DuckDB's `INSTALL sirius` (DuckDB PR #28022, open) and NVIDIA's cudf-polars are shipping the
  same packaging now.
- So it works as a funnel, not as a company.
- Cost: about 0.8× on peacockdb *(estimate)*.

**What holds across all options** (details in [Cross-cutting findings](#cross-cutting-findings)):
- No option has a user with a stated budget.
- Every option rates weak on platform risk.
- NVIDIA and the incumbent engines are adding GPU execution themselves, mostly through Sirius.

So the first job, whatever is built, is the design-partner conversations listed under
[Questions only a prospective user can settle](#questions-only-a-prospective-user-can-settle).

## How the ranking rule decided

The rule, set at the checkpoint and unchanged:
1. An option whose alpha costs more than 2× the engine on both bases is out.
2. The rest are ordered by the strength of the demand evidence (from
   [`demand.md`](../artifacts/product-research/demand.md)).
3. Ties are broken by adoption friction, then the data path, then the funding story, then
   platform risk.

- **Step 1 removes nothing.** The most expensive option is the Spark plugin, at 1.6× on peacockdb
  and 1.3× on Sirius.
- **Step 2** gives these tiers:
  - strong: spark-plugin and workstation-gpu-sql;
  - medium: lean, gpu-bi-dashboards, quant-gpu and llm-corpus-sql;
  - weak: datafusion-plugin, trainer-side-sql-prep, lakehouse-iceberg, other-accelerators and
    postgres-gpu-analytics;
  - thin: fuzzy-join-gpu, grace-superchip-analytics and gpu-cpu-router.
- **Step 3, strong tier:** spark-plugin has medium adoption friction and workstation-gpu-sql
  weak, so spark-plugin ranks first.
- **Step 3, medium tier:**
  - Lean and quant-gpu have medium friction; gpu-bi-dashboards and llm-corpus-sql have weak.
  - Between lean and quant-gpu, the data path decides: lean strong, quant medium.
  - Between gpu-bi-dashboards and llm-corpus-sql, it decides again: dashboards strong,
    corpus medium.

The result: 1 spark-plugin, 2 workstation-gpu-sql, 3 lean, 4 quant-gpu, 5 gpu-bi-dashboards,
6 llm-corpus-sql, then the weak and thin tiers.

## Comparison table

Costs are total new work as a multiple of the current engine. The engine is 22,830 non-test
lines ([yardstick](../artifacts/product-research/inventory.md#yardstick)). All costs are
estimates. Ratings are strong / medium / weak. The "need" column is `demand.md`'s rating; the
other five come from each option file.

| # | Option | Need | Build cost | Data path | Adoption friction | Platform risk | Funding | peacockdb | Sirius |
|--:|---|---|---|---|---|---|---|--:|--:|
| 1 | [spark-plugin](../artifacts/product-research/options/spark-plugin.md) | strong | weak | medium | medium | weak | medium | 1.6× | 1.3× |
| 2 | [workstation-gpu-sql](../artifacts/product-research/options/workstation-gpu-sql.md) (+ dataframe-gpu) | strong | medium | medium | weak | weak | weak | 0.8× | 0.2× |
| 3 | [lean](../artifacts/product-research/options/lean.md) | medium | medium | strong | medium | weak | weak | 1.1× | 0.3× |
| 4 | [quant-gpu](../artifacts/product-research/options/quant-gpu.md) | medium | strong | medium | medium | weak | weak | 0.75× | 0.45× |
| 5 | [gpu-bi-dashboards](../artifacts/product-research/options/gpu-bi-dashboards.md) (+ realtime-vram-cache, gpu-resident-results) | medium | medium | strong | weak | weak | weak | 0.9× | 0.35× |
| 6 | [llm-corpus-sql](../artifacts/product-research/options/llm-corpus-sql.md) | medium | medium | medium | weak | weak | weak | 1.4× | 0.6× |
| 7 | [datafusion-plugin](../artifacts/product-research/options/datafusion-plugin.md) (+ datafusion-log-gpu) | weak | strong | medium | medium | weak | weak | 0.55× | 0.6× |
| 8 | [trainer-side-sql-prep](../artifacts/product-research/options/trainer-side-sql-prep.md) | weak | strong | medium | medium | weak | weak | 0.8× | 0.7× |
| 9 | [lakehouse-iceberg](../artifacts/product-research/options/lakehouse-iceberg.md) (+ gpu-scan-tier) | weak | medium | weak | medium | weak | weak | 0.6–1.1× | 0.3–0.5× |
| 10 | [other-accelerators](../artifacts/product-research/options/other-accelerators.md) | weak | strong | medium | weak | medium | weak | 0.5× | n/a |
| 11 | [postgres-gpu-analytics](../artifacts/product-research/options/postgres-gpu-analytics.md) | weak | medium | medium | weak | weak | weak | 1.1× | 0.4× |
| 12 | [fuzzy-join-gpu](../artifacts/product-research/options/fuzzy-join-gpu.md) | thin | strong | strong | medium | medium | weak | 0.8× | 0.45× |
| 13 | [grace-superchip-analytics](../artifacts/product-research/options/grace-superchip-analytics.md) | thin | strong | strong | medium | weak | weak | 0.8× | 0.3× |
| 14 | [gpu-cpu-router](../artifacts/product-research/options/gpu-cpu-router.md) | thin | strong | medium | medium | weak | weak | 0.45× | 0.25× |

Rows 7–14 are ordered by the same tie-breaks:
- in the weak tier, datafusion-plugin and trainer-side-sql-prep tie on every tie-break, then
  come lakehouse-iceberg, other-accelerators and postgres-gpu-analytics;
- in the thin tier, fuzzy-join-gpu comes before grace-superchip-analytics on platform risk, then
  gpu-cpu-router.

grace-superchip-analytics is "thin" because no first-person poster asks for it; demand.md's own
rule makes fewer than three posters thin. A "weak" platform
risk means the risk is high: NVIDIA, the host project or an incumbent already ships the same
thing.

Three of these are **features, not products**:
- **gpu-cpu-router** belongs inside whichever engine is built.
- **grace-superchip-analytics** is a deployment target. It makes the caches of lean and
  gpu-bi-dashboards bigger and cheaper, and it serves workstation-gpu-sql on DGX Spark.
- **other-accelerators** is a portability feature. An AMD-only port costs about 0.5×, because
  AMD's hipDF ports the same cuDF 25.10 that peacockdb pins.

## The base: peacockdb or Sirius

**For the recommendation, the base is peacockdb.** On the Sirius base, the Spark plugin costs
1.3× rather than 1.6×. But that route takes Spark plans through Gluten, then Substrait, then
DuckDB, which is Sirius's weakest path:
- Substrait plans that DuckDB did not produce are re-planned by DuckDB, with no CPU fallback.
- Substrait from current producers gives silent wrong answers: `LIMIT n` becomes `LIMIT 0`
  (sirius #1963, filed for StarRocks; that DataFusion 45's output is affected is an inference).
- Anti joins and window functions are rejected.
- The smallest unit Sirius runs is a whole plan or a declared fragment. It has no narrow
  optimizer call and no GPU-resident input or output
  ([sirius.md](../artifacts/product-research/sirius.md)).

Comet's serializer, by contrast, hands over a DataFusion-shaped plan. peacockdb's planner already
translates DataFusion plans.

**In general**, Sirius is much cheaper wherever the host is DuckDB:
- lean: 0.3× against 1.1×;
- gpu-bi-dashboards: 0.35× against 0.9×;
- workstation-gpu-sql: 0.2× against 0.8×.

But those cheaper products are thin layers on a fast-moving project whose staff are NVIDIA's.
The [Sirius due diligence](../artifacts/product-research/sirius.md) found:
- 1,244 commits in 12 months;
- about 92% of human commits from NVIDIA staff;
- no versioned releases;
- the old engine deleted on 2026-09-30.

A fork is out: Sirius plus its spill library cuCascade is about 7.8× peacockdb's size. The only
workable route on Sirius is a pinned dependency plus upstream contributions. That route gives up
the two things you said to weigh:
- **Knowledge of the code:** you know peacockdb end to end, while Sirius churns weekly under
  someone else's review.
- **Freedom to evolve it:** the cache, the reader and the router would sit in your code, not
  behind NVIDIA's roadmap.

On Sirius, each of these products becomes "a feature of a free NVIDIA project". That is the
weak funding story repeated across the table.

**What to borrow from Sirius regardless:**
- the pinned-table design;
- its result that moving hot tables to the GPU tier, lightly compressed, sped up its whole suite
  by about 20% against host-pinned raw data, while compression on the host tier cost about 7%
  ([notes](../artifacts/product-research/notes/vram-cache-encoding-datapath.md));
- cuCascade's spill tiers.

## The lean, part by part

Source: [options/lean.md](../artifacts/product-research/options/lean.md) and
[its notes](../artifacts/product-research/notes/lean-gpu-cache-premises/). The researcher was
given the lean as "a candidate product's premises", not as your view. It was judged by the same
template as every other option, and ranks third.

1. **"A multi-GPU, single-host RAM/VRAM cache answers ~100 GB queries in a few hundred ms."**
   - **Holds** on NVLink boxes, but only for data already resident in VRAM.
   - **Fails** on cold data: filling from storage is decode- and NIC-bound.
   - **Partly holds** on PCIe-only boxes, where shuffles between GPUs cost about 0.5 s *(estimate)*.
   - Your own SF20 numbers back the "cache first" half. The PoC spent more time loading than
     computing on 11 of 12 queries
     ([evidence-in-hand.md](../artifacts/product-research/evidence-in-hand.md)).
   - No user anywhere asked for a 100 GB / sub-second target. About six posters asked softly
     for data kept on the GPU between queries.
   - Multi-GPU is not needed for capacity: one H200 or B300 holds 100 GB once compressed. Extra
     GPUs buy latency only, and cost per query is roughly a wash against a 128-core DuckDB box
     *(estimate)*.
2. **"A plugin inside an existing database beats a standalone engine."**
   - **Mostly holds.** The 2026 momentum is inside host engines: Sirius in DuckDB, ClickHouse's
     and Doris's own GPU work, and spark-rapids. Sirius has no named production users, though.
   - The standalone ones (HeavyDB, Kinetica, SQream) stayed niche.
   - No plugin vendor has shown it can charge. BlazingSQL queried data in place and still went
     unpaid.
   - But the best host, DuckDB, already has NVIDIA's Sirius `pin_table`, which covers the
     lean's core for free.
3. **"Voltron Data failed because it built a standalone engine that needed ingestion."**
   - **Does not hold.** Theseus read Parquet in place, and it was sold embedded through partners
     such as HPE.
   - The sourced causes are a missed revenue target against a staff of about 120 (half were cut
     in 2024), and a late launch.
   - Your own spreadsheet adds one more data point. Scaled to SF200 on one GPU, Voltron's
     published numbers come to only about 1.3× one CPU box *(estimate)*
     ([evidence-in-hand.md](../artifacts/product-research/evidence-in-hand.md#tpc-h-sf200-the-numbers-that-survived)).

The lean's real contribution is the cache thesis. That thesis is now inside the recommendation:
the Spark backend's decoded-column cache is the lean's cache, applied where the largest budgets
in the space sit (though no user has stated one) and where NVIDIA has not built it.

The lean's own file also lists three negatives:
- NVIDIA's free Sirius `pin_table` already covers its core;
- on peacockdb, most of its 1.1× rebuilds what Sirius ships;
- its funding story is weak.

## Per-option summaries

Each row's file has the full template, a cost table on both bases, the strongest case against,
and sources.

- **spark-plugin**
  - **Reader:** not fixed in 26.10-rc. The reader at cudf-spark `49d3be94` has the same
    Scala/JVM structure, with no GPUDirect Storage or KvikIO. NVIDIA removed its hybrid scan and
    opened P0 scale-up epic #16073, whose items include a native reader; one PR has merged.
  - **Users:** every headline saving is published by NVIDIA. Independent attempts missed their
    targets (cudf-spark #11992) or ran slower than CPU (a GKE write-up, spark-rapids #8082).
  - **Cache:** none of decoded columns on the GPU. The cache serializer keeps compressed
    Parquet on the JVM heap and re-copies and re-decodes it on every query.
- **workstation-gpu-sql:**
  - Packaging: one manylinux wheel each for x86_64 and aarch64, with libcudf linked statically,
    about 120–200 MB *(estimate)*. `polars[gpu]` is 1.7–2.7 GB.
  - Native Windows is out of reach on libcudf.
  - The hard parts are running out-of-core on 8–32 GB of VRAM, and keeping a static libcudf
    build green.
- **lean:** see [The lean, part by part](#the-lean-part-by-part).
- **quant-gpu:**
  - The as-of join gap is closing for free. NVIDIA is porting KX's TAQ benchmark to cudf-polars
    (cudf #24634), a `cudf::asof_join` PR is open (#24180), and KX ships a GPU `aj`.
  - What would be ours is a sweep runner over resident data.
  - First partners would be small crypto market makers, prop shops and tick vendors.
- **gpu-bi-dashboards:**
  - A GPU SQL server with resident hot tables, live appends, Flight SQL, and in-process device
    hand-off.
  - It repeats the MapD/Kinetica pitch, while HeavyDB is now free and open source and Microsoft
    Fabric previews the same idea.
  - Your storage question is answered below.
- **llm-corpus-sql:**
  - The pain is real: NeMo Curator runs out of memory, and Ray workflows break, at 6–24 TB.
  - But it sits inside NVIDIA's free Curator, nobody asks for SQL, and dedup is a small GPU
    line: about 16 H100-hours per 8 TB, against about 6,000 for one classifier pass.
- **datafusion-plugin:**
  - The seam is the right architecture and the cheapest alpha, but two plugins already ship:
    Algeon, and libcudf-rs from Datadog engineers. Both are on DataFusion 55.
  - peacockdb is on DataFusion 45, and every target host pins its own version, so the plugin
    needs one build per host.
  - In log stores, full-text search is already indexed, so the GPU's work is aggregations and
    regex over a resident window.
- **trainer-side-sql-prep:**
  - The cost at training time is decoding and conversion to tensors, not SQL. Joins run
    upstream.
  - Training GPUs have no spare HBM, and cudf-polars already has an experimental zero-copy
    export to torch.
- **grace-superchip-analytics:**
  - On GH200 the GPU reads Grace memory at 350–418 GB/s. HBM gives 3.4 TB/s, and pinned PCIe on
    H200 gives 47 GB/s.
  - Sirius runs SF1000 on GB300 in 18–21 s from Grace memory, against about 7 s from HBM.
  - A GH200 rents for about $2.29/h. Nobody asks for SQL on Grace.
- **lakehouse-iceberg:**
  - Viable only as a GPU Iceberg engine with three cache tiers: VRAM, pinned RAM and NVMe.
  - Cold S3 reads are one to two orders of magnitude below GPU decode speed.
  - Today's decode, at 7–9 GB/s, is slower than a 100 Gbps NIC. Fix #232 and Snappy decoding
    first.
  - Sirius merged GPU Iceberg scans with position deletes and v3 deletion vectors on 2026-09-15.
    Equality deletes fall back to DuckDB.
- **other-accelerators:**
  - peacockdb uses about 75 libcudf entry points, 5 custom kernels and about 30 CUDA runtime
    calls.
  - An AMD port is cheap. There are 12 requests, but none comes with a workload.
  - Cerebras, Groq and TPU suit hash joins and strings poorly.
- **postgres-gpu-analytics:**
  - PG-Strom has had 9 years with no named customers, and its next release drops its GPU cache
    as "rarely used".
  - Managed Postgres allows no CUDA extension and offers no GPU instances.
  - The cheapest alpha (Sirius in pg_lake's DuckDB sidecar) is just as easy for anyone else.
- **fuzzy-join-gpu:**
  - The work is GPU-shaped: about 8×10⁷ candidate pairs/s on a desk GPU, against about 2×10⁷ on
    192 vCPUs.
  - But Splink on DuckDB already links 1B records in about 9 minutes. The pain is blocking
    explosion and output spill, not scoring speed.
- **gpu-cpu-router:**
  - Hot versus cold data swings ClickBench 20–40×, so the clear cases are predictable.
  - peacockdb's cost model counts bytes but times nothing, so routing would need calibration
    runs.
  - On Sirius, the router is issue #990.

**Your VRAM-cache question**, from gpu-bi-dashboards and
[its notes](../artifacts/product-research/notes/vram-cache-encoding-datapath.md): compressed
segments, or uncompressed data before RLE and dictionary decoding?
- **Capacity:** both forms hold about 2.5–3× more data.
- **Compressed segments:** for an 80 GB working set, general codecs (snappy, zstd) or Parquet
  pages cost about 0.5–0.7 s per query to decode. Through today's `read_parquet`, it is about
  10 s.
- **Encoded columns:** bit-packed or dictionary columns decode in about 50–140 ms. They also
  allow filtering during decode; dictionary-coded strings filter 2.1–2.6× faster that way.
- **The catch, in cuDF:**
  - Dictionary columns work in groupby, join and gather.
  - They fail in binary operations, so filters need a decode or a compare on the keys.
  - cuDF has no RLE or bit-packed column type.
  - cuDF's newer Parquet reader can output strings as dictionaries, and its `hybrid_scan` reads
    filter columns first.
- **Answer:** store lightly encoded columns, with dictionary and bit-packing, in VRAM. Use
  general compression only in the host-RAM tier.

## Options cut at the screen or the checkpoint

Cut by you at the checkpoint (named options):
- **clickhouse:** ClickHouse is building GPU execution itself, on Sirius (RFCs #116807 and
  #124455, plus 11+ PRs).
- **realtime-other:** Doris has merged a Sirius backend; StarRocks has an experimental one.
- **distributed-pluggable:** the engines (Databend and others) are adding GPU through Sirius or
  cuDF themselves.
- **streaming-flink:** no community evidence at all. Flink's GPU work is model inference.
- **vector-sql:** thin evidence, with no named poster.

Discovered options that passed the screen but were not kept, each with two named posters or
fewer: flow-hunt-gpu, single-cell-gpu, in-db-gpu-inference, edge-jetson, geo-join-gpu,
gpu-sql-migration-qualifier and ran-sim-datalake.

Failed at the screen:
- for no sign of need: storage-oem-engine, gov-airgapped, ab-metrics-gpu, idle-gpu-batch,
  telco-xdr-gpu, gpu-data-editor, zk-proof-sql and agent-query-load;
- for not being GPU-centred: robot-log-gpu, trace-analytics-gpu and flight-sql-gpu-endpoint.

Reasons and links: [discovery.md](../artifacts/product-research/discovery.md).

## Sirius and NVIDIA

**Sirius** ([sirius.md](../artifacts/product-research/sirius.md)), at `b9e05ffc`:
- It has no optimizer of its own. It uses DuckDB's for join order and cardinality, with no GPU
  cost model; peacockdb's PR #165 prototype is further along here.
- It caches nothing across queries automatically. Tables are pinned by hand with `pin_table` and
  never evicted.
- Spilling (GPU, then pinned host, then disk, through cuCascade) exists, but it is young and
  unmeasured.
- Substrait comes in through a library call and is re-planned by DuckDB.
- Other frontends (Doris, StarRocks) are experimental and translate to Substrait themselves.

**NVIDIA** ([nvidia.md](../artifacts/product-research/nvidia.md)) sells no database. But it gives
away GPU SQL engines its own staff build:
- HeavyDB, bought in 2025 and open-sourced in October 2026;
- Sirius;
- GPU Presto, with IBM;
- GQE, a DataFusion → Substrait → libcudf proof of concept.

It also sells the Spark plugin, with support, through AI Enterprise.

**"NVIDIA denies competing" has no public source.** "Backs Sirius" understates it: a GTC 2026
session reportedly called Sirius "NVIDIA's GPU-native analytics engine", per one attendee's notes.
The risk is less a rival paid product than the engine becoming free. The trigger to watch is paid
NVIDIA support for Sirius or HeavyDB.

## User demand

Sources: [demand.md](../artifacts/product-research/demand.md) and four community digests in
[sources/](../artifacts/product-research/sources/):
- issue trackers (100 relevant threads);
- Reddit (110 hits, partial: the PullPush archive blocks agents);
- Hacker News and Stack Exchange (133 hits);
- 30 forums (61 hits).

Discord and Slack were unreachable: both public indexes return bot checkpoints.

- **Who accelerates already:**
  - mostly single-node pandas/Polars users on cuDF;
  - a few Spark + RAPIDS shops, such as a nightly 380M-row batch on two DGX Spark units;
  - HeavyDB users with custom patches.
- **What people ask for:**
  - easy installs (Windows, smaller images, newer Python);
  - data or results kept on the GPU between queries;
  - cheaper Spark joins and shuffles;
  - AMD and Apple support.
- **What goes wrong:**
  - the GPU loses to a tuned CPU engine on IO, transfer and silent CPU fallback;
  - install pain;
  - missed cost targets.
- **The case against urgent demand:**
  - About 60 named first-person posters in three years, none with a budget and a deadline.
  - DuckDB beats GPU paths at the sizes people have.
  - ClickHouse's CTO calls GPU wins "a small subset of production workloads in a narrow
    combination of conditions".
  - The free supply from NVIDIA is large.

## Follow-up experiments

The measurements that would most change this ranking, collected from the option files:

1. **Re-run your spark-rapids reader study on 26.10** (SF200, one H100, your patch). Does the 10×
   still hold, and how does it split between footer, copy and decode? This tests the
   recommendation's main claim.
2. **Comet on CPU, spark-rapids, and peacockdb behind Comet,** on one ETL job of the kind in
   cudf-spark #11992, costed per run including the licence. Is there a GPU win left against
   Comet?
3. **Warm cache inside Spark:** PCBS, against a re-read, against a pinned or VRAM copy, on TPC-H
   Q1/Q3 on one H100. This bounds what the cache saves.
4. **Desk-GPU head-to-head:** peacockdb, DuckDB + Sirius and DuckDB on CPU, on an RTX 4090/5090
   and on GB10, at TPC-H SF10/SF50, cold and warm. No one has published this; it decides the
   runner-up.
5. **Sirius `pin_table` on 8 GPUs at SF100.** Does it shard? What are the hot times and the fill
   time? This decides whether the lean adds anything over Sirius.
6. **peacockdb's #232 root cause,** whether the slow `read_parquet` is cudf #24000's over-read.
   Profile the chunked and hybrid readers on GB10 and H200. Every option's cold path depends on
   this.
7. **Random hash-probe rate into Grace memory on GH200** (about $2.29/h on Lambda). This settles
   whether Grace memory can hold a join's build side.

Each option file has its own list.

## Questions only a prospective user can settle

The best first conversations ([demand.md](../artifacts/product-research/demand.md#first-conversations)):
- **For the recommendation:**
  - **Maksim Konstantinov** ran an ETL cost benchmark on AWS that missed its 50% target
    (cudf-spark #11992).
  - **dogman123** runs EMR joins of 150B+ rows, and finds Glue too costly
    ([HN](https://news.ycombinator.com/item?id=43966388)).
  - **Yu-Ting Lin** serves Superset over a 9.6B-row Delta table through Spark Thrift + RAPIDS
    (cudf-spark #11064).
  - **Andy Gong** wants inputs cached in GPU memory across queries, which spark-rapids does not
    support, as NVIDIA confirmed (cudf-spark #13435).
- **For the runner-up and the cache:**
  - **Louie Meyerov** (Graphistry) wants results kept on the GPU (cudf #23039).
  - **Carl Boettiger** (UC Berkeley) is adding a GPU backend to a DuckDB-based data server.
  - **u/fmoralesh** has 1.2 TB of logs on a V100 and 11 TB of S3 Parquet.

What only they can tell you:
1. Would you run a non-NVIDIA GPU backend in production Spark, and what would make you trust its
   answers?
2. What does one target job cost today, and what saving would make you switch? Is it 30%, 50%,
   or more?
3. Do your queries repeat over the same data within hours? This decides whether the cache pays.
4. Which Spark version, which platform (EMR, Databricks, self-run), and which GPUs can you use?
   Databricks limits GPUs.
5. Would you pay for this, or wait for NVIDIA's native reader (#16073)?
