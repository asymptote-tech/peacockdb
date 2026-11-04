# Should peacockdb become a GPU backend for Apache Comet, and how

Kind: prototype. Chain M's only task, run by the ensemble in workspace alpha on branch
`ENS-comet-research`, off master, alongside chain L (the human's decision, 2026-11-03). Unlike
other prototypes, it opens a PR against master when done, so the report can be merged. It gets
no reviewer rounds. The output is one report, plus the measurements and plan dumps it rests on.

## Why

product-research rated the Spark plugin "strong" on demand and "weak" on build cost and
platform risk ([`spark-plugin.md`](../artifacts/product-research/options/spark-plugin.md)). That
page sketched a shape: a native GPU engine behind Comet's Spark plan serializer. It was desk work,
and it left the hard questions open: how Comet's plan reaches an executor, where a GPU could take
part of it, and whether the network starves the GPU anyway.

The human decides two things from this report: **whether** to build a GPU backend for Comet, and
**how**, if the answer is yes. So the report states a go or no-go against the bar below. On a go,
it gives a design a builder could plan an alpha from.

Treat `spark-plugin.md` as leads to check, not as findings. Re-verify every fact it states that
this report relies on, at current versions.

## The bar for "go"

- **At least 2× lower cost per query than Comet on CPU** (the human's decision).
- **Price parity, as given by the human:** one H100 node (16 cores, 200 GB RAM) costs the same as
  one 128-core, 512 GB RAM node. Compare cost per query at that parity.
- **Main workload: batched ETL.** Many scheduled jobs read the same tables in one window, and no
  one waits on a single answer. Batching shares scans, which is where it should pay most (agreed
  with the human).
- **Floor: a single one-pass ETL job.** It is reported alongside. Nothing is shared in that case,
  so it shows the worst case.
- **Realistic deployment.** The human expects real clusters to use small CPU nodes (8 or 16 cores),
  each with its own NIC, not one 128-core box. The cost model compares one H100 node against that
  many small nodes at the same price, with each side's total NIC bandwidth counted. It also
  compares against the single 128-core box that is measured. Experiment 2 measures both a
  1 × 128-core and an 8 × 16-core layout on that box.
- **Small nodes are priced linearly by cores** (helper's call): the H100 node buys eight 16-core
  or sixteen 8-core nodes. Each small node gets the NIC rate the provider states for that size.
  The H100 node gets its own stated rate. Where a provider states none, the figure is an
  assumption and marked as one.
- **Cost per query is VM-hours at that parity** (helper's call). S3 request and egress charges,
  and idle time between scheduled windows, are listed beside it, not folded in.
- **Data size is a variable** (helper's call). sf40 fits in the H100 VM's RAM, and real ETL tables
  often do not. The ratio is reported as a function of data size, marking where the data stops
  fitting in VRAM, in host RAM, and on local NVMe.
- **A modelled verdict is acceptable** (the human: no peacockdb runs). The existing GPU numbers
  are a few TPC-H queries at sf40 on an H200, so most GPU-side times are modelled. Each input is
  marked measured, published or modelled, and the ratio carries an error band. A verdict whose
  band straddles 2× is a **conditional go** that names the measurement that would settle it.
- **How realistic batching is.** Microsoft's work on overlap between production jobs (SCOPE:
  CloudViews, BigSubs, and their successors) is the guide. Use it to say how much real ETL
  pipelines share, and how well batches of TPC-H/TPC-DS queries stand in for them.

## The questions

Each group is answered in the report, with sources.

### 1. Prior art: GPU and hardware acceleration on Comet

- Do any plugins, forks, branches or proposals bring GPU or other hardware acceleration to Comet?
  Check Comet's issues, PRs, mailing list and talks.
- What about Spark accelerators in general, and how each feeds its device? Cover at least
  spark-rapids, Speedata, Gluten (Velox and Velox-cuDF), Auron (formerly Blaze, DataFusion-based)
  and Photon. Add any others discovery finds, such as FPGA and DPU efforts or NeuroBlade.

### 2. Comet's infrastructure

- Is it DataFusion-based, and on which DataFusion, Arrow and Spark versions? Pin a Comet release
  and commit.
- Where is the plan decided? Which Spark rules or extensions rewrite the physical plan, what does
  the driver decide, and what falls back to Spark?
- How does the plan reach the executors? What is the IR (protobuf, and its schema), its unit (a
  stage, a task), and how are expressions and partition-specific data carried?
- How do executors load data (Parquet scan paths, object store, the JVM and native split),
  shuffle (Comet's native and columnar shuffles, and their formats), and write results back?
- How does memory work: off-heap, unified memory manager, spill?

### 3. Injecting a GPU executor

- Can a GPU executor be added inside this system? It would take a Comet plan fragment, translate
  it through peacockdb's DataFusion-side translator into a GPU plan, and run it on the slice of
  data that reaches that executor. Name the exact seam: where in Comet it hooks in, what it
  receives, and what it returns (Arrow over the C Data interface or otherwise).
- **Are Comet plans translatable?** Dump Comet's serialized plans for TPC-H q1-q22 and a TPC-DS
  sample. Classify each operator and expression against peacockdb's node vocabulary and
  expressions (`plan/mod.rs`, `architecture.md`): supported, adaptable, missing. Count them.
  peacockdb plans on DataFusion 45 and Comet on a later version; say what that gap costs.
- **Splitting the plan.** What runs on the CPU executor and what on the GPU executor? Consider
  scan and decode, filters and projections, joins and aggregates, shuffle write and read, and
  Spark-exact expression semantics. State a rule a planner could apply, and how fallback works
  inside one stage.
- How do GPUs map onto Spark executors and tasks? Spark has one or more tasks per executor;
  peacockdb assumes one GPU per process.

### 4. The data path: NIC, PCIe, RAM

- **The NIC.** A 40 Gbit/s NIC delivers about 5 GB/s. PCIe moves a few tens of GB/s, and the GPU
  works through data far faster than either. At what NIC bandwidth does the GPU stall, per query
  shape? Measure what the VMs actually get from S3 (see Experiments).
- **Getting more out of the wire.** Compressed transfer with GPU decompression, column and
  row-group pruning before the wire, local NVMe as a cache, and shuffle volume. What does each buy?
- **Caching in RAM.** Is it possible inside Comet and Spark? Cloud GPU machines have little RAM
  (the H100 VM here has about 200 GB). Survey cloud GPU instances' RAM, NIC and local NVMe
  against their price. Is the plugin only viable on-prem with custom hardware?
- **Data on locally attached SSD** (the human's question; desk only, since Nebius offers no
  local NVMe). Does instance-attached NVMe give more throughput than the NIC, and by how much?
  Survey the instances that offer it (AWS instance store on GPU and storage types, GCP local
  SSD, Azure L- and ND-series): drives, per-drive and aggregate read rates, capacity, price, and
  whether GPUDirect Storage reads it. Model the layout: tables staged or cached on local NVMe,
  read straight to the GPU. Say what its capacity and its loss on VM stop cost.
- **Local SSD as a product option** (the human's request). Do people actually run Spark or
  Comet with data on local SSD, instead of HDFS, S3 or Iceberg on object storage? Separate
  three layouts, with evidence for each (threads, vendor docs, case studies, with dates):
  - local SSD as the primary store (rare in the cloud, is it real anywhere?);
  - HDFS co-located on the compute nodes' own disks, on-prem or on cloud instance storage;
  - local NVMe as a cache in front of the lake (Databricks disk cache, Alluxio, Gluten's local
    cache, spark-rapids' file cache, and others).

  Say which layout a GPU backend should target first, and who runs it today.
- **How others handle it:** spark-rapids (shuffle over UCX, the reader, file cache), Speedata,
  Gluten-cuDF, and any other accelerator that publishes its data path.
- **Exotic architectures** (the human's request; desk only). What feeds the GPU faster than a
  40 Gbit/s NIC? For each: how it works, bandwidth and latency to the GPU, capacity, who sells
  or rents it and at what price, whether Spark or Comet runs on it today, and what the plugin
  would need to use it. Cover at least:
  - InfiniBand or RoCE RDMA to a memory server (Kove:SDM, GPUDirect RDMA from remote RAM, the
    research far-memory systems);
  - CXL memory pools and switches;
  - Grace superchips: GH200's NVLink-C2C to LPDDR, GB200 NVL72's rack-wide memory (GB10
    measurements in [`dgx-spark.md`](../reports/dgx-spark.md));
  - RDMA storage feeding GPUs through GPUDirect Storage (VAST, WEKA, DDN);
  - cloud GPU nodes with InfiniBand (Nebius, AWS EFA, Azure ND, CoreWeave), and whether Spark's
    shuffle and scans can use the fabric.

  The cost model treats each as a tier the data could arrive over instead of the NIC.

### 5. Batched query execution on Comet

query-fuser (chain L) prototypes fusing N queries into one DAG that reads each table once. It
also chooses, for each probe side held behind a join's build, between buffering it and reading it
twice. That choice has a cost model.

- **The API.** How would a user, or a scheduler, submit a batch of queries to Spark with Comet as
  one unit? Consider multi-sink plans, a Thrift or Connect endpoint, a planner extension that
  collects queries over a window, and what Spark already shares (exchange and subquery reuse,
  cached relations).
- **Whole-batch CBO.** Is cost-based optimization over the whole batch doable with Comet? That
  covers probe-side holds, buffer against re-read, and scan sharing. Where would that optimizer
  sit relative to Catalyst, AQE and Comet's serializer, and what statistics would it have?
- **Prior art.** Which systems do multi-query optimization or shared scans, and with what results?
  Examples: the MQO literature, QPipe, CJOIN, SharedDB, Crescando, MQJoin, Microsoft's SCOPE
  reuse work, and anything in the Spark ecosystem.
- **The NIC under batching.** Batching raises GPU utilization, but the NIC stays the limit. How far
  does sharing scans cut the bytes over the wire per query? Model it for batch sizes 3, 10, 20 and
  all, as query-fuser uses.

### 6. The verdict and the design

- Go or no-go against the bar, with the cost-per-query model written down. Inputs: measured,
  published or estimated, each marked as which.
- On a go: an alpha design. It covers the seam into Comet, the IR and translation, the CPU/GPU
  split rule, the data path and cache, the batching API and its optimizer. Size it by components
  (reuse, adapt, new), as `spark-plugin.md` did, and name the hard part.
- On a no-go: what would have to change for a go, such as hardware, prices or workload.

## Experiments

The task may run things. It does **not** run peacockdb's executor (the human's decision); GPU-side
times come from existing measurements and models.

**Hosts** (the human's list): the Nebius H100 and L40S VMs, and the 128-core CPU VM in eu-west1
(`nebius-cpu128`; ids and profiles in [`new-hosts.md`](new-hosts.md)).

- **Access before new-hosts lands** (the human's decision). The L40S goes through
  `scripts/gpu-host.sh`. The H100 and the CPU VM have no script, idle watcher or data on master
  yet, so the chain drives the `nebius` CLI directly with the instance ids and profiles in
  `new-hosts.md`. Never open a Nebius credential file; learn the CLI from `nebius … --help`.
- **The chain stops every VM it started**, at the end of each measurement session and before
  it exits for any reason. **It never stops a VM it found running** (helper's call): that may be
  chain L's run. With no idle watcher on the H100 and the CPU VM, nothing else stops them.
- **Conflict with chain L is accepted** (the human's decision). If chain L is running, it may
  stop a VM mid-measurement, and this chain's runs may disturb its H100 timings. A run cut
  short is re-run and recorded in the detail file.
- **S3 read access** is the default AWS profile, on the VMs and locally (the human's decision),
  with the endpoint `https://storage.eu-north1.nebius.cloud:443`. No credential file is read
  and no key is printed into a transcript.
- **VM-hour caps** (the human's decision): the CPU VM 6 h, the H100 1 h, the L40S 1 h, summed
  over the task. Experiments run in priority order. When a cap is reached, the remaining
  configurations are recorded as skipped, never left half-run. There is no mid-task
  checkpoint with the human; the caps take its place.

1. **VM-to-S3 throughput** on each VM: single and parallel streams, against the dataset bucket
   (eu-north1), and against the VM's local disk. Also the VM's NIC rate as the provider states
   it. The CPU VM reads the bucket across regions. Its rate is reported as measured, and the
   cost model uses an in-region rate taken from the provider's figures or the GPU VMs'
   measurements, marked as an assumption (helper's call). No new bucket is created.
2. **Comet on CPU, on the 128-core VM.** TPC-H sf40 (`tpch-sf40`) and TPC-DS sf40
   (`tpcds-sf40`), read from S3 and from local disk. That shows how far the NIC already limits a
   CPU engine. The human expects it to. Every run uses all 128 cores (the human's decision), in
   two layouts:
   - **1 × 128:** one Spark executor with all the cores;
   - **8 × 16:** eight executors of 16 cores each, each pinned to its own cores. It stands in
     for a cluster of small nodes. Its shuffle goes over loopback and all eight share one NIC,
     so the report says what it does not capture.

   For each batch (sizes 3, 10, 20 and all), run the queries **one after another, and
   concurrently** (the human's decision). Spark's own exchange and subquery reuse stays at its
   defaults. The report quotes the ratio against both. Priority order, for the cap:
   1. TPC-H, both layouts, local disk;
   2. TPC-H, both layouts, S3;
   3. TPC-DS, 1 × 128, local disk and S3;
   4. TPC-DS, 8 × 16, local disk and S3.
3. **Comet plan dumps** for the translatability count in group 3, on whichever host runs Comet.
4. **GPU side, modelled, not run.** Use peacockdb's existing sf40 numbers
   ([`benchmark-minimal.md`](../reports/benchmark-minimal.md)), the measured PCIe and decode rates in
   [`inventory.md`](../artifacts/product-research/inventory.md), and the NIC numbers from item 1.
   Batching gains follow from expected PCIe throughput and query-fuser's design. Chain L's runs do
   not exist yet, and this task does not wait for them (the human's decision).

Anything that would need a peacockdb run or a bigger build to settle goes in the report as a
follow-up experiment.

## Method

The plan sets the phases, in this order of dependence. The developer runs them and may
dispatch its own subagents; the coordinator owns every git write.

1. Desk research on groups 1, 2, 4 (the survey parts, the exotic architectures included) and 5
   (prior art), as parallel subagents. Use `anthropic-skills:deep-research` where the question
   is open-ended.
2. Experiments 1-3, in the foreground, under the caps.
3. Analysis: plan translatability, the split rule, the data-path model, the batching API and CBO.
4. Synthesis into the report, and an audit of it by a fresh subagent. The audit
   is the task's independent check, in place of reviewer rounds (helper's call).

**Intermediate artifacts go in `llm-wiki/artifacts/comet-research/`.** That covers one findings
file per subagent, the plan dumps, raw measurements and the scripts that produced them, and the
cost model's inputs. Subagents write their findings there and return a short summary and the
path. `comet-research-detail.md` holds only run state: which phases are done, VM starts and
stops with local times, VM hours used against each cap, and what a restarted coordinator needs
to resume.

## The report

One file: `llm-wiki/reports/comet-gpu.md`. It opens with the verdict, go or no-go, in a few
lines, with the cost-per-query ratio for batched ETL and for the one-pass floor. Then:

- the cost model and its inputs;
- a section per question group, 1-5, the exotic architectures included;
- the design (on a go) or the conditions for a go (on a no-go);
- follow-up experiments;
- sources.

Every factual claim carries a source link and the date accessed. Measurements give host, date
(local time) and method. Estimates and guesses are marked as such.

## Scope

- **Code:** none in the engine. No file under `peacockdb-core/`, `peacockdb-ffi/`, `cpp/`,
  `flatbuffers/` or `docker/` changes. Scripts that drive the experiments live in the artifacts
  folder, not under `scripts/`.
- **Component-level API:** none.
- **Files written:** `llm-wiki/reports/comet-gpu.md`; `llm-wiki/artifacts/comet-research/`, a new
  folder; this task's `-impl.md` and `-detail.md` while it runs; and chain M's section of
  `llm-wiki/tasks/tasks.md`. The artifacts stay after the task is archived, because the report
  links to them.

## Constraints

- **Paid VMs.** Start a VM only for a measurement session and stop it when the session ends. Each
  start and stop goes in the detail file with local times.
- **VM sessions run in the foreground.** A backgrounded build-and-run chain gets killed mid-run
  with no error.
- Comet, Spark and DataFusion versions are pinned in the report. Comet is built or fetched on the
  VMs, never vendored into the repo.

## Done when

The report is on `ENS-comet-research`, answers question groups 1-6 to the bar above, with
experiments 1-3 run, skipped under a cap, or their failure recorded. The audit's findings are
fixed, every VM the chain started is stopped, and a PR against master is open. The human merges
it after reading the report.
