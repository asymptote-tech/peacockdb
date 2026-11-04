# Splitting a Comet plan between the CPU and the GPU

## Summary
- The unit is one Comet native block, per Spark task: the protobuf `Operator` tree Comet hands its native library (comet-internals.md, "The GPU seam"). The rule works inside that block; Spark's stages, exchanges and sinks stay where Comet puts them.
- **The rule:** the GPU takes the largest subtree of the block, grown from the leaves up, in which every operator, expression and aggregate is supported or adaptable (translatability.md). Everything above the first operator it cannot take stays on Comet's CPU path and reads the GPU's output as Arrow over the C Data interface. That is also how fallback works inside one stage.
- Scan, decode, filter, project, joins and aggregates go to the GPU. Shuffle encode and decode, broadcast decode, object-store I/O, JVM sinks and the few Spark-exact kernels the GPU lacks stay on the CPU, which on the H100 node is 16 cores rather than 128.
- Applied to the 104 final plans: 96 queries put every native operator on the GPU; TPC-H q2, q20 and TPC-DS q9, q10, q14, q24, q35, q99 keep 2-29% on the CPU. The per-query fractions are `plans/eligible.tsv`, which the cost model reads.
- The fraction counts operators, not time (no per-operator times were kept). The cost model treats it as a modelled input with a weight band, and charges the CPU part at 128/16 of Comet's time for it.

## The rule, per operator

| Part of the plan | Where | Why |
|---|---|---|
| Object-store reads (GETs, retries, credentials) | CPU threads | Comet's `object_store` path already does it; the bytes land in pinned host buffers |
| Parquet decode and decompress | GPU, when the block's `NativeScan` is in the GPU subtree | the bytes cross PCIe once, compressed; decode is the GPU's job (inventory.md item 6: 6.8-8.7 GB/s today) |
| Filter, project, Spark-exact arithmetic | GPU | ANSI overflow and decimal-precision checks become per-row flags on the device; the error itself is raised on the CPU, with Spark's message |
| Hash joins (broadcast and shuffled), sort-merge joins | GPU | a sort-merge join runs as a hash join on the same keys. The broadcast build arrives as Arrow from the JVM and is copied in once per executor, not once per task as Comet decodes it |
| Aggregates, partial and final | GPU | state columns in Spark's buffer layout, so a partial produced on the GPU can be merged on the CPU and the reverse |
| Sort, top-N, limit | GPU | |
| Shuffle write | GPU computes Spark's murmur3 partition ids (peacockdb's `spark_hash_partition.cu`) and scatters; the CPU encodes Arrow IPC, compresses (lz4) and writes the file | Comet's `ShuffleWriter` is the root of the block and builds on the executor; its file format is what the reading side expects |
| Shuffle read | CPU reads, decompresses and decodes the blocks; the batches are copied in | the `ShuffleScan` leaf becomes a host-batch source |
| Spark operators Comet did not convert (`fallbacks.tsv`) | CPU, unchanged | outside any native block |
| Operators with a missing expression or aggregate (rlike, bloom filters, structs, JVM UDFs, window) | CPU, Comet's own kernels | no device kernel; Comet's Rust kernel is already Spark-exact |
| Sinks: `TakeOrderedAndProject`, `CollectLimit`, `Union`, `Coalesce` | JVM | never in the protobuf |
| Small blocks (a few MB per task, a final aggregate of a few rows) | CPU (estimate) | copy and launch costs exceed the gain; the byte threshold is a follow-up measurement |

**Fallback inside one stage.** Within a block, `eligible_ops` in `scripts/plan_census.py` marks an operator GPU-able if it, its expressions and every operator below it are. The block then splits once: the GPU subtree runs to completion per batch and its root's output crosses to the CPU (one device-to-host copy per batch), where Comet's DataFusion operators run the rest of the block. A block whose leaf is not GPU-able runs entirely on the CPU. No block bounces back to the GPU above a CPU operator: one crossing per block keeps the PCIe traffic bounded, and in these plans the missing operators sit near the top of their blocks (filters and projections over a bloom filter, a struct-building project).

**What a planner needs to apply it:** the class tables in `plan_census.py` (operator by join type and attributes, expression by kind, scalar function by name, aggregate by kind), the bottom-up walk, and the byte threshold. All of it is per block and needs no statistics beyond what the task already carries.

