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
- Task 3 (experiment 1, VM-to-S3 throughput): Steps 1-2 done 2026-11-04 on the L40S; checkpoint `comet-research: S3 throughput (L40S)` reported, awaiting commit.
- Task 4 (experiments 2 and 3, Comet on the CPU VM, plan dumps): Steps 0-5 done 2026-11-04; cpu128 STOPPED 04:33:49; checkpoint `comet-research: Comet runs and plan dumps` reported, awaiting commit.
- Task 5 (analysis and the cost model): Steps 1-7 done 2026-11-04; checkpoint `comet-research: analysis and cost model` reported, awaiting commit. Notes in `## Task 5` below.
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
| l40s | use requested | 03:15:47 | Task 3; found STOPPED, start called 03:15:48 |
| l40s | RUNNING | 03:16:48 | `gpu-host.sh start`; `use started` logged 03:16:49 |
| l40s | dead-man armed | 03:16:49 | guest, 56 min (the minutes left in the cap) |
| l40s | run | 03:17:22-03:31:32 | detached driver; ssh timed out about 03:24-03:28 (boot disk saturated), run unaffected |
| l40s | stop-used | 03:31:49 | `gpu-host.sh stop --force` from 03:31:52 |
| l40s | STOPPED | 03:32:23 | `vm.sh status l40s` reads STOPPED |
| cpu128 | use requested | 03:54:22 | Task 4; found STOPPED, started |
| cpu128 | RUNNING, ssh | 03:55:31 | `use started` logged; dead-man armed, guest, 118 min |
| cpu128 | runs | 03:55:53-04:32:00 | setup, Experiment 1, calibration, the nine configurations, Step 4 |
| cpu128 | stop-used | 04:32:57 | force stop |
| cpu128 | STOPPED | 04:33:49 | `vm.sh status cpu128` reads STOPPED |

- After Task 2: l40s 0.05 h of the 1 h cap by the log; the wall time from request to STOPPED was
  0.10 h. cpu128 0.00 h of 2 h.
- After Task 3: l40s **0.33 h** by the log (Task 3's session 0.28 h, request 03:15:47 to stop
  03:32:23). Wall time, request to STOPPED: 0.28 h this session, 0.38 h over the task. For a cap
  check, add 10 min for the one `lost` use: 0.50 h of 1 h. cpu128 0.00 h of 2 h.
- After Task 4: cpu128 **0.66 h** of 2 h by the log; wall time, request to STOPPED, 0.66 h
  (03:54:22-04:33:49). No `lost` use, so no margin to add. l40s unchanged at 0.33 h by the log.
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

## Task 3: S3 throughput on the L40S (2026-11-04)

- Files: `artifacts/comet-research/measurements/s3/{l40s.tsv,summary.tsv,notes.md}`;
  `measurements/vm-log.tsv` copied from `.claude/ensemble/M.vm-log.tsv`.
- Object `s3://tpch-sf40/lineitem.parquet` (eu-north1), 9,030,797,348 B, lineitem's only object.
  S3 ranged GETs: 0.129 / 0.568 / **0.917** / 0.862 GB/s at 1 / 4 / 16 / 64 threads. Boot disk
  (NETWORK_SSD 100 GiB, O_DIRECT, cache dropped): 0.061-0.062 GB/s at every thread count. The
  boto3 `download_file` copy to disk ran at 0.152 GB/s.
- The VM was `started`, not `found`; load was still recorded per row (load average, GPU
  utilisation, eth0 `rx_bytes`). eth0 took 9.06-9.08 GB per S3 row, so the bytes crossed the NIC.
- How it ran: `vm.sh push-aws l40s`; the script copied over ssh stdin to `~/comet/s3/` (sha256
  matched); one detached driver (`setsid nohup`, `status` and `rc` files) ran every row. The
  driver was scratch in `/tmp` on dev, not committed. VM: Python 3.12.3, boto3 1.34.46, no aws
  CLI, 24 GB free on `/`; the 9 GB copy was deleted after the reads.
- **Subtle: the boot disk is the slow source.** Nebius VMs here have no local disk; "local disk" is
  a network disk at about 0.06 GB/s. While 64 threads saturated it (load 68), ssh timed out in the
  banner exchange for about 4 min although the API said RUNNING. Task 4 should check cpu128's disk
  type first, and should not count on ssh while a run saturates the boot disk.
