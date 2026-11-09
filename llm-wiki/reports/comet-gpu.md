# A GPU backend for Apache Comet: the cost case, explained

This document explains the research of the task `comet-research` and its result. It is a rewrite
of the report
[`comet-gpu.md`](https://github.com/asymptote-tech/peacockdb/blob/ad978c2411759201624553c5fe6327488b21cdb0/llm-wiki/reports/comet-gpu.md)
at commit `ad978c24` of the branch `ENS-comet-research`. It gives the same findings in a
different order. Each section uses only the concepts of the sections before it. The verdict is at
the end. The appendices hold the details that the argument does not need.

Clock times are in PST (UTC−8). All measurements were made on the evening of 2026-11-03 PST,
which is 2026-11-04 UTC. Web sources were read on 2026-11-04.

## 1. The question

Apache DataFusion Comet (Comet) is a Spark plugin. It replaces parts of a Spark physical plan with
native operators in Rust, built on DataFusion. Comet runs on CPUs only. A GPU backend for Comet
would run part of each Comet plan on a GPU, with the peacockdb engine.

This research supports two decisions:

1. Do we build a GPU backend for Comet?
2. If we build it, how do we build it?

An earlier product study rated the Spark plugin "strong" on demand. It rated the plugin "weak" on
build cost and on platform risk. That study was desk work only. This research adds measurements,
a cost model and a design.

The human set the **go threshold**: the GPU option must cost at least 2× less per query than
Comet on CPU. The next section defines how cost is compared.

## 2. What is compared

The GPU side is one Nebius H100 VM. It has one H100 GPU with 80 GB of VRAM, 16 vCPUs and 200 GB
of host RAM. Its price is $4.50 per hour. This document calls it the "H100 node".

The CPU side runs Comet 1.2.0 on CPU nodes. The human set a rule of **parity**: the GPU side and
the CPU side cost the same per hour. The price of a CPU node is linear in its cores. So $4.50 per
hour buys one of three CPU **layouts**. A layout is one way to spend the same money on CPU nodes:

| layout | nodes | cores per node | status |
|---|--:|--:|---|
| 1 × 128 | 1 | 128 | measured |
| 8 × 16 | 8 | 16 | modelled from 1 × 128 |
| 16 × 8 | 16 | 8 | modelled from 1 × 128 |

The human expects real clusters to use small nodes, each with its own NIC. So 8 × 16 is the main
comparison. Only 1 × 128 was measured; the model derives the other two layouts from it.

The **cost ratio** is the CPU cost per query divided by the GPU cost per query. A cost ratio of 2
means that the GPU side costs half as much. Because of parity, both sides pay the same price per
hour. So the cost ratio is also the CPU time divided by the GPU time. The go threshold is a cost
ratio of 2.

Cost per query is VM hours only. The cost of S3 requests is listed beside the cost ratio, not in
it. Idle time between jobs is not modelled. Egress is zero, because all S3 reads stay in one region.

Comet can run a set of queries in two ways. The **sequential baseline** runs the queries one after
another. The **concurrent baseline** runs all of them at the same time in one Spark application,
with the FAIR scheduler and one thread per query. The concurrent baseline is faster, because it
keeps the cores and the NIC busier. The decision rule (section 5) uses the faster one.

The model picks the faster baseline at each combination of inputs. At the input values, it is the
concurrent baseline.

## 3. The workload

The queries come from two benchmarks:

- TPC-H: all 22 queries, q1 to q22.
- TPC-DS: 82 queries. The list leaves out 17 queries, among them all queries with window
  functions. peacockdb can plan these 82 today.

The **scale factor** (sf) sets the size of a TPC dataset. The model measures size as compressed
Parquet bytes, from the Parquet file footers. At sf40, TPC-H is 42.4 GB and TPC-DS is 15.3 GB.
The same scale factor gives different byte counts for the two benchmarks.

Some results use a size of "1 TB" or "10 TB". These sizes are compressed bytes, not scale factors.
"1 TB" is about SF940 for TPC-H and about SF2,600 for TPC-DS. "10 TB" is about SF9,400 and
SF26,000.

The main workload is batched ETL. Many scheduled jobs read the same tables in one time window, and
no person waits for one answer. A **batch** is a set of queries that run in the same window. The
model uses four batch sizes: the first 3, 10 or 20 queries in run order, and "all" (22 for TPC-H,
82 for TPC-DS).

In a batch, the GPU node can read each column once for all the queries that need it. This is a
**shared scan**. The shared scan reads the union of the columns of the batch. For TPC-H "all", it
reduces the bytes read from 48.3 GB to 9.1 GB. In the model, only the GPU side gets shared scans.
Section 11 shows the effect of shared scans on the CPU side too.

The **cut** of a batch is the bytes read with each query alone, divided by the bytes read with the
shared scan. For TPC-H "all", the cut is 5.31.

The worst case is **one-pass** ETL: one query runs alone, so nothing is shared. The model computes
a one-pass cost ratio for each query against the sequential baseline. The results give the median
and the worst of these per-query cost ratios.

## 4. How time is estimated

No GPU ran in this research. The human decided that there would be no peacockdb runs. So the GPU
time comes from a model. The CPU time comes from measurements on 1 × 128 and from a model for the
small-node layouts. The model is the script
[`cost_model.py`](../artifacts/comet-research/cost_model.py).
The script writes
[`cost.md`](../artifacts/comet-research/cost.md),
and `test_cost_model.py` tests the script.

Three terms come first, because both formulas use them:

- The **wire** is the compressed Parquet bytes that a query or a batch reads from storage. The
  model takes these bytes from the measured Comet runs. A batch with a shared scan reads the union
  of its columns, from the sf40 footers.
- A **tier** is the place that the GPU node reads its data from. The default tier is the NIC,
  which reads from S3 object storage. Other tiers are a cache in VRAM, local NVMe and some faster
  links (section 9). Each tier has a read rate and a price.
- The **decode rate** is how fast the GPU turns compressed Parquet into columns. Its unit is GB/s
  of compressed input.

### The GPU time

The model gives the time of one query, or of one batch, on the H100 node:

```
T_gpu = overhead + max(wire / rate(tier), wire / decode_rate, compute) + shuffle / shuffle_rate + cpu_part
```

Each term means this:

- `overhead` is the time that Spark spends outside the operators. It is the measured driver time
  (`driver_ms`) plus a share of the measured scheduler time (`scheduler_ms`). The share is
  `scheduler_share`, 0.25 (low 0, high 1). The overhead of a query is never more than its own
  Comet time.
- The share is necessary because `scheduler_ms` adds the gaps of stages that run in parallel. So
  it can be longer than the query: TPC-H q8 has 7.1 s of gaps in a 2.9 s query.
- In a batch, queries plan and schedule while other queries compute. So a batch pays only a
  part of the summed overhead: `overhead_overlap`, 0.5 (low 0.2, high 1.0).
- `wire / rate(tier)` is the time to move the wire to the node. For the NIC tier, the rate is the
  S3 rate of the H100 VM times `s3_efficiency`.
- The S3 rate of the H100 VM is `rate_h100_s3`: 1.25 GB/s (low 0.917, high 12.5), a modelled
  value. Nebius states no NIC rate, so the model assumes 10 to 100 Gbit/s.
- `s3_efficiency` is 0.892, a measured value: the part of the best S3 rate that Comet's concurrent
  run used.
- `wire / decode_rate` is the time to decode the wire on the GPU. The decode rate is 4.1 GB/s (low
  2.34, high 18.8), a modelled value.
- `compute` is the time of the GPU operators. For TPC-H q1, q3, q6 and q8, it is the measured H100
  time from the earlier report benchmark-minimal.
- For all other queries, `compute` is the Comet time, times the eligible fraction, divided by
  `gpu_speedup`. The speedup is 10.1 (low 3.4, high 28.3).
- The eligible fraction is the share of a query's operators that the GPU can run. Section 8 shows
  how it was found.
- `max(...)` takes the slowest of transfer, decode and compute, because the three overlap in a
  stream.
- `shuffle / shuffle_rate` is the shuffle time. Comet's shuffle stays on the CPU (Arrow IPC with
  lz4) and uses the 16 cores of the node. One node shuffles inside itself, so no NIC is involved.
- `shuffle_rate` is the input `shuffle_rate_gpu_node`: 4.0 GB/s (low 2.0, high 20.0), a modelled
  value. The high value assumes lz4 on the GPU.
- `cpu_part` is the time of the operators that the GPU cannot run. They run on the 16 cores of
  the node instead of 128 cores.

The VRAM tier holds decoded columns. So a query that reads from VRAM skips the wire and decode
terms.

### The CPU time

The CPU time starts from the measured Comet runs on 1 × 128. In the "local" runs, Comet read data
from RAM (section 6 explains why). In the S3 runs, Comet read the same data from S3. The model
then adds the effects of size and of small nodes. In the formula, k is the number of nodes in the
layout:

```
T_cpu = fixed + work x size^a x cluster_overhead x spill + penalty x size x (R_128 / R_dep)^b + shuffle x (k-1)/k / NIC_dep
```

Each term means this:

- `fixed` is the Spark overhead, the same as on the GPU side.
- `work` is the measured local time less `fixed`.
- `size^a` scales the work with data size. The exponent `size_exponent` is 0.7 (low 0.5, high
  1.0), from Comet's published runs past sf40.
- `cluster_overhead` is the cost of k small nodes against one large node: 1.1 (low 1.0, high
  1.3).