## GPUs, executors and tasks

- One Spark executor per GPU node, with all 16 cores, and `spark.executor.resource.gpu.amount=1`, as spark-rapids requires one GPU per executor (comet-internals.md, "GPUs vs executors and tasks").
- Spark still runs up to 16 tasks at once in that executor. Each task's block becomes one peacockdb session; sessions share the device behind a semaphore, the way spark-rapids' `GpuSemaphore` does. peacockdb today runs one query per process with a single-threaded driver (inventory.md "Driver", "What else does not exist"), so concurrent sessions on one device are new work.
- Tasks are small: one split or one shuffle partition, in 8192-row batches by default (comet-internals.md). The GPU executor should coalesce a task's batches before launching kernels, and should keep a broadcast build side resident across the stage's tasks.

## The GPU-eligible fraction per query

`plans/eligible.tsv` columns: `blocks`, `native_ops`, `spark_ops` (from `fallbacks.tsv`), `gpu_ops` (operators in GPU subtrees), `eligible_fraction` = gpu_ops / (native_ops + spark_ops), and `missing` (what blocked the rest). Rows other than 1.000:

```tsv
dataset	query	blocks	native_ops	spark_ops	gpu_ops	eligible_fraction	missing
tpch	q2	15	61	0	57	0.934	bloomFilterAgg:2 bloom_filter_might_contain:1 func:xxhash64:3
tpch	q20	13	47	0	43	0.915	bloomFilterAgg:2 bloom_filter_might_contain:1 func:xxhash64:3
tpcds	q9	11	42	1	37	0.860	create_named_struct:5
tpcds	q10	10	36	13	36	0.735	
tpcds	q14	30	138	3	138	0.979	
tpcds	q24	20	84	0	78	0.929	jvm_scalar_udf:2
tpcds	q35	11	35	14	35	0.714	
tpcds	q99	6	28	0	27	0.964	jvm_scalar_udf:1
```

- **How the cost model uses it** (cost.md's header): per query, `cpu_part(q) = (Comet time − Spark overhead) × min(1, (1 − eligible_fraction) × w)`, run on 16 cores, so multiplied by 128/16. `w` (input `ineligible_weight`, 0.5-2) stands for an operator's time not being one operator's share.
- The GPU part is the rest: `compute(q) = (Comet time − Spark overhead) × eligible_fraction ÷ speedup`, except where a measured GPU time exists.

## Evidence against
- The split saves nothing on the parts Comet already does well on 128 cores and the GPU node has 16 for: shuffle encode and decode, object-store I/O, broadcast decode. TPC-H moves 46.4 GB and TPC-DS 56.5 GB of shuffle per pass (`measurements/comet/bytes.tsv`), all of it through those 16 cores.
- An operator count is a poor stand-in for time: one bloom-filter project in q2 costs little, while one CPU-side join above a GPU subtree could cost most of a query.
- Every split adds a device-to-host copy and an Arrow export per batch; at 8192-row batches that is many small copies.

## Follow-up experiments
1. Per-operator CPU times from Comet's SQL metrics (`spark.comet.explain.native.enabled` logs per-task metrics), to replace the operator-count weight with time.
2. Share of each stage's time in shuffle encode/decode and broadcast decode on the 128-core box (comet-internals.md follow-up); it bounds what moving the stage to the GPU can save.
3. The small-block threshold: time one block at 1, 8, 64 and 512 MB per task on the CPU and, once a prototype exists, on the GPU.
4. Concurrent peacockdb sessions on one GPU: throughput at 1, 4 and 16 sessions.

## Sources
- `comet-internals.md` (seam, task unit, shuffle and broadcast paths, executors and GPUs), `translatability.md` and `plans/{census,eligible}.tsv` (this folder), `scripts/plan_census.py` (`eligible_ops`).
- `llm-wiki/artifacts/product-research/inventory.md` ("Driver", "C++/cuDF executor", "Benchmark evidence" item 6, "What else does not exist").
- `measurements/comet/bytes.tsv`: shuffle bytes per pass (Task 4, cpu128, 2026-11-04).
- `llm-wiki/architecture.md` "Rehash and the comet hash": peacockdb's Spark-compatible murmur3.