- Not settled: whether ~0.9 GB/s is the NIC or the Python client (CPU 216-283% of 800%). Listed as
  a follow-up in `notes.md`.

## Task 4: Comet on the CPU VM (2026-11-04)

### Step 0: local shakedown on dev (03:35-03:52 UTC, no VM time)

- Setup, all scratch under `~/comet-research/shakedown/` (outside the repo): Spark 4.1.3
  tarball, Comet 1.2.0 jar, `hadoop-aws-3.4.2` and `bundle-2.29.52` in `$SPARK_HOME/jars` (sizes
  match `## Recipes` 4-5), Java 21 (dev's; the VM gets 17), a standalone master and one 8-core
  worker. Data: `testdata/{tpch,tpcds}.sf1` symlinked as `<ds>-sf40`. A wrapper passes dev sizes
  through the new `--conf` (8 cores, executor 6g, off-heap 10g).
- Ran: every query of both lists sequential with `--dump-plans` (TPC-H 22 in ~21 s, TPC-DS 82 in
  ~82 s), concurrent TPC-H "all" and TPC-DS batch 20, `--cache --warmup-query q1` on TPC-H batch
  3, and `comet_bytes.py` over the event logs (126 then 124 query rows, scan and shuffle bytes
  non-zero on every row). An S3 smoke test from dev against eu-north1 (default profile, a
  nation-region join through the same `spark_conf`): native scan, no fallback row, `s3_gets` 4,
  no key in `conf.txt` or the plan files. So the profile provider, path-style and signing region
  `eu-north1` work; eu-west1 is first tried on the VM.
- **No query dropped.** No dialect gap beyond the double quotes Task 2 handled; no ANSI failure.
  Result rows match DuckDB on sf1 for all 22 + 82 queries (a scratch check comparing the new
  `rows` column with DuckDB 1.5.4 on the original texts).
- Comet falls back on TPC-DS q9 (struct-typed project), q10 and q35 (count merge buffer), q14
  (decimal AVG at max precision); every other query is Comet end to end.
- Fixes, each with a test written and watched failing first (pytest 17 -> 24, `test_vm_log.sh`
  14 unchanged, all pass):
  - `comet_bytes.py` crashed on Spark 4's rolling event logs (`eventlog_v2_<app>/events_<n>_…`
    plus a binary `.appstatus…crc`). It now reads each app's parts as one stream in numeric order
    and skips status files (`log_streams`).
  - `fallbacks.tsv` `spark_ops` counted wrappers (TPC-H q1 showed 2 Spark ops where Comet reports
    8/8 accelerated): `AQEShuffleReadExec`, `SubqueryExec`, `ColumnarToRowExec`,
    `RowToColumnarExec`, `InMemoryRelation` are skipped (`op_kind`), and the walk no longer follows
    `CometInMemoryTableScanExec`'s subquery, the Spark scan it replaced (`follows_subqueries`).
    After the fix every `spark_ops` > 0 has a reason row.
  - Native plan `.txt` files carried the query text once per expression (2.5 MB a block on
    TPC-DS q66; 66 MB of TPC-DS's 70 MB). `strip_sql_text` drops those lines; the `.pb` keeps
    them. TPC-DS dumps are now ~22 MB on disk over ~1,700 files, none over 5 MB.
  - New: `runs.tsv` has a `rows` column; `--conf KEY=VALUE` overrides a Spark setting.
- Not checked locally: `s3_gets` on eu-west1, the `--matrix` estimates against real sf40 times.

### The CPU VM's disks (read before setup, from `nebius compute instance get`)

- One disk: the boot disk, `NETWORK_SSD`, 100 GiB, no secondary disk or filesystem. The L40S's
  identical disk type read at 0.06 GB/s. sf40 Parquet is 58.2 GB (TPC-H 42.4, TPC-DS 15.8),
  so data plus the 100 GB spill reserve cannot fit there.
- Decision: data and Spark's local dirs (shuffle and spill) go on tmpfs, `/dev/shm/comet/`
  (`~/comet/data` and `~/comet/spark-local` are symlinks to it); the RAM is the fastest disk the VM
  has. Local runs are therefore "RAM-resident (tmpfs)", stricter than page-cache warm.
- Deviations to fit RAM, passed with `--conf`: `spark.memory.offHeap.size=256g` (was 320g; 2 GB
  per core, the ratio of Comet's own benchmark conf, 16g for 8 cores) and
  `spark.cleaner.periodicGC.interval=5min` (so shuffle files on tmpfs are cleaned between queries).
  Budget: 58 GB data + driver 16 + heap 64 + overhead 48 + off-heap 256 = 442 GB of 512.

### Session (Steps 1-5), live log; times UTC (dev and VM clocks agree)

- 03:54:22 `vm.sh use cpu128`: found STOPPED, started; RUNNING with ssh 03:55:32; boot id
  `90ed1f00-…`; guest dead-man armed at 118 min (poweroff about 05:52). `push-aws`: default,
  eu-west1. Scripts copied (sha256 of `comet_run.py` matched) with the queries, plus two scratch
  helpers `runone.sh` (one configuration, detached under `timeout`, adds the two `--conf`
  deviations) and `s3rows.sh` (Experiment 1).
- VM as found: 128 vCPU AMD EPYC Genoa (AVX2), 503 GiB RAM, `/dev/shm` 252 GiB, boot disk `vda`
  100 GiB (93 GiB free), Ubuntu 24.04.5, NIC eth0. No `/var/lib/peacock`, so no chain L copy.
- `comet_setup.sh all` 03:55:53-03:59:55. Versions: OpenJDK 17.0.20.1, Spark 4.1.3, Comet 1.2.0
  jar 98,985,596 B, hadoop-aws 3.4.2, AWS SDK bundle 2.29.52, Python 3.12.3, pyarrow 26.0.1,
  boto3 1.43.123. Data copied (no reuse) from eu-north1 to tmpfs: TPC-H 42,442,683,465 B in 74.2 s
  (0.572 GB/s), TPC-DS 15,787,929,533 B in 35.2 s (0.449 GB/s). Uploaded to the eu-west1 buckets:
  TPC-H 28.3 s (1.502 GB/s), TPC-DS 13.5 s (1.173 GB/s); every size matches eu-north1. tmpfs free
  after: 198 GiB of 252. Standalone cluster up (one worker, 128 cores, 480g).
- A launch over ssh with a detached child hung the ssh until my 120 s timeout although the child
  started; launches now run under a 30 s timeout.
- 04:00:19 Experiment 1 launched (`s3rows.sh`): lineitem from eu-west1 at 1/4/16/64 threads, the
  16-thread cross-region row against eu-north1, then orders.parquet copied to the boot disk and
  read with O_DIRECT at 1 and 16 threads.
- Experiment 1 S3 rows (lineitem, 9,030,797,348 B): eu-west1 in-region 0.195 / 1.164 / 1.163 /
  1.028 GB/s at 1 / 4 / 16 / 64 threads (client CPU 17 / 173 / 296 / 1408 %); cross-region
  eu-north1 at 16 threads 0.902 GB/s. eth0 took ~9.05 GB per row.
- **The data vanished** after the 04:00:19 launch: `/dev/shm/comet` was gone. systemd-logind's
  `RemoveIPC=yes` (Ubuntu default) deletes a user's files in `/dev/shm` when their last session
  ends, and the 30 s launch ssh was the only session. Fix: a dedicated tmpfs,
  `sudo mount -t tmpfs -o size=320g tmpfs /mnt/comet`, which RemoveIPC does not touch;
  `~/comet/{data,spark-local}` now link there. Data re-fetched 04:02:26-04:04:15 (TPC-DS 0.482
  GB/s; the eu-west1 upload was a no-op, sizes matched). A restarted run must redo the mount
  and the fetch: a tmpfs does not survive a stop.
- **Second bug, `comet_setup.sh cluster`**: `/opt/spark` is root's, so `start-master.sh` could not
  write its log and no daemon started, yet the script printed "cluster up". Fixed in the repo
  script: `SPARK_LOG_DIR`/`SPARK_PID_DIR` under `~/comet`, then a check that the worker
  registered, else exit 1. Verified on the VM (worker registered 04:04:20, 128 cores, 480 GiB).
  No automated test (shell, needs Spark); the VM run is the check.
- Boot-disk rows (orders.parquet, 2,525,325,843 B; copy to disk 41.1 s, 0.062 GB/s): O_DIRECT
  0.062 GB/s at 1 thread, 0.063 at 16. Same as the L40S's disk.
- Step 2 calibration, local (tmpfs), sequential, 1 run, with `--dump-plans`: TPC-H 04:07:00-04:08:18,
  batch 67.4 s, rc 0. Memory after: 60 GiB used, tmpfs 55 GiB. TPC-DS launched 04:08:30.
- TPC-DS calibration 04:08:30-04:11:49, batch 190.7 s, rc 0. `measurements/comet/calibration.tsv`
  written (104 rows). `--matrix` at 04:12: estimates 2.9, 2.9, 3.1, 3.1, 3.1, 2.9, 5.3, 2.9, 2.9
  min, 29.1 cumulative. Used 0.30 h + 10 min margin: about 57 of 120 min. **No projected skips.**
  Each configuration runs as `runone.sh <min(3 x est, minutes left)> ~/comet/out/<name> …`.
- Matrix, outcomes (cap check before each; `vm.sh hours` shown):
  - 1 `tpch-local-sequential-ball` (0.30 h) 04:12:15-04:14:08, rc 0: batch 59.6 s, 46.9 s.
  - 1 `tpch-local-concurrent-ball` (0.33 h) 04:14:22-04:15:41, rc 0: batch 38.3 s, 33.9 s.
  - 2 `tpcds-local-sequential-ball` (0.36 h) 04:15:49-04:18:00, rc 0: batch 122.3 s.
  - 2 `tpcds-local-concurrent-ball` (0.40 h) 04:18:07-04:19:43, rc 0: batch 87.5 s, no OOM, so
    no batch-20 fallback. Peak seen in polls: 167 GiB used, tmpfs 72 GiB.
  - 3 `tpch-s3-sequential-ball` (0.43 h) 04:20:00-04:21:40, rc 0: q1 warm-up 5.9 s, batch 84.7 s.
    Run with `--dump-plans ~/comet/plans-s3/tpch` (untimed) for the scan-parity check.
  - 3 `tpch-s3-concurrent-ball` (0.46 h) 04:21:51-04:22:47, rc 0: batch 46.5 s. No dump (it would
    fall inside the timed batch); same conf as the sequential S3 run.
  - 4 `tpcds-s3-sequential-ball` (0.48 h) 04:22:59-04:27:35, rc 0: batch 265.6 s, with
    `--dump-plans ~/comet/plans-s3/tpcds` (untimed).
  - 5 `tpch-local-sequential-ball-cache` (0.56 h) 04:27:55-04:29:12, rc 0: batch 53.3 s.
  - 5 `tpch-local-concurrent-ball-cache` (0.58 h) 04:29:26-04:30:26, rc 0: batch 36.1 s. Both
    read the cache: 219 and 222 of their event logs' plans have `CometInMemoryTableScan`.
  - Nothing skipped, cut, OOM or with a scan mismatch; `skipped.tsv` has its header only.
- Scan parity: no scan row in any `fallbacks.tsv`, local or S3. TPC-H's local and S3 files are
  identical; TPC-DS's differ only in AQE op counts (q31, q39) and an expression id (q35).
- Step 4, 04:30-04:32: `comet_bytes.py` on the VM over the 11 apps' logs (230 MB, left on the VM's
  boot disk) gave 527 rows. It needed a fix first: rows carried no configuration, so `tpch q1 run 1`
  of six apps could not be told apart. A leading `config` column (the app name less `comet-`) was
  added, test first (pytest 24 pass). Per pass: TPC-H scans 48.3 GB and shuffles 46.4 GB; TPC-DS
  98.4 and 56.5 GB; no spill anywhere. `s3_gets`: 64,991 (TPC-H sequential with warm-up),
  62,809 (TPC-H concurrent), 260,416 (TPC-DS). The cache runs report 456 GB scanned: the decoded
  in-memory bytes, not Parquet.
- Footer cross-check (`footer-check.tsv`; a scratch script summed the projected columns'
  compressed chunk sizes over all row groups): TPC-H q1 0.986, TPC-DS q3 0.983, q42 0.983, q96
  1.000. TPC-H q6 0.153 and q14 0.028 are below because their date filters prune row groups and
  pages; footers are an upper bound.
- 04:32:57 `vm.sh stop-used`; STOPPED 04:33:49 (`vm.sh status cpu128`). Wall time from request
  to STOPPED: 39.5 min.
- Calibration (first pass) is slower than the measured runs (TPC-H 67.4 s against 59.6 / 46.9;
  TPC-DS 190.7 against 122.3): JIT and first-touch warm-up. Run 2 of TPC-H is 21% faster than
  run 1, so single-run TPC-DS numbers carry some warm-up too.
- Files written: `measurements/comet/<config>/{runs.tsv,conf.txt,status,rc,exit}` for the nine
  configurations and `cal-tpch`, `cal-tpcds`; the two S3 sequential dirs also hold their
  `fallbacks.tsv`. `measurements/comet/{summary,bytes,calibration,skipped,footer-check}.tsv`.
  `plans/{tpch,tpcds}/` from the calibration runs (21.4 MB, 2,322 files, none over 5 MB).
  `measurements/s3/cpu128.tsv`, cpu128 rows in `measurements/s3/summary.tsv`, a cpu128 section
  in `measurements/s3/notes.md`. `measurements/vm-log.tsv` copied after the stop.

## Task 5: analysis and the cost model (2026-11-04)

- No VM used in this task.
- New measurement, no VM: `scripts/footers.py` read every sf40 Parquet footer from eu-north1 by
  ranged GETs (default profile, from dev, 04:44:41-04:44:45 UTC) into
  `measurements/footers/sf40.tsv` (490 columns; TPC-H 42.43 GB compressed, TPC-DS 15.26 GB).
  It gives exact per-column bytes for the batch-union (shared-scan) model.
- `scripts/plan_census.py plans` parses every native block's protobuf text (no `protoc` here,
  so a small text-format parser) and writes `plans/{census,eligible,columns}.tsv`. Class tables
  (supported / adaptable / missing) live in the script; `translatability.md` explains them.
  Tests: `scripts/tests/test_plan_census.py` (7), `test_footers.py` (2), watched failing first.