- `spill` is `spill_factor`, 1.3 (low 1.0, high 2.0). It applies only when the shuffle of one
  query on one node is larger than the executor memory.
- `penalty` is the measured S3 time less the local time on 1 × 128. It is the extra time to read
  from S3 instead of RAM.
- `(R_128 / R_dep)^b` shrinks the penalty when the layout has more total S3 bandwidth.
- `R_128` is the measured S3 rate of cpu128, 1.164 GB/s.
- `R_dep` is k times the S3 rate per node, up to the 20 GB/s limit of one bucket. `NIC_dep` is
  the total NIC rate of the k nodes.
- The S3 and NIC rates of the small nodes are assumptions, because Nebius states none (Appendix
  E).
- The exponent b (`s3_penalty_bandwidth_share`) is the part of the penalty that depends on
  bandwidth: 0.5 (low 0, high 1).
- `shuffle x (k-1)/k / NIC_dep` is the shuffle over the network. On k nodes, (k−1)/k of the
  shuffle bytes go through the NICs. On 1 × 128, k is 1, so this term is zero.

### Size

Bytes grow in proportion to size. The work grows as size to the power 0.7 on both sides. The Spark
overhead does not grow with size. On the GPU side, spill applies when the shuffle of one query is
larger than the 200 GB of host RAM. peacockdb cannot spill today.

### The two factors of the cost ratio

The model splits each batched cost ratio into two factors. This shows where the gain comes from:

- The **sharing factor** is the GPU time when each query reads alone, divided by the GPU time
  with the shared scan. It measures the gain from the shared scan.
- The **device factor** is the CPU time divided by the GPU time when each query reads alone. It
  measures the gain from the GPU node itself.

The cost ratio is the sharing factor times the device factor. For example, take TPC-H at sf40, batch
"all", the concurrent baseline and the 8 × 16 layout. It has a sharing factor of 2.38 and a device
factor of 0.79. Its cost ratio is 2.38 × 0.79 = 1.88. A device factor below 1 means that, without
sharing, the GPU node is slower than the CPU side for the same money.

## 5. Uncertainty and the decision rule

Each input of the model has a tag:

- `measured`: the value comes from a file under `measurements/` of this task.
- `published`: the value comes from a source document, which can be an earlier report in the
  repository.
- `modelled`: the value is an assumption or an estimate derived from other values.

A test in the model rejects any other tag. Most GPU-side inputs are `modelled`, because no GPU ran.

Each input has a value, a low value and a high value. The **range** of a result is the lowest and
the highest cost ratio over all combinations of these low and high values. This document writes a
result as its value followed by its range: 1.88 [0.52-12.88].

The ranges are wide. Several modelled inputs have a high value 2 to 10 times the low value. The
top of a range needs every GPU-side input at its best value at the same time.

A **sensitivity table** shows which input changes one result the most. Each row moves one input to
its low value and then to its high value, with all other inputs at their values. The rows are in
order of the change, largest first. These are the first three rows for TPC-H at sf40 (value
1.88):

| input | cost ratio at the low value | cost ratio at the high value | tag |
|---|--:|--:|---|
| `shuffle_rate_gpu_node` | 1.29 | 2.97 | modelled |
| `scheduler_share` | 2.14 | 1.47 | modelled |
| `rate_h100_s3` | 1.69 | 2.27 | modelled |

The input in the top row has the most effect. A measurement of that input settles the result
first. This document calls it the **settling measurement**. Appendix F has the full sensitivity
tables.

The human fixed the decision rule on 2026-11-03, before any number existed:

- Go: the low end of the range is 2 or more.
- No-go: the high end of the range is less than 2.
- **Conditional go**: the range includes 2. The settling measurement decides it.

The rule applies to four results only. Each one is a **decision case**. All four use the same
conditions:

- the batch "all";
- the faster CPU baseline at each combination of inputs (at the input values, the concurrent
  baseline);
- parity with the 8 × 16 layout;
- data read over the NIC tier.

The model computes four sizes: sf40, sf200, "1 TB" and "10 TB". The four decision cases differ in
dataset and size:

| dataset | size | why this size |
|---|---|---|
| TPC-H | sf40 (42 GB) | the measured size |
| TPC-H | sf200 (212 GB) | the first size in the model's grid past the 200 GB of host RAM |
| TPC-DS | sf40 (15 GB) | the measured size |
| TPC-DS | "1 TB" (about SF2,600) | the first size in the grid past the host RAM |

The second size is there because real ETL tables often do not fit in RAM. For TPC-DS, the true
crossing of the host RAM is near sf520. So the "1 TB" case is about 5× past the crossing, and its
cost ratio is lower than a case at the crossing.

One input of the decision cases is not measured. The plan of experiments ran TPC-DS concurrently
only from RAM, not from S3. So the concurrent S3 time of TPC-DS is modelled: 124.4 s (low 87.5,
high 161.1). The model starts from the TPC-DS local concurrent time, 87.5 s. It adds the TPC-DS
sequential S3 penalty, 73.6 s, times 0.50. The factor 0.50 is the TPC-H ratio of the concurrent
S3 penalty to the sequential S3 penalty.

## 6. What was measured

Three experiments ran on two Nebius VMs. No experiment used a GPU for computation.

- "l40s" is an L40S GPU VM with 8 vCPUs in eu-north1. It was used only to measure S3 reads.
- "cpu128" is a CPU VM with 128 vCPUs and 512 GB of RAM in eu-west1. It ran all Comet runs.

### Experiment 1: S3 read rates

The script [`s3_throughput.py`](../artifacts/comet-research/scripts/s3_throughput.py) read one 9.03 GB Parquet object
(`tpch-sf40/lineitem.parquet`) with ranged GET requests of 64 MiB each. It also read the boot
disk with `O_DIRECT`, after it dropped the page cache. Results are in
[`measurements/s3/`](../artifacts/comet-research/measurements/s3/notes.md).

| host, time (2026-11-03) | source | best GB/s (Gbit/s) | threads at best | other thread counts |
|---|---|--:|--:|---|
| l40s, 19:18 PST | S3 eu-north1, same region | 0.917 (7.33) | 16 | 0.129 (1), 0.568 (4), 0.862 (64) |
| cpu128, 20:01 PST | S3 eu-west1, same region | 1.164 (9.31) | 4 | 0.195 (1), 1.163 (16), 1.028 (64) |
| cpu128, 20:01 PST | S3 eu-north1, other region (reference) | 0.902 (7.22) | 16 | |
| l40s, 19:29 PST | boot disk, network SSD | 0.062 | 64 | 0.061 at 1, 4, 16 |
| cpu128, 20:06 PST | boot disk, network SSD | 0.063 | 16 | 0.062 at 1 |

Each row is one host and one source. "Best GB/s" is the highest read rate over the thread counts.

- Nebius does not state a NIC rate for these VM types. The guest OS does not show a link speed.
- The limit of about 1 GB/s can come from the NIC or from the client. The test does not settle
  this. The cpu128 client used fewer than 3 of 128 cores at the limit. So a network limit per VM
  is more probable (estimate).
- The disk of these VMs is a network disk. It is 15 to 19 times slower than S3. Nebius sells local
  NVMe only on its 8-GPU B300 VM.

### Why "local" means "in RAM"

The cpu128 VM has one disk: a 100 GiB network disk that reads at 0.06 GB/s. That disk is too slow
for a fair test. So the data, the Spark shuffle files and the spill files were put on a tmpfs, in
RAM. The "local" runs therefore read from RAM. This is the strictest CPU baseline: it is faster
than data in the page cache of a disk.

### Experiment 2: Comet on CPU

One Spark application ran with one executor on all 128 cores of cpu128. It ran TPC-H q1 to q22 and
the 82 TPC-DS queries. It ran each set sequentially and concurrently, from local data and from S3
in the same region. The script is [`comet_run.py`](../artifacts/comet-research/scripts/comet_run.py), and the results are
in [`measurements/comet/`](../artifacts/comet-research/measurements/comet/summary.tsv).

The table gives the time of the batch "all", measured from 20:12 to 20:27 PST. The last column is
the S3 time divided by the local time:

| run | local (in RAM) | S3, same region | S3 ÷ local |
|---|--:|--:|--:|
| TPC-H sequential | 59.6 s, 46.9 s (2 runs) | 74.1 s | 1.24-1.58 |
| TPC-H concurrent | 38.3 s, 33.9 s (2 runs) | 46.5 s | 1.21-1.37 |
| TPC-DS sequential | 122.3 s | 196.0 s | 1.60 |
| TPC-DS concurrent | 87.5 s | not run | |

- The S3 sequential times are the sums of the per-query times. The batch rows of these runs, 84.7 s
  and 265.6 s, also include the plan dumps. Appendix D explains this.
- The NIC already limits Comet on CPU. TPC-H concurrent from S3 read 48.3 GB in 46.5 s. That is
  1.04 GB/s, or 0.89 of the best S3 rate of the VM.
- The sequential runs read at only 0.43 to 0.56 of that rate. One query at a time does not fill
  the NIC.
- The second TPC-H run was 21% faster than the first, because of JIT warm-up. TPC-DS ran once, so
  its times include some warm-up.
- The scan bytes agree with the Parquet footers: 0.983 to 1.000 for TPC-H q1 and TPC-DS q3, q42
  and q96. TPC-H q6 and TPC-DS q14 read less (0.153 and 0.028), because date filters skip row
  groups.
