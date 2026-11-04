# comet-research — run state

## Coordinator

- 2026-11-04: run started on `ENS-comet-research`; state `building`. The developer is dispatched
  one plan task at a time (`comet-research-impl.md`); each checkpoint is committed by the
  coordinator with the plan's message.
- Tasks 0 and 1 committed (f4e2767d, 5330543d). Control file said `pause` after the desk wave;
  next dispatch is Task 2 once it clears.

## Tasks

- Task 0 (set up the run): done, committed.
- Task 1 (desk wave): done, checkpoint `comet-research: desk wave` pending.
- Task 2 (experiment scripts): todo
- Task 3 (experiment 1, VM-to-S3 throughput): todo
- Task 4 (experiments 2 and 3, Comet on the CPU VM, plan dumps): todo
- Task 5 (analysis and the cost model): todo
- Task 6 (the report and its audit): todo

Plan drift (fixed by the coordinator, 2026-11-04): Global Constraints and Review Focus in
`comet-research-impl.md` were renumbered to match the headings, Task 0 to Task 6.

## Clones

Directory: `~/comet-research/ext/` on the local host (outside every worktree). Blobless clones
(`--filter=blob:none`, full history and tags), made 2026-11-04.

| Repo | Path | Ref | HEAD SHA |
|---|---|---|---|
| apache/datafusion-comet | `~/comet-research/ext/datafusion-comet` | tag `1.2.0` (detached) | `ab46419fcc99bcd34aa4d89f7b545d5d51694a88` |
| NVIDIA/cudf-spark (was NVIDIA/spark-rapids) | `~/comet-research/ext/cudf-spark` | `main` | `ce6349699ccb3bd527c223c66ba24bd708e67e18` |
| apache/gluten (was apache/incubator-gluten) | `~/comet-research/ext/gluten` | `main` | `d64a51dfd01bbe1b8e4adb6c1f74331fb75f531f` |
| apache/auron | `~/comet-research/ext/auron` | `master` | `cdd73ecfafa52a96a2602235930dc309edb9c019` |

`NVIDIA/spark-rapids` and `apache/incubator-gluten` now redirect to the names above (GitHub
API, 2026-11-04).

### Comet pin

- Tag `1.2.0`, the latest release (published 2026-11-03; `1.1.0` was 2026-10-06). Tag commit
  `ab46419fcc99bcd34aa4d89f7b545d5d51694a88`, 2026-10-30, on `branch-1.2`.
- Spark (`pom.xml`): default build 4.1.3 (Scala 2.13, Java 17). Profiles: `spark-3.4` 3.4.3,
  `spark-3.5` 3.5.9, `spark-4.0` 4.0.4, `spark-4.1` 4.1.3, `spark-4.2` 4.2.0.
- Arrow Java (`pom.xml` `arrow.version`): 18.3.0.
- DataFusion (`native/Cargo.toml`): 55.2.0 (`datafusion-spark` 55.2.0 too).
- Arrow Rust / parquet (`native/Cargo.toml`): 59.2.0. `object_store` 0.13.2.

## Task 1: desk wave replies

Dispatched 2026-11-04 as five parallel `Explore` subagents, not `general-purpose`: the developer's
agent definition allows the Agent tool for Explore only. Explore cannot write files, so each
returned its file's full text and the developer wrote it to disk (HTML entities unescaped, light
wording edits, no change of substance).

### comet-internals (`artifacts/comet-research/comet-internals.md`)
- Conditional go on feasibility, no plug-in point. The clean seam is native and per Spark task:
  protobuf plan bytes in, Arrow batches out over the C Data interface. Comet has no extension
  point for a second engine, so a GPU backend means forking Comet's native crate (or replacing
  its JNI path), per task, at DataFusion 55.2.
- Pin confirmed: 1.2.0 = ab46419f; DF 55.2.0, Arrow-rs 59.2.0, Spark 4.1.3 default, Arrow Java
  18.3.0. peacockdb is on DF 45 / Arrow 54.2.
- Against: one native plan per task over one split or shuffle partition, 8192-row batches;
  shuffle codec and broadcast build are CPU and per task; GPU memory invisible to Spark;
  partial/final aggregate state layout must match; Spark-exact kernels are CPU-only Rust.
- Plan dumps: no `spark.comet.*` config writes the protobuf. A driver-side spark-shell script
  collects one blob per native block; per-task bytes differ only by injected file lists,
  shuffle-writer root and output paths. Developer spot-checked the recipe's API at ab46419f
  (`CometNativeExec.serializedPlanOpt`, `nativeOp` public; `SerializedPlan.isDefined`;
  `collectWithSubqueries` used in Comet tests; "Comet native query plan" log at jni_api.rs:1558).

