# comet-research — run state

## Coordinator

- 2026-11-04: run started on `ENS-comet-research`; state `building`. The developer is dispatched
  one plan task at a time (`comet-research-impl.md`); each checkpoint is committed by the
  coordinator with the plan's message.
- Tasks 0 and 1 committed (f4e2767d, 5330543d). Control file said `pause` after the desk wave;
  the plan was revised (2d4a7971, d0c2feba) and the pause cleared.
- 2026-11-04: Task 2 (experiment scripts) dispatched to a fresh developer.
- 2026-11-03 18:25 local (helper, the human's order): chain M hard-killed mid-Task-2. Task 2's
  developer had written nothing and no VM was used. **Next dispatch: Task 1 Steps 4-5**
  (the desk-wave amendments: `comet-internals.md` `## Recipes`, `data-path.md` `## Rates for
  the cost model`), checkpoint `comet-research: desk wave amendments`. Then Task 2 from its
  Step 1. Read the plan's Task 1 section before dispatching.
- 2026-11-04 (coordinator, restart): Task 1 Steps 4-5 dispatched to a fresh developer. No VM
  in use; `vm.sh` does not exist yet, so no startup `stop-used`.
- 2026-11-04: Task 1 Steps 4-5 committed (96b2657f). Task 2 dispatched to a fresh developer.
- 2026-11-04: Task 2 committed (af00cb78). Task 3 (L40S S3 throughput) dispatched to a fresh developer.

## Tasks

- Task 0 (set up the run): done, committed.
- Task 1 (desk wave): Steps 1-3 done, committed (5330543d). Steps 4-5 (amendments): done 2026-11-04, checkpoint `comet-research: desk wave amendments` reported, awaiting commit.
- Task 2 (experiment scripts): Steps 1-4 done 2026-11-04; checkpoint `comet-research: experiment scripts` reported, awaiting commit.
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

### Amendments (Steps 4-5, 2026-11-04)
Two foreground `Explore` subagents; the developer wrote their text in place and spot-checked the key facts against the clone at ab46419f (fallback key and alias, no `scan.impl`, profile providers in `s3.rs:640`, benchmark conf verbatim) and over HTTP (jar and Spark tarball return 200). Step 3's check printed nothing; `exotic.md` keeps its tier header.
- `comet-internals.md` `## Recipes` (replaces `## Plan-dump recipe`): plan dumps through py4j with a pasteable `dump(spark, name, sql, outdir)`: `collect()`, then walk `df._jdf.queryExecution().executedPlan()`, assert `isFinalPlan()`, follow `executedPlan()`/`plan()`/`child()` of reused nodes/`subqueries()`, write each `CometNativeExec` block's `serializedPlanOpt().plan().get()` (dedup by hash) and `fallbacks.tsv` rows from the `CometExplainInfo.FALLBACK_REASONS` tag. Classic PySpark only (not the `-connect` tarball); Comet jar on `spark.driver.extraClassPath`.
- Fallback keys: `spark.comet.explain.fallback.enabled` (current; `spark.comet.explainFallback.enabled` is an alias), driver WARN to stderr; `spark.comet.explain.native.enabled` logs per task to executor stderr (debug only); `spark.sql.extendedExplainProviders=org.apache.comet.ExtendedExplainInfo`.
- **Scan:** 1.2.0 has no `spark.comet.scan.impl` (removed by 0.17). V1 Parquet is either `CometNativeScanExec` or Spark's scan, so local and S3 share one scan by construction; Task 4 checks there are no scan rows in `fallbacks.tsv`. Plan drift: the plan's "`spark.comet.scan.impl` values" and "same scan implementation" check become this check.
- **S3:** `fs.s3a.endpoint=https://storage.eu-west1.nebius.cloud:443`, `fs.s3a.endpoint.region=eu-west1`, `fs.s3a.path.style.access=true`, `fs.s3a.aws.credentials.provider=software.amazon.awssdk.auth.credentials.ProfileCredentialsProvider`; `export AWS_PROFILE=eu-west1` for the driver and `spark.executorEnv.AWS_PROFILE=eu-west1`. Native ignores the profile's endpoint/region, so they go in conf (not secrets). Every `fs.s3a.*` value is copied into the plan bytes, so no key may be in conf. Comet's own benchmark provider `DefaultAWSCredentialsProviderChain` is rejected by native: a required deviation. JVM S3A needs `hadoop-aws-3.4.2.jar` and AWS SDK `bundle-2.29.52.jar` in `$SPARK_HOME/jars`. Unverified: Nebius path-style and signing region.
- **Jar:** https://repo1.maven.org/maven2/org/apache/datafusion/comet-spark-spark4.1_2.13/1.2.0/comet-spark-spark4.1_2.13-1.2.0.jar (98,985,596 B; amd64 and aarch64 `libcomet.so`; x86-64-v3 so AVX2; glibc >= 2.30). Spark: https://archive.apache.org/dist/spark/spark-4.1.3/spark-4.1.3-bin-hadoop3.tgz. Java 17. Comet's benchmark conf (`benchmarks/tpc/run.py:107-137`, `engines/comet.toml:24-33`) is quoted verbatim with the single-executor deviations.
- `data-path.md` `## Rates for the cost model`: **Nebius states no NIC rate for any VM** (H100, L40S, cpu-d3/cpu-e2 at 8, 16, 128 vCPU). Only `gpu-h100-sxm` lists a ConnectX-6 100 Gbps host adapter. Assumed ranges, Gbit/s: H100 10-100; L40S 5-20; 128 vCPU 32-100; 16 vCPU 6.25-32; 8 vCPU 3.125-16 (0.39-2 Gbit/s per vCPU from AWS/GCP/Azure peers). At parity small nodes sum to 50-256 Gbit/s, 2.6-5× the H100 node at matching ends; a Standard bucket caps a tenant at 160 Gbit/s.
- Local NVMe tier: six TSV rows (g6e 0.9, a3 1.38, NC40ads 2, Nebius H100 none, B300 5.1 per GPU, on-prem 8×Gen4 GDS 54 GB/s); single-GPU cloud NVMe is slower than the same VM's NIC and lost on stop.
- Comet scale results: TPC-DS SF1000 1.84× (1.1.0, tag), 2.04× / 2.51× (1.2.0 on Spark 4.2 / 4.1.3, PR #7444); TPC-H SF1000 1.64× (2.01× hash join, 0.16.0); SF100 single box 2.27× TPC-H, 1.42× TPC-DS; SF3000 1.57×. No run at two SFs on one setup; S3 cluster costs 5-7× the core-seconds per SF of one local-NVMe box.
- Drift fixed in `data-path.md`: our L40S is `1gpu-8vcpu-32gb` (was 16 vCPU / 64 GiB, $1.75 → $1.55). Drift **not** fixed (outside this step's files): `exotic.md`'s p5.4xlarge NVMe 6 GB/s may be ~3.9 (IOPS-scaled), and its `local_nvme_x4_gds` 24.3 GiB/s is an L40S PCIe Gen4 link limit, not a drive limit.
- Platforms read with `nebius compute instance get` (control plane only; all three STOPPED, none started): CPU VM `cpu-d3` `128vcpu-512gb`, L40S `gpu-l40s-a` `1gpu-8vcpu-32gb`, H100 `gpu-h100-sxm` `1gpu-16vcpu-200gb`. The `nebius` CLI is at `~/.nebius/bin`, not on an agent shell's PATH (`gpu-host.sh` adds it).

## VM hours

Times are the local host's (UTC). The count is `vm.sh hours <vm>`, from `.claude/ensemble/M.vm-log.tsv`.

| VM | event | local time | note |
|---|---|---|---|
| l40s | use requested | 2026-11-04 03:07:47 | found RUNNING (up 15 min, idle: no card process, no test, no build; watcher "idle 908s") |
| l40s | dead-man armed | 03:07:51 | guest, 3 min (Task 2 Step 4) |
| l40s | guest poweroff | 03:10:51 | the timer fired; ssh stopped answering after 03:10:47 |
| l40s | STOPPED | 03:13:39 | API state, polled with `vm.sh status l40s` (STOPPING from 03:12:05) |
| l40s | stop-used | 03:13:46 | logged `lost found STOPPED`; no stop call needed |

- After Task 2: l40s 0.05 h of the 1 h cap by the log; the wall time from request to STOPPED was
  0.10 h. cpu128 0.00 h of 2 h.
- The log undercounts a dead-man stop: `lost` closes a use at its last heartbeat (the plan's
  rule), here 03:10:47, while the VM ran to 03:13:39. A long detached run polled every 9 min can
  lose up to that much per use. Task 6 should report wall times beside the log's hours.

## Task 2: experiment scripts (2026-11-04)

- Tests: `bash llm-wiki/artifacts/comet-research/scripts/tests/test_vm_log.sh` (14 cases) and
  `python3 -m pytest llm-wiki/artifacts/comet-research/scripts/tests` (17). Both passed; both
  were watched failing first (scripts absent).
- **Dead-man result (Step 4): the guest poweroff works.** `vm.sh use l40s --deadman-min 3` armed
  `sudo systemd-run --unit=comet-deadman --on-active=3m systemctl poweroff`. The API showed
  STOPPED 5 min 48 s after arming (2 min 48 s after the poweroff). The default stays
  `COMET_DEADMAN=guest`; the local fallback (`COMET_DEADMAN=local`, a `systemd-run --user` timer
  running `vm.sh stop <vm>`) is tested with stubs only. A dead-man stop lands about 3 min after
  its timer, so a use can overrun the cap by that much. Not checked on cpu128 (not started in
  this task); its first `use` arms the same guest timer.
- Files beyond the plan's list: `comet_data.py` (reuse, fetch and upload decisions for
  `comet_setup.sh`, tested in `test_comet_data.py`) and `test_s3_throughput.py`.
- `vm.sh` details: a `started` use counts from its `use-requested` (billing starts at the start
  call); a `found` use from the `use` row. `use` refuses (exit 1, nothing logged) when no minute
  is left in the cap. A second `use` re-arms the dead-man at the minutes now left. `stop-used`
  logs `lost` then still force-stops a RUNNING VM whose boot id changed (the human's ruling: every
  VM used is force-stopped). `vm.sh stop <vm>` exists for the local dead-man. `push-aws` merges
  the profile sections into the VM's `~/.aws` (dir 700, files 600) from JSON on ssh stdin.
- `comet_run.py` details, for Task 4:
  - Deploy: copy `scripts/` to the VM with `testdata/tpch-queries` and `testdata/tpcds-queries`
    as `scripts/queries/tpch` and `scripts/queries/tpcds`. Run with
    `SPARK_HOME=/opt/spark ~/comet/venv/bin/python comet_run.py …` after
    `comet_setup.sh cluster`. Local data is `~/comet/data/<ds>-sf40/<table>.parquet`.
  - The queries are peacockdb's DuckDB-dialect texts. Spark 4 reads `"x"` as a string, so
    `statements()` turns double-quoted names into backticks (q16, q32, q50, q62, q90, q92, q94,
    q95, q99 have them). Untested on Spark: other dialect gaps, and ANSI mode (on by default in
    Spark 4, left at Comet's default) failing a query, show up first in Step 2's calibration.
  - Every query runs under job group `<dataset>:<query>:<run>`; `comet_bytes.py` keys on it.
    Plans are dumped for run `1` only, outside the timed span. `fallbacks.tsv` has one row per
    fallback reason (`<Operator>: <reason>`), or one row with an empty reason; the operator
    counts skip AQE, query-stage, reused, InputAdapter and WholeStageCodegen wrappers.
  - `--matrix` estimates: concurrent costed as sequential (an upper bound), S3 as local × 2.0
    until an S3 run calibrates it, `--cache` adds one pass, plus 1 min start-up per run.
    `--warmup-query q1` is the TPC-H S3 warm-up.
  - A SIGTERM (Task 4's `timeout`) writes `rc` 143 and status `cut by signal 15`.
- `comet_bytes.py`: `driver_ms` is the SQL executions' wall time less the time a stage of the
  query was open; `scheduler_ms` sums, per stage, its open time less its tasks' span. `s3_gets` is
  Comet's `scan_io_http_observed_gets`; `scan_bytes` is `input.bytesRead`, which Comet feeds from
  `bytes_scanned`. Not yet checked against a real 1.2.0 event log: that a stage's accumulables
  carry the SQL metric under its display name.
- Not run anywhere yet: `comet_setup.sh`, `comet_data.py`'s S3 paths, `s3_throughput.py` against S3,
  and the Spark side of `comet_run.py`. `bash -n` passes on both shell scripts; shellcheck is not
  installed here.

## Rulings

- 2026-11-03 (helper, the human's decision): Task 1 Steps 4-5 run before Task 2. Task 2 consumes
  `## Recipes`, and Tasks 3 and 5 consume `## Rates for the cost model`; neither section exists yet.

- 2026-11-04 (coordinator): the desk wave proposed two additions. An iperf3 run between VMs is
  allowed in Task 3 if it fits the caps. A Comet run with
  `spark.comet.exec.inMemoryCache.enabled=true`, as a fairer batching baseline, goes after the
  spec's whole matrix in Task 4, lowest priority, and is skipped under the cap like any other
  configuration. The spec's matrix keeps Spark defaults.

- 2026-11-04 (coordinator, after the amendments): Comet 1.2.0 has no `spark.comet.scan.impl`
  (removed in 0.17). Task 4 Step 3's scan-parity check reads as: an S3 run's `fallbacks.tsv` has
  no scan rows that the local run lacks; otherwise `scan mismatch`. Nebius states no NIC rate for
  any type used; the cost model takes the assumed ranges in `data-path.md` `## Rates for the cost
  model`, tagged as assumptions. Known drift left for Task 5: `exotic.md`'s p5.4xlarge NVMe 6 GB/s
  (likely ~3.9) and `local_nvme_x4_gds` 24.3 GiB/s (the L40S PCIe limit, not a drive's). The
  `nebius` CLI is at `~/.nebius/bin`, not on an agent shell's PATH.

- 2026-11-04 (coordinator, after Task 2): `vm.sh hours` closes a `lost` use at its last
  heartbeat, so it can undercount by up to one poll interval per use. For every cap check in
  Tasks 3 and 4, add 10 minutes per open or `lost` use to `vm.sh hours`. The report's VM hours
  give both figures: the log's sum and the request-to-STOPPED wall time.