- No query spilled. For TPC-H, a pass shuffles about as many bytes as it scans (46.4 GB against
  48.3 GB). For TPC-DS, it shuffles 56.5 GB against 98.4 GB scanned.
- Comet ran every query end to end, except TPC-DS q9, q10, q14 and q35. Spark ran a part of each of
  these four. The parts are a project with a struct type, an existence join and a count merge, and a
  decimal AVG at maximum precision.
- No run was skipped, stopped early or out of memory, and no scan differed between runs.

### Experiment 3: plan dumps

The two calibration passes ran each query once before the measured runs. They saved the final
executed plan of every query, and the protobuf that Comet
sends to its native code. They are in [`plans/`](../artifacts/comet-research/plans). Sections 8 and 10 use them.

## 7. Prior art

The full study is [`prior-art.md`](../artifacts/comet-research/prior-art.md).

- No one has built, forked or proposed a GPU backend for Comet. Comet's tracker has no GPU issue,
  PR or discussion. The Comet 1.2.0 README says that it needs no GPU or other accelerator.
- The nearest attempt stopped early. Auron PR #1001 connected a Spark engine on DataFusion to
  libcudf. It covered scan, filter and project only. It was closed as stale on 2025-12-05.
- Comet opened the Native Executors epic #7288 on 2026-10-28. A native executor would get "only
  proto bytes, a partition index, input streams and config" and return an output stream.
- spark-rapids is mature and sold commercially. Gluten with Velox-cuDF is experimental, and its
  table scan stays on the CPU by default. Photon runs on CPU only.
- Speedata (ASIC), NeuroBlade and DualBird (FPGA) are vendor plugins, with numbers from the vendors
  only.
- In each of these systems, scan and I/O decide the result, not compute. For example, Theseus
  reduced its cloud run time by 75% when it replaced its S3 client.
- Published GPU cost gains against CPU-native engines are 1.1× to 2.9×. This span includes the go
  threshold of 2×. No study compares a GPU with Comet.
- An AWS study on EMR G7 shows 1.42× to 1.54× against Spark, not Comet. Theseus against Photon at
  equal cost shows 1.12× at SF1k and 4.1× at SF30k.
- Some evidence is against GPU engines. Voltron Data, the company behind Theseus, shut down. Gluten
  removed Intel IAA support after it saw no performance gain.
- spark-rapids removed its hybrid CPU scan, because few users used it. NVIDIA tells engine builders
  to use the Velox cuDF backend.
- The CPU baseline gets faster. Comet 1.2.0 ran TPC-DS SF1000 2.51× faster than Spark 4.1.3, and
  2.04× faster than Spark 4.2.0.
- The earlier study `spark-plugin.md` gave leads. Each lead was checked again at current versions,
  and each one is still true. This document uses no fact from `spark-plugin.md` alone.

## 8. Can a GPU attach to Comet?

This section is a summary of [`comet-internals.md`](../artifacts/comet-research/comet-internals.md),
[`translatability.md`](../artifacts/comet-research/translatability.md) and [`split.md`](../artifacts/comet-research/split.md). The code facts are
at Comet commit `ab46419f` (release 1.2.0).

### How Comet runs a plan

Comet 1.2.0 runs DataFusion 55.2.0 and arrow-rs 59.2.0 in native code. Its default build is for
Spark 4.1.3. peacockdb plans on DataFusion 45 and arrow-rs 54.2.

1. Comet adds rules to the physical planning of Spark. `CometScanRule` replaces Parquet scans.
   `CometExecRule` walks the plan from the bottom up and maps Spark operators to Comet operators.
2. An operator converts only if all its children are native. If not, the operator stays in Spark,
   with a recorded fallback reason.
3. A **native block** is a maximal native subtree: it cannot grow without crossing a JVM boundary.
   Comet sends each native block to the executors as protobuf.
4. Each Spark task runs one native block once. The task adds its own list of files to the block
   and calls `Native.createPlan`.
5. Rust code builds a DataFusion plan from the protobuf and runs partition 0 only.

Expressions are trees in the protobuf. A flag (`EvalMode`: LEGACY, TRY or ANSI) gives the Spark
semantics of each one.

Data moves through Comet in these ways:

- Parquet is read in native code through `object_store`, with no decode in the JVM.
- The native shuffle writes compressed Arrow IPC (lz4 by default), one file per map task.
- Results go back to the JVM over the Arrow C Data interface.
- Each task decodes each broadcast again and builds its hash table again.

Comet needs the off-heap memory mode of Spark. Its memory pool calls the Spark
`TaskMemoryManager` over JNI. Native operators spill by themselves; Spark cannot make them spill.
GPU memory would be outside all of this (estimate).

Comet 1.2.0 has only one native Parquet scan. So the local runs and the S3 runs used the same scan.
The plan dumps confirm this.

### Where a GPU executor fits

A GPU executor can fit at one place: the native step that turns the protobuf of a task into a
plan. This document calls it the **insertion point**. It is the function
`PhysicalPlanner::create_plan` (`planner.rs:1626`).

- The insertion point receives the decoded `Operator`, with the file list of the task, and the
  Arrow inputs of the task.
- It returns a DataFusion 55 `SendableRecordBatchStream`. Comet sends that stream to the JVM
  without change.
- Comet has no plug-in interface here. The operator registry is a private singleton, fixed at
  compile time.
- So a GPU executor needs a fork of the native crate of Comet, pinned to one release.
- A swap of blocks on the JVM side is possible. But it depends on `private[spark]` classes
  (estimate).

### Can peacockdb run Comet plans?

The script [`plan_census.py`](../artifacts/comet-research/scripts/plan_census.py) classed each item in the 104 final plans
against the plan nodes of peacockdb. There are three classes:

- supported: peacockdb has a matching node.
- adaptable: peacockdb has a node, but it needs changes for Spark semantics.
- missing: peacockdb has no matching node.

| item | supported | adaptable | missing |
|---|--:|--:|--:|
| operators | 2,647 | 2,374 | 0 |
| expressions and aggregates (26,944) | 93.2% | 6.7% | 20 (0.07%) |

- No operator is missing. The 20 missing expressions are in five queries. They are a runtime bloom
  filter (TPC-H q2, q20), a struct (TPC-DS q9) and a JVM codegen UDF (TPC-DS q24, q99).
- The adaptable items are the real work. 1,625 expressions use ANSI mode. Spark's decimal types and
  aggregate buffer format also need work.
- Sort-merge joins must run as hash joins. And there are three new sources: Parquet from object
  storage per task, Arrow from the JVM, and Comet shuffle blocks.
- The query list favors this result, because it leaves out the window queries.

The difference in DataFusion versions costs little if the translator reads the protobuf of Comet,
not a DataFusion plan. Then the two engines meet only as Arrow data over the C Data interface.
peacockdb does not use DataFusion 45 on this path. A translator that reads DataFusion 55 plans
would need to move the peacockdb planner ten major versions of DataFusion (estimate).

### How a plan is split between GPU and CPU

The split works inside one native block. The GPU takes the largest subtree, grown from the leaves,
in which every operator and expression is supported or adaptable. The CPU runs the rest, from the
Arrow output of the GPU. Each block has one copy from device to host.

| on the GPU | on the CPU |
|---|---|
| Parquet decode, filter, project, Spark-exact arithmetic (the CPU raises errors) | reads from object storage into pinned host buffers |
| hash joins, and sort-merge joins run as hash joins | shuffle encode, compress and write; shuffle read and decode |
| partial and final aggregates, in Spark's buffer format | broadcast decode; JVM sinks; Spark operators that Comet did not convert |
| sort, top-N, limit; Spark murmur3 partition IDs for the shuffle | operators with a missing expression; small blocks (estimate) |

In 96 of the 104 queries, the GPU takes every native operator. The other 8 queries keep 2 to 29%
of their operators on the CPU. The eligible fraction of section 4 comes from this split. It counts
operators, not time.

### GPUs and Spark tasks

The H100 node runs one Spark executor with all 16 cores and one GPU. spark-rapids also requires one
GPU per executor. Up to 16 tasks then share the GPU behind a semaphore, as in spark-rapids. Today
peacockdb runs one query per process. So more than one session on one GPU is new work.

## 9. The data path

This section is a summary of [`data-path.md`](../artifacts/comet-research/data-path.md) and [`exotic.md`](../artifacts/comet-research/exotic.md).
It asks how fast data can reach the GPU, and from where.

### Where the GPU stalls on the NIC

The stall rate of a query is a NIC rate. Below it, the transfer over the NIC is the slowest term in
`max(...)`, so the GPU waits for data. The numbers below are for one TPC-H query alone at sf40,
with all inputs at their values.

- With the decode rate at 4.1 GB/s, 19 of the 22 queries stall below 4.1 GB/s (32.8 Gbit/s).
  Above that rate, the decoder is the limit, not the NIC.
- q14, q15 and q16 stall below 1.5 to 2.9 GB/s, because compute takes most of their time.
- The decode rate of 4.1 GB/s comes from H200 load times, and the model uses it for the H100. If
  memory bandwidth limits decode, the H100 decodes at about 0.7 of it, near 2.9 GB/s (estimate).
- With the decode rate at its high value (18.8 GB/s), the stall rates go from 1.5 to 18.8 GB/s.
  Queries with joins and large scans (q3, q8, q17, q19) need the most.
- At the assumed H100 rate of 1.25 GB/s times 0.89, every query stalls on the NIC. So a slower
  decode does not change the decision cases.
