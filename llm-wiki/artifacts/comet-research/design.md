# Alpha design for a GPU Comet backend, and the conditions for a go

## Summary
- **Verdict: conditional go in all four headline cells; three of the four mids are below 2×.** Batch "all", the faster CPU baseline, eight 16-core nodes, the NIC tier (cost.md): TPC-H 1.88 at sf40 and 1.38 at sf200; TPC-DS 2.14 at sf40 and 1.58 at 1 TB. Every band straddles 2× (0.06 to 37), so the spec's rule gives a conditional go. Only TPC-DS sf40's mid is above 2.
- **The one-pass floor is a no-go at mid values:** the median per-query ratio is 0.81 (TPC-H) and 0.83 (TPC-DS) at sf40 on 8 × 16. One query reading S3 alone leaves a GPU node slower per dollar than the CPU cluster.
- **The gain is batching's, not the GPU's.** The device factor is below 1 in every headline cell (0.42-0.88); shared scans (sharing factor 2.4-3.7) carry it to about 2. Against a CPU baseline that shares scans the same way, the mids fall to 1.76, 1.23, 1.85 and 1.02.
- **Settling measurements** (the sensitivity tables' top rows): the H100 node's shuffle throughput on its 16 cores (TPC-H, both sizes; mid reaches 2 at 4.6 GB/s for sf40 and 9.6 GB/s for sf200); Spark's per-query overhead on a GPU node (TPC-DS sf40); and the never-run TPC-DS concurrent S3 time on the CPU (TPC-DS 1 TB; mid reaches 2 if it is at least 166 s).
- **The design** puts the GPU inside Comet's native crate, per task, on a protobuf-to-IR translator; it shares scans passively through an executor-level column cache; and it keeps shuffle, I/O and Spark-exact edge cases on the CPU. The hard part is not the GPU kernels. It is Spark-exact semantics, plus the 16-core CPU path every byte of shuffle crosses.

## 1. What would turn the conditional go into a go
Each line is one input moved alone, the others at their values (`cost.md`; break-even by bisection on `cost_model.evaluate`).

| Cell | Lever | Value now | Break-even for mid = 2 | How to measure it |
|---|---|---|---|---|
| TPC-H sf40 | `shuffle_rate_gpu_node` | 4 GB/s (modelled) | 4.6 GB/s | Comet on 16 cores of the CPU VM, TPC-H sf40 local: shuffle write + read time per GB from the event log. No GPU needed |
| TPC-H sf200 | `shuffle_rate_gpu_node` | 4 | 9.6 GB/s | the same; above about 8 GB/s it needs lz4 on the GPU (nvCOMP) |
| TPC-H sf40 | `scheduler_share` | 0.25 | 0.13 | per-stage task-launch gaps on a 16-core executor, from the same run |
| TPC-H sf40 | `rate_h100_s3` | 1.25 GB/s (assumed) | 1.53 GB/s | `s3_throughput.py` on the H100 VM itself (not used in this task) |
| TPC-DS sf40 | `scheduler_share` | 0.25 | 0.32 (the mid is 2.14 at 0.25) | as above |
| TPC-DS 1 TB | `cpu128_s3_concurrent_all_s.tpcds` | 134.9 s (modelled) | 165.9 s | Comet TPC-DS sf40 concurrent "all" from S3 on the CPU VM: a few minutes; most of the 2 h cap is unused |
| TPC-DS 1 TB | `rate_h100_s3` | 1.25 | 2.72 GB/s | as above |

- **Decode and GPU compute do not appear.** In the batched cells the wire binds before either: at 1.25 GB/s × 0.89 the NIC is below the 4.1 GB/s decode rate, and the speedup moves TPC-H sf40 only between 1.72 and 1.88. A faster GPU or reader buys nothing until the wire and the CPU side do.
- **Hardware and price.** GH200 at $2.29/h is the one tier whose mid clears 2 in every headline cell (3.7-4.1 at sf40, 3.2-3.8 at the large sizes, batch "all"), and it wins on price, not on its 375 GB/s link. Every tier that only feeds the GPU faster (100-400 Gbit/s NICs, GDS, Nebius's shared filesystem) stays at 2.1-2.3 at sf40 (batch "all"), because the 16-core CPU side then sets the time. Faster and dearer hosts (p5.4xlarge, B200, Azure ND, GB200) fall below the NIC tier at their prices.
- **Workload.** The gain grows with the batch's cut: TPC-DS "all" (cut 8.1) is above TPC-DS batch 10 (cut 1.9). Production sharing is nearer 1.6× (SCOPE, batching.md), about TPC batch 10, where TPC-H is 2.17 and TPC-DS 1.75 at mid. A design partner's real overlap decides which.
- **The CPU can share too.** If Comet gets the same shared scans (its own cache, or the cache below fed to its CPU operators), the ratio falls by 5-31% at sf40 and up to 63% at 1 TB (cost.md, "CPU with shared scans"). So the GPU case rests on the device factor, which the levers above have to lift past 1.

## 2. The alpha, by component

### Seam
- Fork `native/core` of Comet 1.2.0 (option A in comet-internals.md). In `PhysicalPlanner::create_plan` (`planner.rs:1626`), a block whose GPU subtree (split.md) is non-empty becomes a `GpuStageExec: ExecutionPlan` over the CPU remainder. It receives the prost-decoded `Operator` with the task's `file_partition` injected, plus the task's input slots, and returns a DataFusion 55 `SendableRecordBatchStream`. Comet's `prepare_output` hands it to the JVM over the C Data interface unchanged.
- One executor per GPU node: 16 cores, `spark.executor.resource.gpu.amount=1`. Up to 16 tasks share the device through a semaphore, as in spark-rapids.
- Pinned to one Comet release. The protobuf is not a stable API, so a release bump re-runs `plan_census.py` and the translator's tests.

### IR and translation
- A protobuf-to-IR translator writes peacockdb's plan nodes directly from `spark_operator::Operator`. No DataFusion 55 plan is built for the GPU part, and DataFusion 45 is not used on this path (translatability.md, "What the DataFusion gap costs").
- New sources: Parquet from `object_store` with the task's file list; Arrow batches from the JVM (`Scan`: broadcasts, Spark-operator output); Comet shuffle blocks decoded on the CPU (`ShuffleScan`).
- Adaptations: aggregate state in Spark's buffer layout; sort-merge join as hash join; Existence as `LeftMark`; `Expand` as grouping sets; Spark-exact arithmetic, cast, `check_overflow` and decimal reinterpretation (`make_decimal`, `unscaled_value`). The census counts 2,374 adaptable operators and 1,802 adaptable expressions, against 0 and 20 missing.

### Split rule
- split.md: the GPU takes the block's largest leaf-up subtree in which everything is supported or adaptable, and the CPU runs the rest from its Arrow output. Applied to the dumped plans, 96 of 104 queries run every native operator on the GPU.
- Shuffle stays Comet's: the GPU computes Spark's murmur3 partition ids (peacockdb's `spark_hash_partition.cu`, already bit-exact against Comet's) and scatters; the CPU encodes, compresses and writes. Moving lz4 to the GPU is the first optimisation the model asks for (section 1).

### Data path and cache
- Object-store reads on CPU threads into pinned host buffers (today's reader uses pageable buffers, #149), then GPU decode with the chunked reader (#232; 4-6× faster than `read_parquet` on GB10, inventory.md item 7, benchmark-minimal.md).
- **An executor-level column cache, pinned host RAM first, VRAM second,** keyed by (file, row group, column), with soft affinity so a window's tasks for one file land on the same node. This is batching option E (batching.md): no new API. Concurrent tasks from different queries hit one copy, and that is where the sharing factor comes from.
- Capacity: 200 GB of host RAM holds TPC-H sf40 and TPC-DS up to about sf500; past that the cache serves its share and the NIC the rest (cost.md's tier rule).

### Local SSD, and the exotic tiers
- **Target first: local NVMe as a cache in front of the lake** (data-path.md section 5, layout c), but only where the drive is cheap. On the H100 shapes that have one, it buys nothing at its price: p5.4xlarge's drive (3.9 GB/s, $6.88/h) gives 1.47-1.49 at sf40, below the Nebius NIC tier. Nebius's H100 VM has no local disk. So RAM is the cache tier, and NVMe waits for a shape where it comes free.
- **One exotic tier changes the answer: GH200,** and through its price. Port the executor to aarch64 (it already runs on GB10) and keep the same seam. The fabrics (InfiniBand, CXL, GDS storage) do not move the ratio while the 16-core CPU side is the bottleneck.

### Batching API and whole-batch CBO
- **First, no API:** passive sharing in the executor cache (option E), with jobs submitted concurrently into one long-lived application (a Thrift or Connect server, so the cache outlives one spark-submit) and FAIR scheduling.
- **Then a Spark Connect command plugin** (option A) that carries N relations as one unit, if E's measured cut falls short.
- **Whole-batch CBO, above Catalyst.** It takes the window's N analyzed logical plans and decides what to share, keeping (file, column) chunks for the window's length. Catalyst, AQE and Comet's serializer then run per query, unchanged; AQE's per-query stage cache is not touched. Its statistics are Parquet footers plus run history (event logs of earlier runs of the same recurring jobs; more than 60% of SCOPE jobs recur, batching.md).
- **Probe-side holds,** after query-fuser's item 1a. Spark materializes every exchange, so across stages a hold is "keep the shared scan's chunks cached until the later stage reads them, or read them again". That is item 1a's buffer-or-re-read choice, priced in bytes kept against the load time saved. Inside one GPU block the rule applies as written: buffer at the edge with the smallest recorded `batch_bytes`, or re-read. Run history takes the place of the sf40 benchmark files.

### Size, against inventory.md's yardstick (22,830 lines; all estimates)

| Component | Reuse | Adapt | New |
|---|--:|--:|--:|
| Seam in Comet's native crate, `GpuStageExec`, JNI unchanged | | | 1,000 |
| Protobuf-to-IR translator (yardstick: DataFusion translator 2,129) | | | 2,500 |
| Node vocabulary (4,095): three new sources, Spark buffer layouts, Expand | 4,095 | 1,500 | |
| Spark-exact expressions on the device (ANSI overflow flags, Spark decimal typing and cast, `check_overflow`) | | | 3,000 |
| C++/cuDF executor (4,586): concurrent sessions, pinned buffers, chunked reader | 4,586 | 1,500 | |
| Driver and accountant (1,820): one session per task, a device semaphore | 1,820 | 800 | |
| Data path: object-store reads into pinned buffers | | | 1,500 |
| Executor column cache (pinned RAM, VRAM), soft affinity | | | 2,000 |
| Shuffle: murmur3 partition ids (reuse); lz4 on the GPU (new) | 225 | | 800 |
| Batching: Connect command plugin (phase 2) | | | 1,000 |
| Whole-batch CBO above Catalyst, with item 1a's hold rule (phase 2) | | | 2,000 |
| **Total** | about 10,700 | about 3,800 | about 13,800 |

New and adapted code together come to about 17,600 lines, 0.77× the engine as it stands. The alpha (phase 1) is about 14,600 lines without the plugin and the CBO.

### The hard part
- **Spark-exact semantics on the GPU, per task.** ANSI overflow errors with Spark's messages, Spark's decimal result types and cast table, and aggregate buffers that must interoperate with CPU partials across exchanges. All of it has to match Comet bit for bit, or a stage gives a different answer depending on where it ran. That is 1,625 ANSI-mode expressions in 104 queries.
- **And the 16-core CPU side.** Shuffle encode and decode, object-store I/O and broadcast decode run on 16 cores where the CPU baseline has 128. The model puts shuffle at the top of the sensitivity tables for TPC-H.

## Evidence against
- No headline cell is a go at mid values, and the one-pass floor is below 1. The case rests on modelled inputs (shuffle rate, Spark overhead share, the H100 VM's NIC) that no one has measured on a GPU node.
- The GPU numbers are hand-written cuDF pipelines on 4 TPC-H queries, not the engine (inventory.md); the speedup band (3.4-28×) is applied to everything else.
- The CPU baseline keeps moving: Comet 1.2.0 is 1.23× faster than 1.1.0 (data-path.md), and a sharing CPU erodes the ratio further (section 1).
- Small tasks: 8192-row batches per task, a broadcast rebuilt per task, and every split adds a device-to-host copy.

## Follow-up experiments
1. **Comet on 16 cores of the CPU VM** (no GPU): TPC-H and TPC-DS sf40, local and S3. It measures shuffle throughput per core, the per-stage scheduling gaps at 16 slots, and the 16-core time of the parts split.md leaves on the CPU. It settles the top sensitivity rows of three of the four cells.
2. **TPC-DS concurrent "all" from S3** on the CPU VM (the spec's matrix skipped it): settles the 1 TB cell's top row.
3. **S3 throughput on the H100 VM** with `s3_throughput.py`, and on a 16-vCPU CPU VM: replaces the assumed NIC rates.
4. **A passive-sharing simulation** over the dumped plans' column chunks under FAIR scheduling (batching.md follow-up 4): the cache's hit rate and the real cut at 10, 50 and 200 GB.
5. **A translator prototype** on TPC-H q1 and q6 inside a Comet fork, against Comet's answers under ANSI mode; then one GH200 hour running the same.

## Sources
- This folder: `cost.md` and `cost_model.py` (verdict, factors, sensitivity, tiers), `inputs.tsv`, `translatability.md`, `split.md`, `comet-internals.md` (seam, protobuf, executors), `batching.md` (options A-E, SCOPE overlap, Spark's reuse), `data-path.md` (layouts, rates, Comet's published results), `exotic.md` (tier table), `prior-art.md`, `measurements/` (Task 3 and Task 4, 2026-11-04), `plans/`.
- `llm-wiki/artifacts/product-research/inventory.md` @ f25c250d (yardstick, maturity, #149, #232); `llm-wiki/reports/benchmark-minimal.md` (H100 and H200 execute times); `llm-wiki/tasks/query-fuser.md` item 1a (the buffer-or-re-read rule); `llm-wiki/architecture.md` "Rehash and the comet hash".
- Comet 1.2.0 @ab46419fcc99bcd34aa4d89f7b545d5d51694a88: `native/core/src/execution/planner.rs:1626`, `jni_api.rs:1536-1564` (via comet-internals.md), https://github.com/apache/datafusion-comet/tree/ab46419fcc99bcd34aa4d89f7b545d5d51694a88 (accessed 2026-11-04).