- Census: no missing operator in any plan (the 82 TPC-DS exclude the window queries). Missing
  expressions only in TPC-H q2, q20 (runtime bloom filter: `bloomFilterAgg`,
  `bloom_filter_might_contain`, `xxhash64`), TPC-DS q9 (`create_named_struct`), q24, q99
  (`jvm_scalar_udf`); Spark-side fallbacks in q10, q14, q35 (`fallbacks.tsv`).
- Batch union from footers and the plans' column sets: TPC-H cut 1.05 / 3.34 / 4.37 / 5.31 at
  3 / 10 / 20 / all; TPC-DS 1.08 / 1.94 / 2.62 / 8.07 (measured scan bytes over union bytes).
- `exotic.md` drift fixed (the Rulings' two items): `local_nvme_x1_p5_4xlarge` 6 -> 3.9 GB/s;
  `local_nvme_x4_gds` and section 4 now say 24.3 GiB/s is the L40S PCIe Gen4 link limit.
- Subtle: `bytes.tsv` `scheduler_ms` sums per-stage gaps over stages that run in parallel, so it
  can exceed the query's wall time (TPC-H q8: 7.1 s of scheduler gaps in a 2.9 s query). It is
  not a wall-time overhead. The model takes `driver_ms` whole and a modelled share of
  `scheduler_ms`, capped at the query's own time.

- Results (all four headline cells conditional go; 8 x 16, batch "all", faster baseline, NIC tier):
  TPC-H sf40 1.88 [0.50-12.88], sf200 1.38 [0.22-13.40]; TPC-DS sf40 2.14 [0.54-18.10], 1 TB
  1.58 [0.06-37.40]. Device factor 0.42-0.88 in every cell; sharing 2.38-3.74. One-pass median
  0.81 (TPC-H) and 0.83 (TPC-DS) at sf40. Top sensitivity rows: `shuffle_rate_gpu_node` (TPC-H
  both), `scheduler_share` (TPC-DS sf40), `cpu128_s3_concurrent_all_s.tpcds` (TPC-DS 1 TB).
- Tests: `python3 -m pytest llm-wiki/artifacts/comet-research/test_cost_model.py
  llm-wiki/artifacts/comet-research/scripts/tests` 47 passed (cost model 14); `bash
  .../scripts/tests/test_vm_log.sh` 14 passed. `cost.md` and `plans/*.tsv` regenerate byte for byte.
- Two bugs found by the generator, each pinned by a test written red first: the NIC tier's rate
  became an array over corners and `math.isinf` on it failed (the `vram` tier now says "no wire"
  with `None`); an empty tier (`nebius_h100_1gpu_no_local`, 0 GB/s, 0 GB) divided by zero (now
  all its bytes go over the NIC).
- Model choices that depart from the plan's text, each stated in cost.md's header:
  - shuffle on the H100 node is `shuffle_bytes / shuffle_rate_gpu_node` (Comet's CPU-side codec
    on 16 cores), not `/ NIC`: one node shuffles inside the box;
  - `spark_overhead` = `driver_ms` + `scheduler_share` x `scheduler_ms` (see the subtle point above);
  - compute for TPC-H q1, q3, q6, q8 is benchmark-minimal's measured H100 G=1 time (8 x H100 table),
    not H200 scaled by HBM; the H200-scaled figure is `gpu_measured_scale`'s high end;
  - one-pass reads S3 at `s3_efficiency_sequential` (0.490, Comet's sequential run), batches at
    `s3_efficiency` (0.892, the concurrent run), both measured-derived;
  - `ratio()` also takes baseline `faster` (per corner, the lesser CPU time) and `cpu_shares=True`
    (the "CPU with shared scans" row);
  - in-repo report figures (benchmark-minimal.md) are tagged `published`, since `measured` must
    point under `measurements/` (a test enforces it);
  - the tier table leaves out `gb10_unified`: a tier swaps the data path and keeps the H100's
    compute, and GB10's GPU is about 19x slower in memory bandwidth.
- Steps 1 and 7 were written by the developer, not a subagent: Step 1 needed code (the census),
  and Explore subagents cannot write files. `cost.md` is 420 lines, over the 300-line cap for
  findings files; it is generated and holds the plan's required tables.
- Plan drift: Task 5 Step 4 says `spark_overhead` is `driver_ms` plus `scheduler_ms`; that sum is
  not wall time (above). Files beyond the plan's list: `scripts/plan_census.py`,
  `scripts/footers.py`, their tests, `measurements/footers/sf40.tsv`, `plans/{census,eligible,columns}.tsv`.

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

- 2026-11-04 (coordinator, before Task 4): the Spark side of `comet_run.py` and the `--dump-plans`
  path have never run, and debugging them on the CPU VM spends its 2 h cap. So Task 4 opens with
  a local shakedown on dev (8 cores, 31 GB, no VM time): Spark 4.1.3 and the Comet jar, TPC-H and
  TPC-DS at sf1 from local Parquet, every query of both lists through `comet_run.py --source local
  --mode sequential --runs 1 --warmup 0 --dump-plans`, plus one short concurrent batch and one
  `comet_bytes.py` pass over its event log. Dialect fixes, ANSI failures and broken metric names
  are fixed here. A query that cannot run under Spark is recorded and dropped from the list for
  both datasets' runs, and the report names it. Its times are not measurements and are not
  committed as such. Also on the CPU VM: read its disk type before setup, and keep data and
  Spark's spill on the fastest disk it has.
- 2026-11-04: Task 3 committed (5008e6a6). L40S used about 0.33 h by the log, 0.50 h with margin.
  Task 4 dispatched to a fresh developer.
- 2026-11-04: Task 4 committed (cf35deed): all nine configurations ran, none skipped; cpu128
  0.66 h, STOPPED. Task 5 dispatched to a fresh developer. Coordinator note for Task 5: the
  "local" runs are RAM-resident (tmpfs), the strictest local baseline; the S3 runs read
  in-region, so no in-region correction is modelled. Fix the `exotic.md` drift noted above.
- 2026-11-04: Task 5 committed: every headline cell a conditional go, bands straddle 2×. Task 6
  dispatched to a fresh developer.