- The peacockdb reader today decodes about 3.9 GB/s of compressed data on q6, on the H200. That is
  slower than a 40 Gbit/s NIC. The reader must get faster before the NIC is the question.

### Ways to get more out of the NIC

- Parquet already crosses the NIC compressed, for the CPU and for the GPU. So the question is the
  speed of decode on the GPU.
- The H100 has software decompression only. The hardware engine of the B200 decompresses Snappy at
  117 GB/s of output.
- Column and row-group pruning cuts bytes for both sides by the same amount (estimate). TPC-H q6
  reads 0.15 of the footer bytes of its columns.
- Local NVMe as a cache speeds up repeated reads on CPU engines. Microsoft Fabric reports "up to
  60%".
- The shuffle is large: one TPC-H sf40 pass shuffles about as many bytes as it scans. The H100 node
  shuffles inside itself, but with 16 cores. That is why the shuffle rate is the top row of the
  TPC-H sensitivity tables.

### A cache in RAM

- Spark's own cache cannot hold data between jobs. The `CacheManager` belongs to one Spark
  application, so separate spark-submit jobs share nothing.
- Comet has an Arrow cache, but it is "experimental and disabled by default". It stores data in
  chunks on the JVM heap.
- Comet's cache gave no gain in this test. With the TPC-H tables cached, the local runs took 53.3 s
  sequential and 36.1 s concurrent (20:27 to 20:30 PST).
- Without the cache, the same runs took 53.2 s and 36.1 s (the mean of two runs). The data was in
  RAM already.
- A GPU cache must be in the native executor: pinned host RAM first, then VRAM. It must be outside
  Spark's BlockManager, and one long-lived application must hold it (estimate).
- The RAM per GPU is about 200 to 320 GB on every cloud and on a DGX H100. On-premises servers do
  not have more. So the plugin is not only for on-premises use.
- The 200 GB of the H100 node holds at most about sf190 of TPC-H or sf520 of TPC-DS, as compressed
  bytes.
- `data-path.md` gives "near SF390" for TPC-H in host RAM. That value came from sf1 files, which
  compress differently. It is about 2.8× too high.

### Data on local SSD

These facts come from documents only. No local SSD was measured.

- CPU instances made for storage read 1.3 to 3.8 times their NIC rate from local NVMe. Instances
  with one GPU read about 0.5 times their NIC rate, or the rate is not published.
- For example, AWS i4i.4xlarge reads 2.8 GB/s from NVMe, with a 1.17 GB/s base NIC rate. GCP
  a3-highgpu-1g reads 1.4 GB/s from two drives, with 3.1 GB/s of NIC.
- GPUDirect Storage (GDS) on cloud VMs runs in "compatibility mode". It then reads through host
  memory, as POSIX reads do. A direct path from NVMe to the GPU needs bare metal.
- Local NVMe loses its data when the VM stops, on AWS, GCP, Azure and Nebius. A refill of the
  3.84 TB drive of p5.4xlarge from S3 takes about 5 to 7 minutes (estimate).
- In the model, the local NVMe tier gives no gain at its price. The drive of p5.4xlarge gives 1.49
  (TPC-H) and 1.42 (TPC-DS) at sf40, below the NIC tier of Nebius.

### Local SSD as a product option

There are three ways to use local SSD:

1. Local SSD as the main store. This is rare. Meta replaced Presto Raptor, which kept tables on the
   flash of the workers, with RaptorX, a cache in front of the warehouse. No Spark or Comet
   production system with local SSD as its main store was found.
2. HDFS on the disks of the compute nodes. This is real on premises, but it gets smaller. Uber runs
   about 1.5 EB of HDFS and moves its data lake to GCS. Comet's HDFS support is "experimental".
3. Local NVMe as a cache in front of the data lake. This is common. The Databricks disk cache and
   the Fabric intelligent cache are on by default. Meta RaptorX and the Presto cache of Uber use it.

The recommendation is option 3, with pinned host RAM before NVMe. On instances with one GPU, NVMe
is not faster than the NIC. Managed platforms use this design today.

### How other systems feed the GPU

- spark-rapids puts the files of a task into one host buffer and decodes on the GPU. Its UCX
  shuffle is an option, and its file cache is off by default.
- Gluten-cuDF keeps the scan on the CPU and copies the result to the GPU.
- Presto with Velox-cuDF reads through KvikIO. Its UCX exchange reduced a TPC-H SF1000 run from 828
  s to 93 s.
- Theseus got its cloud speed when it wrote a new S3 client.

### Other tiers

[`exotic.md`](../artifacts/comet-research/exotic.md) studied faster tiers. The cost model gives each one a cost ratio at
its own price (section 11). Each row below is one tier; "GB/s" is the rate to one GPU.

| tier | GB/s to one GPU | can you rent it? | does Spark or Comet use it today? | finding |
|---|---|---|---|---|
| RDMA to a memory server (GPUDirect RDMA, Kove:SDM, research on far memory) | about 24 per 400G NIC (Mooncake) | peer RAM only on 8-GPU nodes; Kove has no price | no | a cache, not a source |
| CXL memory pools and switches | about 20, measured from an H100 | no GPU instance anywhere | on the CPU side only | adds capacity, not speed |
| GH200 NVLink-C2C | 375 from host to device | yes, $1.99 to 2.29 per hour | Comet runs on arm64 (CPU side) | changes the answer, because of its price |
| GB200 NVL72 rack memory | 900 peer HBM | $10.50 per GPU-hour | no | access to remote Grace memory not verified |
| GDS on VAST, WEKA, DDN; S3 over RDMA | 22 to 28 for GDS (28.3 measured into one L40S, 21.7 from a vendor); about 46 for S3 over RDMA (estimate) | on premises | no; needs a cuFile reader | the only exotic tier that helps one-pass ETL |
| cloud InfiniBand or EFA | 12.5 per GPU over IP on p5.48xlarge | only on 8-GPU nodes | Comet shuffles over Spark's TCP | does not fit parity with one GPU per node |

None of these tiers makes the cold one-pass path faster in the cloud.

## 10. Batching on Comet

This section is a summary of [`batching.md`](../artifacts/comet-research/batching.md).

### What Spark shares today

Spark shares work only inside one query, through exchange reuse, subquery reuse and subplan
merging. Across queries, it shares only through an explicit cache. Comet 1.2.0 keeps this reuse.
Its protobuf has one root per native block. No batch API exists.

### Ways to send a batch

These are the options, cheapest first:

- E. Passive sharing in the GPU executor. This needs no API. The scan tasks of concurrent queries
  use one column cache, keyed by file, row group and column. On one H100 node, all tasks run in one
  executor, so they share the cache. This is the first recommendation (estimate).
- A. A Spark Connect command plugin that carries N relations as one unit. Add it if the measured
  cut of option E is too small.
- B and C are middle options. B is a server that collects queries over a time window. C is a run of
  Declarative Pipelines.
- D, one plan with many outputs, does not exist in Spark. It costs the most.

### An optimizer for the whole batch

Catalyst, AQE and the Comet rules see one query at a time. So an optimizer for a whole batch must
work above Catalyst. It takes N analyzed plans and decides what to share. Its statistics come from
Parquet footers and from run history. More than 60% of SCOPE jobs at Microsoft recur.

The earlier task query-fuser has a rule (item 1a) that holds probe-side data for a later stage.
Spark writes each exchange to completion, so this rule changes. It becomes: keep the chunks of
the shared scan until the later stage reads them, or read them again.

### Evidence from other systems

- Shared scans pay at high concurrency. QPipe shows about 2×, MQJoin 2× to 5×, and CJOIN up to 5×
  at 32 queries.
- Sharing every time is not free. Sharing chosen by a model beat sharing every time by 2.5× on
  average (Johnson et al., VLDB 2007).
- At low concurrency, shared operators lose to operators that run one query at a time
  (Psaroudakis et al., VLDB 2013).

### How much real ETL shares

In SCOPE production, CloudViews reduced the data read by 38.84% (about 1.6×) and the processing time
by 38.96%. Almost 40% of daily SCOPE jobs overlap with another job. That is reuse over time, not
shared scans in one window. So 1.6× is a case like production, not an upper limit.

A TPC batch of all queries overstates sharing. It reads one schema, writes nothing, and each query
is different. That is a best case. A batch of 3 to 10 queries is nearer to the 1.6× of SCOPE
(estimate).

### The wire under batching

The table gives the wire of each batch, with each query alone and with the shared scan, and the
cut. "Per query" is the shared wire divided by the queries in the
batch. Wire is in GB.

| batch | TPC-H alone → shared | cut | per query | TPC-DS alone → shared | cut | per query |
|---|---|--:|--:|---|--:|--:|
| 3 | 4.1 → 3.9 | 1.05 | 1.30 | 1.2 → 1.1 | 1.08 | 0.37 |
| 10 | 24.4 → 7.3 | 3.34 | 0.73 | 13.3 → 6.9 | 1.94 | 0.69 |
| 20 | 39.8 → 9.1 | 4.37 | 0.46 | 24.8 → 9.5 | 2.62 | 0.48 |
| all | 48.3 → 9.1 | 5.31 | 0.41 | 98.4 → 12.2 | 8.07 | 0.15 |

These measured cuts replace the earlier estimates of `batching.md`, which came from sf1 footers
(6.2× for TPC-H "all").
A union of columns loses row-group pruning. So the shared bytes are an upper limit for queries such
as q6.