### prior-art (`artifacts/comet-research/prior-art.md`)
- No prior art: nobody has built, forked or proposed a GPU or hardware backend for Comet. The
  one DataFusion-based Spark GPU attempt (Auron PR #1001, libcudf) stopped after scan, filter
  and project and was closed stale (2025-12-05).
- Comet: no GPU issues/PRs/discussions; 1.2.0 README says "no GPUs, FPGAs". Nearest seam is the
  Native Executors epic #7288 (proto bytes + Arrow streams in, Arrow stream out).
- Published GPU-vs-native-CPU cost gaps run 1.1x-2.9x, straddling 2x; vs vanilla Spark the best
  recent AWS figure is 1.4x cheaper. No study compares a GPU with Comet; vs Comet a GPU likely
  loses or breaks even (estimate).
- Scan and I/O decide the result, not compute (Parquet I/O 60-70% of Presto-GPU runtime; a
  custom S3 reader cut Theseus' cloud runtime 75%).
- Against: Voltron Data (Theseus) shut down; NVIDIA dropped its CPU-scan hybrid; Gluten dropped
  Intel IAA. Settling measurement: rerun AWS's G7 TPC-DS 3 TB test with Comet 1.2.0 as the CPU side.

### data-path (`artifacts/comet-research/data-path.md`)
- The wire sets the speed, not the GPU. At price parity, small CPU nodes buy as much or more
  NIC per dollar than single-GPU VMs: about 0.5-1.9x on AWS, 3x on Azure, 18x on GCP. Local
  NVMe on single-GPU VMs is no faster than the NIC, or unpublished.
- One-pass ETL from S3 should not clear 2x on the data path alone (estimate). Batched ETL can,
  only when the shared scan is served from pinned host RAM or VRAM: conditional go.
- Corrections to leads: Nebius now sells local NVMe, but only on its 8-GPU B300 VM (none for
  H100, L40S or CPU VMs). GDS on cloud VMs is "compatibility mode" (copies through host RAM), so
  NVMe-straight-to-GPU is bare-metal only.
- Today peacockdb's reader (about 3.8 GB/s compressed on q6) is slower than a 40 Gbit/s NIC.
- Target first: node-level cache (host RAM, then NVMe) in front of the lake, as Databricks,
  Fabric, Meta and Uber run it.

### exotic (`artifacts/comet-research/exotic.md`)
- None of these lifts the cold one-pass path in the cloud. Rentable sources top out at about
  12-16 GB/s per GPU (p5.4xlarge 100 Gbit, Nebius shared FS). The big rates (GH200 375 GB/s,
  NVLink 900 GB/s, peer RAM over IB ~24 GB/s) are caches: they only help batched re-reads.
- Faster sources (GDS on VAST/WEKA/DDN, 22-47 GB/s per GPU; S3 over RDMA) are on-prem; Spark
  and Comet cannot use them today; the plugin would need its own cuFile reader.
- CXL and memory servers fail: no GPU reads CXL directly (~20 GB/s measured, below host RAM over
  PCIe); Kove publishes no GPU path, bandwidth or price; far-memory systems are CPU prototypes.
- Cloud fabrics do not fit price parity: InfiniBand only on 8-GPU nodes (Nebius 1-GPU cannot
  join); Comet shuffles over Spark TCP; only spark-rapids' non-default UCX shuffle uses RDMA.
- Cheapest lever: GH200 at $1.99-2.29/h (about half Nebius H100's $4.50), 375 GB/s to 480 GB
  Grace memory, NIC/S3 rate unknown. Tier table (TSV, 23 rows) is at the end of the file.

### batching (`artifacts/comet-research/batching.md`)
- Batching cuts wire bytes per query usefully at large batches, probably not enough alone to
  clear 2x. Spark and Comet have no way to submit or run a batch as one unit today.
- TPC-H sf40 column-level model from repo sf1 footers x40 (estimate): cut 1.4x (3), 4.5x (10),
  5.8x (20), 6.2x (all 22), numeric order; TPC-DS 1.0x, 1.6x, 2.3x, 7.5x.
- SCOPE production (CloudViews, EDBT 2021 Table 1): data read -38.84% (~1.6x), processing
  -38.96%. TPC "all" batches likely overstate sharing about 4x (estimate).
- Spark shares only inside one query (exchange/subquery reuse, MergeSubplans); across queries
  only via explicit cache. Comet 1.2.0 keeps exchange reuse; one root per native block; its
  Arrow cache is off by default. A whole-batch optimizer must sit above Catalyst (footers +
  run history). Cheapest form on one GPU node: executor-level shared column cache, no new API.
- Against: small batches share little; unioned scans lose row-group pruning; sharing costs at
  low concurrency; a CPU engine could share scans the same way. With 8 small nodes at equal
  NIC each, the GPU node needs an 8x cut just to tie on the wire.

## VM hours

## Rulings

- 2026-11-04 (coordinator): the desk wave proposed two additions. An iperf3 run between VMs is
  allowed in Task 3 if it fits the caps. A Comet run with
  `spark.comet.exec.inMemoryCache.enabled=true`, as a fairer batching baseline, goes after the
  spec's whole matrix in Task 4, lowest priority, and is skipped under the cap like any other
  configuration. The spec's matrix keeps Spark defaults.