## 11. Results

All numbers in this section come from [`cost.md`](../artifacts/comet-research/cost.md). Each result is a cost ratio
followed by its range. A value above 2 meets the go threshold at the input values.

### The decision cases

Each row is one decision case: batch "all", the faster baseline, the 8 × 16 layout and the NIC
tier. The last two columns are the two factors of the cost ratio.

| dataset | size | cost ratio [range] | sharing factor | device factor |
|---|---|---|--:|--:|
| TPC-H | sf40 (42 GB) | 1.88 [0.52-12.88] | 2.38 | 0.79 |
| TPC-H | sf200 (212 GB) | 1.38 [0.23-13.40] | 2.62 | 0.52 |
| TPC-DS | sf40 (15 GB) | 2.06 [0.55-13.32] | 2.42 | 0.85 |
| TPC-DS | "1 TB" (1,000 GB) | 1.44 [0.06-22.98] | 3.74 | 0.38 |

- Each range includes 2. Only one value, TPC-DS sf40, is above 2.
- The device factor is below 1 in all four cases (0.38 to 0.85). Without sharing, the H100 node is
  slower than the concurrent baseline for the same money.
- The sharing factor (2.38 to 3.74) lifts the cost ratio to about 2. So the gain comes from the
  shared scan, not from the GPU.

### Batch size and one-pass, at sf40

Each row is one workload against one CPU baseline, on the 8 × 16 layout and the NIC tier. A cell
gives the cost ratio and its range, then "s" for the sharing factor and "d" for the device factor.
Batches 3, 10 and 20 are compared with the sequential baseline only. The batch "all" is compared
with both baselines.

| workload | batch | baseline | TPC-H sf40 | TPC-DS sf40 |
|---|---|---|---|---|
| batched | 3 | sequential | 1.85 [0.67-13.65] s1.03 d1.79 | 3.62 [1.13-20.44] s1.04 d3.48 |
| batched | 10 | sequential | 2.17 [0.61-17.92] s1.92 d1.13 | 1.75 [0.55-13.05] s1.35 d1.29 |
| batched | 20 | sequential | 2.61 [0.72-19.20] s2.18 d1.20 | 2.23 [0.64-15.71] s1.56 d1.42 |
| batched | all | sequential | 2.78 [0.76-19.62] s2.38 d1.17 | 3.02 [0.78-18.11] s2.42 d1.25 |
| batched | all | faster | 1.88 [0.52-12.88] s2.38 d0.79 | 2.06 [0.55-13.32] s2.42 d0.85 |
| one-pass | median | sequential | 0.89 [0.34-11.94] | 0.90 [0.33-9.11] |
| one-pass | worst | sequential | 0.42 [0.12-3.89] | 0.23 [0.06-1.22] |

- The median one-pass cost ratio is below 1 (0.89 for TPC-H, 0.90 for TPC-DS). So at the median,
  one query alone, read from S3, costs more on the H100 node than on the 8 × 16 cluster.
- For TPC-H, the local time of a query is the faster of a cold run and a warm run. The S3 time is
  one cold run. This favors the GPU a little.
- Against the sequential baseline, the device factors are above 1 (1.17 for TPC-H, 1.25 for
  TPC-DS). On 8 × 16, the modelled sequential time is about 1.5× the concurrent time (2.78/1.88 and
  3.02/2.06).

### Data size and layout

Each row is one dataset on one layout. Each cell is the cost ratio for batch "all", the concurrent
baseline and the NIC tier. A word in brackets shows where the data fits by compressed bytes: in
VRAM, on local NVMe, or nowhere ("none"). The H100 node of Nebius has no local NVMe. So past its
host RAM, its data comes over the NIC.

| dataset | layout | sf40 | sf200 | "1 TB" | "10 TB" |
|---|---|---|---|---|---|
| TPC-H | 8 × 16 | 1.88 (vram) | 1.38 (local_nvme) | 1.04 | 0.84 (none) |
| TPC-H | 16 × 8 | 1.76 | 1.23 | 0.89 | 0.68 |
| TPC-H | 1 × 128 | 1.83 | 1.36 | 1.05 | 0.87 |
| TPC-DS | 8 × 16 | 2.06 (vram) | 1.73 (vram) | 1.44 (local_nvme) | 1.14 (none) |
| TPC-DS | 16 × 8 | 1.95 | 1.56 | 1.22 | 0.90 |
| TPC-DS | 1 × 128 | 2.28 | 2.12 | 1.97 | 1.73 |

- The cost ratio falls as size grows. The 16 cores and the NIC of the H100 node do more of the
  work as data grows.
- For TPC-DS, the one 128-core node is the easiest comparison for the GPU.

### Other tiers

The model runs the batched cases again with each tier of section 9 and with local NVMe, at the
price of each tier.

- GH200 is the only tier with a value above 2 in all four decision cases. The values are 4.09 and
  3.20 for TPC-H sf40 and sf200, and 3.74 and 3.47 for TPC-DS sf40 and "1 TB".
- GH200 wins on price, not on its 375 GB/s link. It costs $2.29 per hour on Lambda, against $4.50
  for the H100 node. The model uses the H100 compute time for it.
- Tiers that only feed the GPU faster reach 2.1 to 2.3 at sf40 (TPC-H 2.16 to 2.27, TPC-DS 2.07 to
  2.18). These are NICs of 40 to 400 Gbit/s, GDS storage and the Nebius shared filesystem.
- With these tiers, the 16 CPU cores of the H100 node set the time.
- Faster and more expensive hosts (p5.4xlarge, B200, Azure ND, GB200) are below the NIC tier, at
  their prices.

### A CPU side with shared scans

The decision cases give shared scans to the GPU side only. Comet could get shared scans too. The
model shrinks the S3 penalty of the CPU side by the cut of the batch, and computes the decision
cases again:

| decision case | cost ratio, CPU without shared scans | cost ratio, CPU with shared scans |
|---|--:|--:|
| TPC-H sf40 | 1.88 | 1.76 |
| TPC-H sf200 | 1.38 | 1.23 |
| TPC-DS sf40 | 2.06 | 1.84 |
| TPC-DS "1 TB" | 1.44 | 1.00 |

On one 128-core node, the drop is larger: TPC-DS "1 TB" goes from 1.97 to 0.82, a drop of 58%.

### Costs beside the cost ratio

These costs are not in the cost ratio. Each row is one pass of all queries at sf40. All S3 reads
were in one region, so egress is zero. Idle time between scheduled windows is not modelled.

| dataset | CPU GET requests, sequential S3 run | CPU wire, GB | GPU wire with shared scan, GB |
|---|--:|--:|--:|
| TPC-H, all 22 | 62,809 | 48.3 | 9.1 |
| TPC-DS, all 82 | 260,416 | 98.4 | 12.2 |

The TPC-H count does not include the q1 warm-up query. With it, the count is 64,991.

## 12. The verdict

### The rule applied

The range of each decision case includes 2. So the verdict is a conditional go in all four decision
cases. No range is wholly above 2, so no case is a go. No range is wholly below 2, so no case is a
no-go.

### The settling measurements

Each row is one decision case and the input in the top row of its sensitivity table. "Reaches 2 at"
is the input value at which the cost ratio becomes 2. The thresholds come from
[`design.md`](../artifacts/comet-research/design.md) section 1.

| decision case | input | value used | reaches 2 at | how to measure it |
|---|---|---|---|---|
| TPC-H sf40 | `shuffle_rate_gpu_node` | 4 GB/s (modelled) | 4.6 GB/s | Comet on 16 cores of the CPU VM: shuffle write and read time per GB. No GPU needed. |
| TPC-H sf200 | `shuffle_rate_gpu_node` | 4 GB/s (modelled) | 9.6 GB/s | the same run; above about 8 GB/s it needs lz4 on the GPU |
| TPC-DS sf40 | `scheduler_share` | 0.25 (modelled) | already 2.06; below 2 above 0.28 | the scheduling gaps per stage on a 16-core executor, in the same run |
| TPC-DS "1 TB" | `size_exponent` | 0.7 (modelled) | 0.88 | Comet at sf40 and at a second scale factor, on the same CPU VM |

- One cheap run without a GPU settles three of the four cases: Comet on 16 cores.
- A CPU run at a second data size settles the fourth case.
- For TPC-DS "1 TB", two other inputs can also bring the value to 2. They are
  `shuffle_rate_gpu_node` at 9.1 GB/s (row 2) and `rate_h100_s3` at 4.5 GB/s (row 6).
- The modelled TPC-DS concurrent S3 time cannot lift that case to 2. At its high value, 161.1 s,
  the cost ratio is 1.94.

### What turns the conditional go into a go

- Measure first, without a GPU. Comet on 16 cores settles the shuffle rate and the scheduler
  share. Comet at a second scale factor settles the size exponent.
- In the same session, run TPC-DS concurrent "all" from S3. It replaces the modelled CPU time of
  both TPC-DS cases.
- These runs fit in about one hour of the CPU VM (estimate). The task used 0.66 h of its 2 h limit
  on that VM, so 1.34 h is left.
- The decode rate is not among the top inputs, because the NIC limits first.
- GPU speed matters only for TPC-DS sf40. The speedup moves TPC-H sf40 only from 1.72 to 1.88. It
  moves TPC-DS sf40 from 1.66 to 2.06.
- TPC-DS sf40 stays above 2 only while the speedup is above 6.5. The model uses 10.1 (low 3.4,
  high 28.3).
- For TPC-H, a faster GPU gives nothing until the NIC and the 16-core CPU side get faster.
- Hardware and price: GH200 at $2.29 per hour is above 2 in every decision case. Faster data
  feeds alone are not.
- Workload: the gain grows with the cut of the batch. The real overlap of a design partner decides
  between a TPC "all" cut (5 to 8) and a SCOPE cut (about 1.6).
- If Comet gets the same shared scans, the values drop by 5 to 26% at sf40. On one 128-core node,
  TPC-DS "1 TB" drops by up to 58%.
- Then the case depends on the device factor, which must go above 1.

### The evidence against

- Three of the four values are below 2. The fourth, TPC-DS sf40, is 2.06.
- The median one-pass cost ratio is below 1 on 8 × 16.
- The GPU numbers come from cuDF pipelines written by hand for four TPC-H queries, not from the
  engine.
- Comet gets faster: 1.2.0 is 1.23× faster than 1.1.0.
- Comet tasks are small: batches of 8192 rows, and a broadcast is built again in each task.

### The alpha design

This is the design for a first version, if the result becomes a go. It is a summary of
[`design.md`](../artifacts/comet-research/design.md).

- Insertion point: fork `native/core` of Comet 1.2.0. In `PhysicalPlanner::create_plan`, a
  block with a GPU subtree becomes a `GpuStageExec` over the CPU part. JNI and the JVM side do
  not change.
- One executor runs on each GPU node, with 16 cores. Tasks share the GPU behind a semaphore.
- Translation: a translator reads the protobuf `spark_operator::Operator` and writes peacockdb
  plan nodes. It builds no DataFusion 55 plan for the GPU part.
- New sources: Parquet per task from `object_store`, Arrow from the JVM, and Comet shuffle
  blocks.
- Adaptations: Spark's aggregate buffer format, sort-merge as hash join, `Expand` as grouping
  sets, and Spark-exact arithmetic, cast and decimal rules.
- Split: the rule of section 8, per block. The shuffle stays in Comet. The GPU computes the murmur3
  partition IDs (`spark_hash_partition.cu` in peacockdb).
- The first optimization that the model asks for is lz4 on the GPU.
- Data path and cache: reads from object storage into pinned host buffers, and GPU decode with the
  chunked reader. A column cache in the executor uses pinned RAM first and VRAM second.
- Soft affinity sends the tasks of one window for one file to one node.
- Local SSD and other tiers: use a cache in RAM in front of the data lake. Add NVMe only on an
  instance where it is cheap. GH200 is the only other tier worth a port (aarch64).
- peacockdb already runs on GB10, which is also aarch64.
- Batching: first no API. Use passive sharing in the cache, and send jobs concurrently into one
  long-lived Thrift or Connect server, with FAIR scheduling.
- Then add a Connect command plugin and an optimizer for the whole batch above Catalyst. It uses
  the query-fuser rule of section 10, with input from run history.

### Size of the work

The estimates use `inventory.md` as the yardstick: the engine today has 22,830 lines.

| part | lines (estimate) |
|---|--:|
| reused | about 10,700 |
| adapted | 3,800 |
| new | 13,800 |
| adapted and new together | about 17,600 (0.77 × the engine) |
| the alpha without the Connect plugin and the optimizer | about 14,600 |

### The hard part

The hard part is not the GPU kernels. It is exact Spark semantics on the GPU, in each task:

- ANSI overflow errors, with the messages of Spark;
- the decimal types and the cast table of Spark;
- aggregate buffers that work with CPU partial aggregates across exchanges.

A stage must give the same answer on the GPU and on the CPU.

The second hard part is the 16-core CPU side. Shuffle, object-storage I/O and broadcast decode run
on 16 cores, but the CPU baseline has 128.

## Appendix A. Glossary

Each term is defined in the section given.

| term | meaning | section |
|---|---|--:|
| go threshold | the GPU option must cost at least 2× less per query than Comet on CPU | 1 |
| parity | the GPU side and the CPU side cost the same per hour | 2 |
| layout | one way to buy CPU nodes for the price of the H100 node: 1 × 128, 8 × 16 or 16 × 8 | 2 |
| cost ratio | CPU cost per query divided by GPU cost per query; at parity, CPU time divided by GPU time | 2 |
| sequential baseline | Comet runs the queries one after another | 2 |
| concurrent baseline | Comet runs the queries at the same time in one application; the faster baseline at the input values | 2 |
| scale factor (sf) | the TPC size parameter | 3 |
| batch | a set of queries that run in the same time window | 3 |
| shared scan | the GPU node reads each column once for all queries of a batch | 3 |
| cut | the bytes read with each query alone, divided by the bytes read with the shared scan | 3 |
| one-pass | one query runs alone, with no sharing | 3 |
| wire | the compressed Parquet bytes that a query or a batch reads | 4 |
| tier | the place that the GPU node reads its data from | 4 |
| decode rate | how fast the GPU turns compressed Parquet into columns | 4 |
| sharing factor | GPU time with each query alone, divided by GPU time with the shared scan | 4 |
| device factor | CPU time divided by GPU time with each query alone | 4 |
| range | the lowest and highest cost ratio over all combinations of the inputs' low and high values | 5 |
| sensitivity table | the inputs of one result, in order of their effect on it | 5 |
| settling measurement | the measurement of the input in the top row of a sensitivity table | 5 |
| conditional go | the range includes 2 | 5 |
| decision case | one of the four results to which the decision rule applies | 5 |
| native block | a maximal native subtree of a Comet plan: it cannot grow without crossing a JVM boundary | 8 |
| insertion point | `PhysicalPlanner::create_plan`, where a GPU executor can fit | 8 |

## Appendix B. Versions and method

Comet 1.2.0, tag commit `ab46419fcc99bcd34aa4d89f7b545d5d51694a88`, was released on 2026-11-03.
It runs on DataFusion 55.2.0, arrow-rs 59.2.0, Arrow Java 18.3.0 and Spark 4.1.3.

The CPU VM ran this software:

- OpenJDK 17.0.20.1;
- the Comet jar `comet-spark-spark4.1_2.13-1.2.0.jar` (98,985,596 B);
- hadoop-aws 3.4.2 and the AWS SDK bundle 2.29.52;
- Python 3.12.3, pyarrow 26.0.1 and boto3 1.43.123;
- Ubuntu 24.04.5.

peacockdb plans on DataFusion 45 and arrow-rs 54.2 (`peacockdb-core/Cargo.toml`).

The work had five parts:

1. Desk research by five subagents, with one findings file each:
   [`comet-internals.md`](../artifacts/comet-research/comet-internals.md), [`prior-art.md`](../artifacts/comet-research/prior-art.md),
   [`data-path.md`](../artifacts/comet-research/data-path.md), [`batching.md`](../artifacts/comet-research/batching.md) and
   [`exotic.md`](../artifacts/comet-research/exotic.md).
2. Code facts from local clones at fixed commits: Comet `ab46419f`, spark-rapids (now
   `NVIDIA/cudf-spark`) `ce634969`, Gluten `d64a51df` and Auron `cdd73ecf`.
3. Experiments 1 to 3 (section 6).
4. Analysis: [`translatability.md`](../artifacts/comet-research/translatability.md), [`split.md`](../artifacts/comet-research/split.md), the
   tested cost model and [`design.md`](../artifacts/comet-research/design.md).
5. An audit: a new subagent checked the report against the artifacts
   ([`audit.md`](../artifacts/comet-research/audit.md)).

## Appendix C. Changes to the Comet configuration

The base is the TPC configuration of Comet. The single-executor changes are in
[`comet-internals.md`](../artifacts/comet-research/comet-internals.md) "Recipes" 3:

- one executor on 128 cores, a 16g driver, a 64g heap, a 48g overhead and 256 shuffle partitions;
- the AWS profile credentials provider, which the native S3 reader of Comet requires.

These changes were added on top of that:

- `spark.memory.offHeap.size=256g`. The recipe said 320g. 256g is 2 GB per core, the ratio of
  Comet, and it leaves room in RAM for the tmpfs.
- `spark.cleaner.periodicGC.interval=5min`, so Spark removes shuffle files on tmpfs between
  queries.
- The local directories of Spark (shuffle and spill) on tmpfs, not on disk.
- `spark.comet.explain.fallback.enabled` and the explain provider of Comet in every run, to list
  the fallbacks.
- A q1 warm-up only in the TPC-H S3 sequential run. No other run had a warm-up query.
- The two S3 sequential runs also saved plans. Appendix D explains the effect.

## Appendix D. VM hours, failures and the audit

The run log is [`vm-log.tsv`](../artifacts/comet-research/measurements/vm-log.tsv). Each row is one VM.

| VM | limit | hours by the log | wall time, request to stop | sessions (2026-11-03 PST) |
|---|--:|--:|--:|---|
| l40s (`gpu-l40s-a`, eu-north1) | 1 h | 0.33 h | 0.38 h | dead-man check 19:07:47-19:13:39 PST (0.10 h); Experiment 1 19:15:47-19:32:23 PST (0.28 h) |
| cpu128 (`cpu-d3`, 128 vCPU, eu-west1) | 2 h | 0.66 h | 0.66 h | setup, Experiment 1, calibration, nine configurations, bytes: 19:54:22-20:33:49 PST |
| H100 | none | 0 | 0 | not used, by the decision of the human |

- The log shows too little time for the first l40s session. Its dead-man poweroff closed the
  session at the last heartbeat (19:10:47 PST). The VM ran until 19:13:39 PST. The wall time
  includes this.
- All nine configurations ran, in the priority order of the spec. They were TPC-H local
  sequential (2 runs) and concurrent "all" (2 runs), and TPC-DS local sequential and concurrent
  "all" (1 run each).
- The others were TPC-H S3 sequential and concurrent "all", TPC-DS S3 sequential, and the two runs
  with the Comet cache (TPC-H local, lowest priority). No configuration was skipped.
  TPC-DS concurrent "all" did not run out of memory, so no fallback to batch 20 was necessary.
- The S3 runs have the same plans as the local runs. For TPC-H they are identical. For TPC-DS, only
  the AQE operator counts of q31 and q39 and one expression ID in q35 differ.
- The first setup failure: systemd-logind's `RemoveIPC` deleted the data in `/dev/shm` when
  the last ssh session ended. A dedicated tmpfs at `/mnt/comet` replaced it.
- The second setup failure: `comet_setup.sh cluster` reported "cluster up" when no daemon had
  started. Both failures were fixed in the session.
- The audit found a third bug. The sequential batch timer of `comet_run.py` also counted the time
  of `--dump-plans`. The two S3 sequential runs and the two calibration passes used that flag.
- The plan dumps took 10.7 s for TPC-H and 69.7 s for TPC-DS. Each per-query time excludes its
  dump.
- So the model now uses the sum of the per-query times for those two S3 batches. The script is
  fixed and has a regression test. No run was repeated.
- This changed the TPC-DS decision cases from 2.14 and 1.58 to 2.06 and 1.44. The TPC-H values did
  not change.
- Data: sf40 Parquet was copied from eu-north1 (TPC-H 42.4 GB at 0.57 GB/s, TPC-DS 15.8 GB at 0.45
  GB/s). It was uploaded to the eu-west1 buckets at 1.17 to 1.50 GB/s.
- The size of each object was checked. The buckets stay. The human decides when to empty them.
- All VMs used are stopped.

## Appendix E. The scalar inputs

From [`inputs.tsv`](../artifacts/comet-research/inputs.tsv). The per-query tables (Comet times, scan and shuffle bytes,
driver and scheduler time, footer bytes) are `measured`. The eligible fractions are
`modelled`. Each row below is one scalar input; a blank low and high means that the input has
no span.

| input | value | low | high | unit | tag | source |
|---|--:|--:|--:|---|---|---|
| `price_h100_node` | 4.50 | | | $/h | published | [Nebius prices](https://nebius.com/prices) |
| `rate_h100_s3` | 1.25 | 0.917 | 12.5 | GB/s | modelled | assumed 10-100 Gbit/s; the low value is the measured S3 read of l40s |
| `rate_cpu128_s3` | 1.164 | | | GB/s | measured | cpu128, eu-west1, same region, 4 threads, 2026-11-03 20:01 PST |
| `rate_node16_s3` | 1.0 | 0.78 | 4.0 | GB/s per node | modelled | assumed 6.25-32 Gbit/s |
| `rate_node8_s3` | 0.917 | 0.39 | 2.0 | GB/s per node | modelled | assumed 3.125-16 Gbit/s; the value is the measured read of the 8-vCPU l40s |
| `bucket_cap` | 20 | | | GB/s | published | [Nebius storage classes](https://docs.nebius.com/object-storage/storage-classes) |
| `s3_efficiency` | 0.892 | 0.560 | 1.0 | fraction | measured | TPC-H S3 concurrent: 48.3 GB in 46.5 s, over 1.164 GB/s |
| `s3_efficiency_sequential` | 0.560 | 0.431 | 0.892 | fraction | measured | TPC-H S3 sequential: 48.3 GB in 74.1 s, the per-query sum |
| `decode_rate` | 4.1 | 2.34 | 18.8 | GB/s compressed | modelled | H200 load times of benchmark-minimal over footer bytes; low scaled by H100/H200 memory bandwidth (3.35/4.8); high is the chunked reader on GB10 |
| `gpu_speedup` | 10.1 | 3.4 | 28.3 | × | modelled | DuckDB warm time over H100 G=1 execute time, q1, q3, q6, q8 |
| `gpu_measured_scale` | 1.0 | 1.0 | 1.73 | × | modelled | 1.0 keeps the H100 G=1 times; high is H200 scaled by HBM rate |
| `shuffle_rate_gpu_node` | 4.0 | 2.0 | 20.0 | GB/s | modelled | estimate: lz4 and Arrow IPC on 16 cores; high is lz4 on the GPU |
| `ineligible_weight` | 1.0 | 0.5 | 2.0 | × | modelled | operator count stands in for time |
| `scheduler_share` | 0.25 | 0 | 1 | fraction | modelled | section 4 |
| `overhead_overlap` | 0.5 | 0.2 | 1.0 | fraction | modelled | estimate |
| `cpu_run_spread` | 1.0 | 0.881 | 1.119 | × | measured | TPC-H local sequential runs, 59.6 s and 46.9 s |
| `cluster_overhead` | 1.1 | 1.0 | 1.3 | × | modelled | estimate: k small nodes against one node |
| `s3_penalty_bandwidth_share` | 0.5 | 0 | 1 | exponent | modelled | estimate: the part of the S3 penalty that is bandwidth |
| `size_exponent` | 0.7 | 0.5 | 1.0 | exponent | modelled | Comet's published runs past sf40 (`data-path.md`) |
| `spill_factor` | 1.3 | 1.0 | 2.0 | × | modelled | estimate |
| `vram_gb`, `host_ram_gb`, `cores_gpu_node` | 80, 200, 16 | | | GB, GB, vCPU | published | Nebius `gpu-h100-sxm` `1gpu-16vcpu-200gb` |
| `cores_cpu`, `mem_cpu128_gb` | 128, 320 | | | vCPU, GB | measured | `conf.txt`: 128 cores; off-heap 256g plus heap 64g |
| `mem_node16_gb`, `mem_node8_gb` | 40, 20 | | | GB | modelled | estimate: executor memory on 64 GB and 32 GB nodes |
| `ram_cpu128_gb`, `ram_node16_gb`, `ram_node8_gb` | 512, 64, 32 | | | GB | published | Nebius `cpu-d3` presets |
| `cpu_ram_usable` | 0.5 | | | fraction | modelled | estimate: the RAM that can hold data next to Spark |
| `cpu128_s3_concurrent_all_s.tpcds` | 124.4 | 87.5 | 161.1 | s | modelled | section 5 |

## Appendix F. The sensitivity tables

From the sensitivity section of [`cost.md`](../artifacts/comet-research/cost.md). Each table is one decision case.
Each row moves one input to its low value and then to its high value. The rows are in order of
the change, largest first.

TPC-H sf40 (value 1.88):

| input | cost ratio at the low value | cost ratio at the high value | tag |
|---|--:|--:|---|
| `shuffle_rate_gpu_node` | 1.29 | 2.97 | modelled |
| `scheduler_share` | 2.14 | 1.47 | modelled |
| `rate_h100_s3` | 1.69 | 2.27 | modelled |
| `overhead_overlap` | 2.08 | 1.63 | modelled |
| `cpu_run_spread` | 1.68 | 2.09 | measured |
| `s3_efficiency` | 1.58 | 1.95 | measured |
| `s3_penalty_bandwidth_share` | 2.14 | 1.79 | modelled |
| `cluster_overhead` | 1.77 | 2.11 | modelled |

TPC-H sf200 (value 1.38):

| input | cost ratio at the low value | cost ratio at the high value | tag |
|---|--:|--:|---|
| `shuffle_rate_gpu_node` | 0.90 | 2.41 | modelled |
| `size_exponent` | 1.15 | 1.87 | modelled |
| `rate_h100_s3` | 1.21 | 1.88 | modelled |
| `scheduler_share` | 1.54 | 1.03 | modelled |
| `s3_penalty_bandwidth_share` | 1.67 | 1.26 | modelled |
| `rate_node16_s3` | 1.47 | 1.13 | modelled |
| `cpu_run_spread` | 1.22 | 1.53 | measured |
| `s3_efficiency` | 1.13 | 1.44 | measured |

TPC-DS sf40 (value 2.06):

| input | cost ratio at the low value | cost ratio at the high value | tag |
|---|--:|--:|---|
| `scheduler_share` | 2.72 | 1.53 | modelled |
| `overhead_overlap` | 2.63 | 1.52 | modelled |
| `shuffle_rate_gpu_node` | 1.64 | 2.60 | modelled |
| `s3_penalty_bandwidth_share` | 2.48 | 1.90 | modelled |
| `ineligible_weight` | 2.27 | 1.75 | modelled |
| `cpu128_s3_concurrent_all_s.tpcds` | 1.81 | 2.32 | modelled |
| `gpu_speedup` | 1.66 | 2.06 | modelled |
| `cpu_run_spread` | 1.93 | 2.20 | measured |

TPC-DS "1 TB" (value 1.44):

| input | cost ratio at the low value | cost ratio at the high value | tag |
|---|--:|--:|---|
| `size_exponent` | 1.11 | 2.55 | modelled |
| `shuffle_rate_gpu_node` | 0.96 | 2.40 | modelled |
| `s3_penalty_bandwidth_share` | 2.25 | 1.13 | modelled |
| `cpu128_s3_concurrent_all_s.tpcds` | 0.94 | 1.94 | modelled |
| `scheduler_share` | 1.80 | 0.93 | modelled |
| `rate_h100_s3` | 1.26 | 2.01 | modelled |
| `spill_factor` | 1.28 | 1.82 | modelled |
| `rate_node16_s3` | 1.57 | 1.09 | modelled |

## Appendix G. Follow-up experiments

The settling measurements come first.

1. Comet on 16 cores of the CPU VM, with no GPU. Run TPC-H and TPC-DS sf40, local and from S3.
   Measure the shuffle rate per core, the scheduling gaps per stage at 16 slots, and the 16-core
   time of the CPU part of the split. This settles three decision cases.
2. Comet at a second scale factor on the same VM. Fit time against size, instead of the published
   SF^0.5 to SF^0.9. This settles TPC-DS "1 TB" (`size_exponent`).
3. TPC-DS concurrent "all" from S3 on the CPU VM. This replaces the modelled CPU time of both
   TPC-DS decision cases.
4. S3 read rates on the H100 VM and on a 16-vCPU CPU VM. These replace the assumed NIC rates. Use
   several processes or a native client, to tell a NIC limit from a client limit.
5. A simulation of passive sharing over the column chunks of the dumped plans, under FAIR
   scheduling. Measure the cache hit rate and the real cut at 10, 50 and 200 GB.
6. The loss of row-group pruning in a union of columns, for each query alone and in the batch.
7. A real ETL trace from the Spark event logs of a design partner. Measure the access sets and
   the overlap per window. This replaces the TPC and SCOPE factors.
8. A translator prototype for TPC-H q1 and q6 in a Comet fork, checked against Comet's answers in
   ANSI mode. Then one GH200 hour on the same queries.
9. CPU times per operator from Comet's native metrics, to replace the weight by operator count.
   Also the part of stage time in shuffle encode, shuffle decode and broadcast decode.
10. Concurrent peacockdb sessions on one GPU, at 1, 4 and 16 sessions.
11. The small-block threshold: one block at 1, 8, 64 and 512 MB per task. Get the block sizes per
    task from a patched `CometExecIterator`.
12. A plan census of the 17 TPC-DS queries left out, of real jobs, and of the next Comet release
    (to see changes in the protobuf).
13. The AWS G7 TPC-DS 3 TB study again, with Comet 1.2.0 as the CPU side. Also Algeon against
    DataFusion 55 on the H100 VM and the CPU VM.
14. The Nebius shared filesystem from the 1-GPU H100 VM, with fio and then KvikIO. Also GH200 S3
    reads, and p5.4xlarge S3 at 100 Gbit/s and its NVMe.

## Appendix H. Sources

In the repository, at commit `ad978c24`:

- the source report: [`comet-gpu.md`](https://github.com/asymptote-tech/peacockdb/blob/ad978c2411759201624553c5fe6327488b21cdb0/llm-wiki/reports/comet-gpu.md);
- the task spec: [`comet-research.md`](../tasks/comet-research.md) and its run state
  [`comet-research-detail.md`](../tasks/comet-research-detail.md);
- the artifacts in [`artifacts/comet-research/`](../artifacts/comet-research); each one lists its own sources with dates;
- the measurements of 2026-11-03 PST, hosts l40s and cpu128: [`measurements/s3/`](../artifacts/comet-research/measurements/s3/notes.md),
  [`measurements/comet/`](../artifacts/comet-research/measurements/comet/summary.tsv),
  [`measurements/footers/sf40.tsv`](../artifacts/comet-research/measurements/footers/sf40.tsv) (read 20:44 PST),
  [`plans/`](../artifacts/comet-research/plans) and [`vm-log.tsv`](../artifacts/comet-research/measurements/vm-log.tsv);
- the scripts and their tests: [`scripts/`](../artifacts/comet-research/scripts);
- [`benchmark-minimal.md`](benchmark-minimal.md) (H100 and H200 sf40 times),
  [`dgx-spark.md`](dgx-spark.md), `artifacts/product-research/inventory.md`, and
  `artifacts/product-research/options/spark-plugin.md` (leads only).

Code, cloned at fixed commits:

- Comet 1.2.0: https://github.com/apache/datafusion-comet/tree/ab46419fcc99bcd34aa4d89f7b545d5d51694a88
- spark-rapids `ce6349699ccb3bd527c223c66ba24bd708e67e18`: https://github.com/NVIDIA/cudf-spark
- Gluten `d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f`: https://github.com/apache/gluten
- Auron `cdd73ecfafa52a96a2602235930dc309edb9c019`: https://github.com/apache/auron
- Spark v4.1.3: https://github.com/apache/spark/tree/v4.1.3

Web sources, each read on 2026-11-04:

- Comet: [release 1.2.0](https://github.com/apache/datafusion-comet/releases/tag/1.2.0),
  [PR #7444](https://github.com/apache/datafusion-comet/pull/7444),
  [issue #7288](https://github.com/apache/datafusion-comet/issues/7288),
  [GPU issue search](https://github.com/apache/datafusion-comet/issues?q=GPU)
- [Auron PR #1001](https://github.com/apache/auron/pull/1001)
- Gluten: [GPU doc](https://github.com/apache/gluten/blob/d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f/docs/backends/velox/get-started/VeloxGPU.md),
  [IAA doc](https://gluten.apache.org/docs/v1.4.0/get-started/VeloxIAA.html)
- spark-rapids: [#15847](https://github.com/NVIDIA/cudf-spark/issues/15847);
  [NVIDIA FAQ](https://perspectives.nvidia.com/cudf/faq/we-want-to-add-a-gpu-execution-path-to-our-existing-query-engine-but-don-t-want-to-fork-the-mainline-6627a6/)
- [AWS EMR G7 blog](https://aws.amazon.com/blogs/big-data/gpu-accelerated-apache-spark-with-amazon-emr-and-nvidia-rtx-pro-4500-on-amazon-ec2-g7-instances-runs-up-to-3-7x-faster/)
- [Theseus](https://arxiv.org/html/2508.05029v1); [Presto with GPUs](https://arxiv.org/html/2606.24647v1);
  [Pavlo's 2025 retrospective](https://www.cs.cmu.edu/~pavlo/blog/2026/01/2025-databases-retrospective.html)
- Nebius: [VM types](https://docs.nebius.com/compute/virtual-machines/types.md),
  [prices](https://nebius.com/prices),
  [storage classes](https://docs.nebius.com/object-storage/storage-classes),
  [local disks](https://docs.nebius.com/compute/storage/local-disks)
- AWS: [storage-optimized specs](https://docs.aws.amazon.com/ec2/latest/instancetypes/so.html),
  [I4i launch blog](https://aws.amazon.com/blogs/aws/new-storage-optimized-amazon-ec2-instances-i4i-powered-by-intel-xeon-scalable-ice-lake-processors/),
  [instance store lifetime](https://docs.aws.amazon.com/AWSEC2/latest/UserGuide/instance-store-lifetime.html),
  [EMR file systems](https://docs.aws.amazon.com/emr/latest/ManagementGuide/emr-plan-file-systems.html)
- [GCP local SSD](https://docs.cloud.google.com/compute/docs/disks/local-ssd)
- GDS: [release notes](https://docs.nvidia.com/gpudirect-storage/release-notes/index.html),
  [O_DIRECT and compatibility mode](https://docs.nvidia.com/gpudirect-storage/o-direct-guide/index.html);
  [Blackwell decompression](https://arxiv.org/html/2512.02189)
- Caches: [Fabric intelligent cache](https://learn.microsoft.com/en-us/fabric/data-engineering/intelligent-cache),
  [Databricks disk cache](https://docs.databricks.com/aws/en/optimizations/disk-cache),
  [Meta RaptorX](https://hackernoon.com/avoiding-data-silos-in-presto-in-meta-from-raptor-to-raptorx),
  [Uber](https://www.uber.com/blog/securing-hadoop-on-gcp)
- Spark: [SharedState.scala](https://github.com/apache/spark/blob/master/sql/core/src/main/scala/org/apache/spark/sql/internal/SharedState.scala),
  [ReuseExchangeAndSubquery.scala](https://github.com/apache/spark/blob/77bbf77e86ad48f58b5dfbc6ac882b3e70cf1989/sql/core/src/main/scala/org/apache/spark/sql/execution/reuse/ReuseExchangeAndSubquery.scala#L28-L58),
  [Connect.scala](https://github.com/apache/spark/blob/77bbf77e86ad48f58b5dfbc6ac882b3e70cf1989/sql/connect/server/src/main/scala/org/apache/spark/sql/connect/config/Connect.scala#L209-L216)
- GH200: [arXiv 2407.07850](https://arxiv.org/html/2407.07850v1),
  [price list](https://www.amcompute.com/gpu-cloud-prices/gh200);
  [CXL from an H100](https://arxiv.org/html/2602.22457v1); [GDS report](https://gds-report.pages.dev/)
- Batching: [CloudViews, SIGMOD 2018](https://www.microsoft.com/en-us/research/wp-content/uploads/2018/03/cloudviews-sigmod2018.pdf),
  [EDBT 2021](https://edbt2021proceedings.github.io/docs/p165.pdf),
  [Johnson et al., VLDB 2007](https://www.vldb.org/conf/2007/papers/research/p351-johnson.pdf),
  [Psaroudakis et al., VLDB 2013](https://www.vldb.org/pvldb/vol6/p637-psaroudakis.pdf)
