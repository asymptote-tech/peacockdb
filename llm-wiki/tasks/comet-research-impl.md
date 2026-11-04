# comet-research Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One report, `llm-wiki/reports/comet-gpu.md`. It says go, conditional go or no-go on a
GPU backend for Apache Comet against the 2× cost-per-query bar. On a go, it gives the alpha design.

**Architecture:** Chain M's only task, `Kind: prototype` with a PR and no reviewer, in workspace
alpha on dev, on `ENS-comet-research`. The developer runs every step. It has no web tools, so it
dispatches desk research and analysis to its own `general-purpose` subagents. Each writes one
artifact file under `llm-wiki/artifacts/comet-research/` and returns at most ten lines. The
developer runs the experiments through a VM wrapper that logs every use, force-stops what was
used, arms a dead-man stop on each VM, and counts hours against the caps. Long runs go detached
on the VM and are polled. A tested cost model turns measured, published and modelled inputs into
the verdict by a rule fixed in the spec. The coordinator owns every git write and the PR.

**Tech Stack:** Agent tool (`general-purpose`), `anthropic-skills:deep-research`, `gh`, the
`nebius` CLI, `scripts/gpu-host.sh` (L40S), Python 3 with boto3, pyarrow and pytest, Apache Spark
standalone with the Comet release jar, `fio`.

**Spec:** [`comet-research.md`](comet-research.md). Read it first. This plan argues from it.

**Revision (2026-11-03, after an analyst's reading; the run paused after Task 1 to take it).**
The human ruled: the verdict rule; 1 × 128 only; the CPU VM cap 2 h; force-stop every VM used;
concurrent runs for batch "all" only; the H100 VM not used, its S3 measurement on the L40S;
in-region eu-west1 buckets for the CPU VM.
Everything else here is the helper's call. Of the coordinator's two rulings after the desk wave,
the in-memory-cache run stays (Task 4, priority 5) and the iperf3 run between VMs is dropped: with
the H100 out, the two VMs left are in different regions.

## Global Constraints

- **Roles.** A step ending "report a checkpoint `<msg>`" is a checkpoint, not green: the
  developer stops and reports; the coordinator commits with that message, pushes, and dispatches
  the next step. The developer never runs a git write. Only Task 6 Step 5 is green. Then the
  coordinator does Task 6 Step 6. No `reviewing` state, reviewer, completeness pass or CI wait.
  The same duties are on the board's chain M section, which the coordinator reads at startup.
- **The developer's done** is this plan's tests (Tasks 2 and 5) passing and the audit's findings
  fixed. No engine test suite runs.
- **No peacockdb executor runs.** Nothing under `peacockdb-core/`, `peacockdb-ffi/`, `cpp/`,
  `flatbuffers/`, `docker/` or `scripts/` changes. Experiment scripts live in
  `llm-wiki/artifacts/comet-research/scripts/`.
- **Credentials.** Never open a Nebius credential file (`~/.nebius/config.yaml`,
  `credentials.yaml`, any `*.pem` under `~/.nebius/`, on any host). Learn the CLI from `nebius …
  --help`. S3 is the default AWS profile, endpoint `https://storage.eu-north1.nebius.cloud:443`,
  read through the SDKs' profile providers. No key is ever put in Spark conf, a command line, or
  a file, and `conf.txt` is redacted (Task 2).
- **In-region buckets** (the human's decision): the CPU VM reads `tpch-sf40` and `tpcds-sf40`
  in eu-west1, which exist empty and are filled in Task 4 Step 1. Every eu-west1 access uses
  `--profile eu-west1` (its config carries the endpoint and region); eu-north1 uses the default
  profile with `--endpoint-url https://storage.eu-north1.nebius.cloud:443`. The L40S reads
  eu-north1. A cross-region read appears only as the labelled reference row.
- **AWS profiles on a VM.** `vm.sh push-aws <vm>` writes dev's `default` and `eu-west1` profile
  sections into the VM's `~/.aws/` (mode 600) through ssh stdin, never a command line, and
  prints only the profile names. The L40S gets `default` only.
- **Hosts:** L40S through `scripts/gpu-host.sh`; CPU VM `computeinstance-e01t2mm4265y8x88ey`
  (eu-west1, profile `eu-west1`) through the `nebius` CLI. **The H100 VM is never used** (the
  human's decision): `vm.sh` has no entry for it. Login `dmitry`, the default ssh key, known hosts
  in `~/.ssh/known_hosts_comet` with `StrictHostKeyChecking=accept-new`, keyed by
  `HostKeyAlias=comet-<vm>`.
- **Every VM used is force-stopped** (the human's ruling): at the end of each measurement session
  and before any exit, whether this chain started it or found it running, even under chain L.
  `vm.sh stop-used` does it; for the L40S it is `gpu-host.sh stop --force`. The coordinator
  pre-starts no VM. It runs `stop-used` (when `scripts/vm.sh` exists) at startup if no dispatch
  is live, after any dispatch that ends without a report, and before every exit.
- **Dead-man stop.** Each `vm.sh use` arms a stop that needs no live agent: on the VM, `sudo
  systemd-run --on-active=<minutes left in the cap>m systemctl poweroff`. Task 2 verifies once
  that a guest poweroff shows STOPPED in `nebius compute instance get`. If it does not, the
  fallback is a local `systemd-run --user --on-active=… <abs path>/vm.sh stop <vm>` on dev.
- **Caps, summed over the task:** CPU VM 2 h, L40S 1 h. `vm.sh hours <vm>` is the
  count. Before each configuration, if hours used plus its calibrated estimate exceed the cap, it
  is skipped (`skipped.tsv`). Each configuration also runs under `timeout` set to the lesser of
  3 × its estimate and the minutes left; a timed-out one is `cut at cap`.
- **Long runs are detached on the VM** (`setsid nohup … &`, writing a `status` file and an `rc`
  file), and the developer polls with foreground `vm.sh <vm> ssh` calls of at most 9 minutes. No
  Bash call runs longer than 10 minutes, and no local command uses `run_in_background`.
- **Cuts.** A configuration cut by a VM stopping under it is re-run once. Cut again, it is `cut`
  and the run moves on. Cut time counts against the cap.
- **Exit 75 (no capacity)** from any VM: the developer records it and moves to the next VM or
  configuration. The coordinator retries `vm.sh <vm> use` every 15 minutes for at most 2 hours,
  then the experiment is recorded skipped, never stalled.
- **The coordinator never exits voluntarily at a checkpoint**, only at a point with no VM in use
  and only when its window forces it: three restarts with the board unchanged stall the chain.
- **Bar, workload and verdict rule:** the spec's "The bar for go", verbatim. The headline cell
  is batch "all", the faster CPU baseline, parity with eight 16-core nodes, per dataset, at sf40
  and at the first size past host RAM; go only if the band's low end is ≥ 2×.
- **Query lists:** TPC-H q1-q22 in numeric order. TPC-DS: the 82 lines carrying `tp1_single` in
  `git show origin/ENS-containers-rebase:peacockdb-core/tests/common/corpus_benchmark_cases.inc`,
  in file order, copied once into `scripts/tpcds-82.txt`. Batches are the first 3, 10, 20 and all.
- Every factual claim carries a link and `(accessed YYYY-MM-DD)`, code a commit SHA. Every
  measurement gives host, local date and time, and method. Every cost-model input is tagged
  `measured`, `published` or `modelled`. Guesses are marked `(estimate)`.
- External repos are cloned under `~/comet-research/ext/` on dev, outside every worktree. No
  committed file under the artifact folder exceeds 5 MB; raw event logs stay on the VM, and only
  their summaries are committed.
- A subagent returns at most ten lines. The writing is plain: short sentences, ordinary words.

## Review Focus

1. **A VM left running after an agent dies.** Pinned by the dead-man stop (Task 2 Step 4 checks it
   on a real VM), the coordinator's startup `stop-used`, and Task 6 Step 4.
2. **A ratio manufactured by the baseline or by batching.** Pinned by both CPU modes, Comet's own
   benchmark config with deviations listed (Task 4), the same scan implementation on both sources
   (Task 4 Step 3), and `cost.md` splitting the ratio into its sharing and device factors (Task 5).
3. **Modelled GPU numbers presented as measured.** Pinned by the input tags, which a test
   enforces, and by the audit's check of every `measured` row against `measurements/`.
4. **Missing bytes.** Every GPU-side and NIC term needs per-query bytes; Task 4 gathers them from
   Spark event logs into `bytes.tsv`, cross-checked against Parquet footers.
5. **An arithmetic slip in the model** (Gbit/s against GB/s, H200 against H100). Pinned by the
   audit's hand recomputation of the headline cells (Task 6 Step 2).

---

## Artifact layout (the interface every task shares)

    llm-wiki/artifacts/comet-research/
      comet-internals.md      Task 1: group 2, group 3's seam, plan-dump and scan recipes
      prior-art.md            Task 1: group 1, and group 4's "how others handle it"
      data-path.md            Task 1: group 4's survey: instances, NIC rates, local SSD, wire savings
      batching.md             Task 1: group 5's API options and prior art, SCOPE overlap numbers
      exotic.md               Task 1: group 4's exotic architectures, with the tier table
      scripts/                Task 2: vm.sh, s3_throughput.py, comet_setup.sh, comet_run.py,
                              comet_bytes.py, tpcds-82.txt, tests/
      measurements/vm-log.tsv copied from .claude/ensemble/M.vm-log.tsv at each checkpoint
      measurements/s3/        Task 3
      measurements/comet/     Task 4: one dir per configuration, summary.tsv, bytes.tsv, skipped.tsv
      plans/{tpch,tpcds}/     Task 4: final executed plans, fallbacks.tsv
      split.md                Task 5: the per-operator CPU/GPU placement rule
      translatability.md      Task 5: the classification and counts
      cost_model.py, test_cost_model.py, inputs.tsv, cost.md   Task 5
      design.md               Task 5: the alpha design, or the conditions for a go
      audit.md                Task 6

Every findings file is at most 300 lines, opens with a five-line `## Summary`, and ends with
`## Follow-up experiments` and `## Sources`.

---

### Task 0: Set up the run (done before the revision)

- [x] Folders, `-detail.md`, clones under `~/comet-research/ext/`, Comet pinned
  (`comet-research: start the run`).

### Task 1: Desk wave (five parallel subagents)

**Files:** `comet-internals.md`, `prior-art.md`, `data-path.md`, `batching.md`, `exotic.md`

Steps 1-3 ran before the revision, with the jobs as written then. Step 4 adds what the revision
needs.

- [x] **Step 1:** Dispatch all five at once, each with the prompt template below.
  - **comet-internals:** spec group 2 in full, plus group 3's first and last bullets. Read from
    the Comet clone at the pinned SHA, cited by file and line. Include the protobuf schema's
    operator and expression lists, as a table Task 5 can classify.
  - **prior-art:** spec group 1, and group 4's "how others handle it". Speedata, spark-rapids,
    Gluten and Velox-cuDF, Auron and Photon at least. Any GPU or hardware effort on Comet itself.
  - **data-path:** spec group 4's NIC, wire, RAM-cache and local-SSD bullets. A table of cloud
    GPU and CPU instances (AWS, GCP, Azure, Nebius) with cores, RAM, NIC, local NVMe, GDS support
    and on-demand price. The wire savings, each with a published number. The local-SSD
    product-option bullet with its three layouts. Whether a RAM cache is possible inside Comet
    and Spark.
  - **batching:** spec group 5's API, CBO-placement and prior-art bullets. Microsoft's SCOPE
    overlap numbers. What Spark already shares within and across queries.
  - **exotic:** spec group 4's exotic-architectures bullet, plus CXL vendors (Samsung, Astera
    Labs, Liqid, UnifabriX, Panmnesia), product-research's Grace option file, and Spark's fabric
    paths (spark-rapids' UCX shuffle, SparkRDMA). End with the tier table:
    `tier	GB_per_s_to_GPU	capacity_GB	price_per_hour	cloud|on-prem	source`.
- [x] **Step 2:** As each returns, append its ten lines to `-detail.md`.
- [x] **Step 3: Verify**, in the artifact folder: `for f in comet-internals prior-art data-path
  batching exotic; do for h in Summary Sources 'Follow-up experiments'; do grep -qF "## $h"
  $f.md || echo "$f: $h"; done; grep -q 'accessed 20' $f.md || echo "$f: no dates"; done` prints
  nothing, and `exotic.md` has the tier table's header. Re-dispatch the owner for any gap.
- [ ] **Step 4: Amendments** (revision). Dispatch two subagents with the same template, each
  editing an existing file in place:
  - **comet-internals**, a new `## Recipes` section, at the pinned tag:
    - how to get each query's *final executed* plan after `collect()` under AQE
      (`isFinalPlan=true`), and the serialized native plans of its `CometNativeExec` nodes,
      through py4j;
    - the fallback settings (`spark.comet.explainFallback.enabled`,
      `spark.comet.explain.native.enabled`) and where their output goes;
    - Comet's own benchmark configuration for TPC-H and TPC-DS at that tag, verbatim, with its
      path;
    - how the native scan reads S3 (the `spark.comet.scan.impl` values, the S3A-to-object_store
      settings, the AWS profile provider), so S3 and local runs use the same scan;
    - the release jar matching the pinned Spark version, and its URL.
  - **data-path**, a new `## Rates for the cost model` section:
    - the provider's stated NIC rate for the H100, L40S and 128-core VM instance types, and for
      Nebius's 8- and 16-vCPU CPU sizes;
    - the local NVMe tier as rows in the tier-table format above;
    - Comet's published results at SF100 and SF1000, or the nearest, for CPU scaling past sf40.
- [ ] **Step 5:** Re-run Step 3's check. Report a checkpoint `comet-research: desk wave amendments`.

**The prompt template** (fill the brackets):

> You are researching one part of a decision: whether to build a GPU backend for Apache Comet on
> peacockdb, a GPU SQL engine (Rust/DataFusion 45 planner, FlatBuffers plan IR, C++/cuDF
> executor; map in `llm-wiki/architecture.md`). Context: [the spec's Why and The bar for "go"
> sections, pasted]. A prior desk study, `llm-wiki/artifacts/product-research/options/spark-plugin.md`,
> is a list of leads: re-verify any fact from it you rely on, at current versions. Comet is
> cloned at [path], pinned at [tag, SHA]; other clones: [paths]. Your job: [job, with the spec's
> bullets pasted]. Write findings to [path], at most 300 lines, opening with `## Summary` (five
> lines) and ending with `## Follow-up experiments` and `## Sources`. Use
> `anthropic-skills:deep-research` for open-ended questions. Desk only: never start or touch a
> VM, and never run a git write. Clone only under `~/comet-research/ext/`. Every claim carries a
> link and `(accessed YYYY-MM-DD)`, code a commit SHA. Mark guesses `(estimate)`. Look for
> evidence against a GPU backend as hard as for it. Write nothing else in the repo. Reply in at
> most ten lines: verdict and file path.

### Task 2: The experiment scripts

**Files:** `scripts/vm.sh`, `scripts/s3_throughput.py`, `scripts/comet_setup.sh`,
`scripts/comet_run.py`, `scripts/comet_bytes.py`, `scripts/tpcds-82.txt`, `scripts/tests/`
(`test_vm_log.sh`, `test_comet_run.py`, `test_comet_bytes.py`, `fixtures/`)

**Interfaces:**
- Consumes: `comet-internals.md` `## Recipes`.
- Produces:
  - `vm.sh use <vm>`: logs `use-requested`, then reads the state. STOPPED: start it, wait for
    RUNNING and ssh, log `use started`. RUNNING: log `use found`. STARTING or STOPPING: wait,
    then re-read. Then record the VM's `/proc/sys/kernel/random/boot_id` and arm the dead-man
    stop. `<vm>` is `cpu128` or `l40s` (l40s delegates to `gpu-host.sh`); any other name is exit 1.
  - `vm.sh ssh <vm> <cmd…>`, `vm.sh status <vm>`, `vm.sh ip <vm>`. Each `ssh` refreshes the
    heartbeat (time and boot id) in the log.
  - `vm.sh stop-used`: force-stops every VM with an open use, and logs `stop`. A VM already
    STOPPED, or with a different boot id, is logged `lost`, closed at its last heartbeat.
  - `vm.sh hours <vm>`: sums use-to-stop intervals from the log, an open use up to now, a
    `lost` one up to its last heartbeat. Prints hours with two decimals.
  - The log: `local_time	vm	event	detail`, at `$(git rev-parse --show-toplevel)/.claude/ensemble/M.vm-log.tsv`
    (git-ignored), overridable by `COMET_VM_LOG`. Exit codes as `gpu-host.sh`'s (0, 75, 69, 1).
    On exit 1 from a call that started the VM, it stops it.
  - `s3_throughput.py --bucket B --key K --threads N`: ranged GETs through boto3 into
    `/dev/null`. `--file F` reads a local file with `O_DIRECT` after dropping the page cache.
    Prints one TSV row: `vm	threads	bytes	seconds	GB_per_s	source	client_cpu_pct`.
  - `comet_setup.sh`: on the CPU VM. Installs Java, the pinned Spark, the Comet release jar,
    pyarrow and boto3. Data: it reuses an existing sf40 copy (chain L's default
    `/var/lib/peacock/`) if every file's size matches the bucket listing. Otherwise it copies the
    TPC-H tables and the TPC-DS tables, timing the copy. It checks `df` first and leaves 100 GB for
    Spark's spill. Then it uploads the local copy to the eu-west1 buckets (`--profile
    eu-west1`), and checks each object's size against eu-north1. Idempotent: an object already
    present at the right size is not uploaded again. Spark's S3 reads use `AWS_PROFILE=eu-west1`
    through the profile provider.
  - `comet_run.py --dataset tpch|tpcds --source s3|local --mode sequential|concurrent --batch K
    --runs R --warmup W --out DIR [--dump-plans DIR] [--matrix]`, run on the VM. One Spark
    application, one executor on all 128 cores, Comet's benchmark config plus the listed
    deviations. Concurrent means a FAIR scheduler, one pool and one thread per query. Event logs
    are on, compressed, kept on the VM. It writes `DIR/runs.tsv` (`query	run	ms`, plus a
    `batch` row per run), `DIR/conf.txt` (settings with keys matching
    `(?i)secret|password|token|access[._]?key` redacted, versions, host, local start time),
    `DIR/status` and `DIR/rc`. `--dump-plans` writes each query's final executed plan, its native
    plans and `fallbacks.tsv` (`query	comet_ops	spark_ops	reason`). `--matrix` prints the
    spec's configurations in priority order with their estimates, from `calibration.tsv` when it
    exists, and runs nothing.
    `prefix_sums(runs_tsv: Path, sizes: list[int]) -> dict[int, float]` gives sequential batch
    times as the summed per-query times of the first K queries, by run.
  - `comet_bytes.py EVENTLOG_DIR > bytes.tsv`:
    `dataset	query	run	scan_bytes	shuffle_bytes	spill_bytes	s3_gets	driver_ms	scheduler_ms`.

- [ ] **Step 1: Write the failing tests.**
  - `test_vm_log.sh`, with `nebius` and `ssh` stubbed on `PATH` and `COMET_VM_LOG` a temp file:
    - `use` on a STOPPED VM logs `use-requested` then `use started`; `stop-used` stops it;
    - `use` on a RUNNING VM logs `use found`; `stop-used` force-stops it too;
    - `use` twice on the same VM, then `stop-used`: one stop;
    - a stub that reports STOPPED at `stop-used` time: the log says `lost`, and no stop is called;
    - `hours` over two 30-minute use/stop pairs prints `1.00`; over a `lost` use, it stops at the
      last heartbeat;
    - the dead-man command is issued with the minutes left in the cap.
    - `vm.sh use h100` exits 1, calls no `nebius`, and logs nothing;
    - `push-aws` with a stubbed `ssh` sends the profile sections on stdin, and neither key value
      appears in the stub's argv or in `push-aws`'s output.
  - `test_comet_run.py`:
    - `--matrix` lists exactly nine runs: the seven of the spec, in order: TPC-H local sequential and concurrent
      "all", TPC-DS local sequential and concurrent "all", TPC-H S3 sequential and concurrent
      "all", TPC-DS S3 sequential; then the two in-memory-cache runs (Task 4, priority 5);
    - `prefix_sums` of a sequential `runs.tsv` gives batches 3, 10 and 20 as the sums of the
      first 3, 10 and 20 queries' times;
    - TPC-H `--batch 3` takes q1, q2, q3; TPC-DS `--batch 3` takes the first three lines of
      `tpcds-82.txt`, which has 82 lines;
    - `conf.txt` writing replaces a `fs.s3a.secret.key` value with `<redacted>`.
  - `test_comet_bytes.py`: on `fixtures/eventlog-small.json`, a hand-made two-stage event log,
    the sums equal the fixture's stage metrics.
- [ ] **Step 2:** Run them. Expected: FAIL, the scripts do not exist.
- [ ] **Step 3:** Write the scripts.
- [ ] **Step 4:** Run the tests. Expected: PASS. Then, as the only VM time in this task: `vm.sh use
  l40s`, arm the dead-man stop at 3 minutes, and confirm `vm.sh status l40s` shows STOPPED within
  10 minutes. If not, switch to the local fallback, and repeat on the L40S. Record the result in
  `-detail.md`.
- [ ] **Step 5:** Report a checkpoint `comet-research: experiment scripts`.

### Task 3: Experiment 1, VM-to-S3 throughput, on the L40S

**Files:** `measurements/s3/` (`<vm>.tsv`, `summary.tsv`, `notes.md`)

The CPU VM's row is measured at the start of Task 4, in the same session.

- [ ] **Step 1:** On `l40s` (the H100's measurement moved here, the human's decision): `vm.sh use`, `vm.sh push-aws l40s`, copy the script over, and run
  `s3_throughput.py` against lineitem's largest object at 1, 4, 16 and 64 threads. Then copy that
  object to local disk and read it with `--file`. When the VM was `found`, record its load next
  to each row: load average, `nvidia-smi` utilisation, NIC counters. `vm.sh stop-used`.
- [ ] **Step 2:** Write `summary.tsv` (best rate per VM and source) and `notes.md` (stated NIC
  rates, from `data-path.md`, the H100's included: the cost model's GPU node is the H100).
- [ ] **Step 3:** Report a checkpoint `comet-research: S3 throughput (L40S)`.

### Task 4: Experiments 1-3 on the CPU VM: S3 throughput, Comet, plan dumps

**Files:** `measurements/s3/cpu128.tsv`, `measurements/comet/<dataset>-<source>-<mode>-b<batch>/`,
`measurements/comet/{summary,bytes,skipped,calibration}.tsv`, `plans/{tpch,tpcds}/`

All of it is one session, inside the CPU VM's 2 h. The target split is about 20 min for setup
and Experiment 1, and about 100 min for the matrix.

- [ ] **Step 1:** `vm.sh use cpu128`, then `vm.sh push-aws cpu128`. Run `comet_setup.sh`
  detached and poll. Record in `-detail.md`: versions, the data's source (reused or copied, with
  copy time and rate), the upload time and rate, and free disk. If bucket creation or upload
  fails, record the error. The S3 runs then read eu-north1, every row is labelled
  `cross-region`, and Task 5 adds the modelled in-region correction. Then run Task 3 Step 1's
  measurements on the CPU VM against the eu-west1 copy, plus one 16-thread `cross-region` row
  against eu-north1.
- [ ] **Step 2: Calibration and plan dumps.** `comet_run.py --source local --mode sequential
  --batch all --runs 1 --warmup 0 --dump-plans plans/<ds>` for TPC-H, then TPC-DS. This pass is
  also each dataset's warm-up. Write `calibration.tsv` from its per-query times. Then print
  `--matrix` and write its projected skips into `-detail.md` before going on.
- [ ] **Step 3: The matrix**, in the spec's priority order, one configuration per detached run.
  Before each, check the cap. Poll until `rc` exists.
  1. TPC-H local: sequential, 2 measured runs; concurrent "all", 2 measured runs.
  2. TPC-DS local: sequential, 1 measured run; concurrent "all", 1 run. If "all" runs out of
     memory, that is recorded, and concurrent batch 20 runs instead.
  3. TPC-H S3: one warm-up query (q1), then sequential, 1 run; concurrent "all", 1 run.
  4. TPC-DS S3: sequential, 1 run.
  5. The coordinator's ruling after the desk wave (in `-detail.md`): TPC-H local, sequential and
     concurrent "all", 1 run each, with `spark.comet.exec.inMemoryCache.enabled=true` and the
     tables cached first, as a fairer batching baseline. Lowest priority, skipped under the cap.

  Sequential batches 3, 10 and 20 are prefix sums of the sequential run; nothing else runs for
  them (the human's decision).

  Run counts are the helper's call, to fit 2 h. The final plan of an S3 run must use the same
  `spark.comet.scan.impl` and the same Comet scan node as the local run. If it does not, record
  the configuration as `scan mismatch`, not as a NIC result. Local runs are labelled "page-cache
  warm".
- [ ] **Step 4:** Run `comet_bytes.py` on the VM over the event logs, and copy back `bytes.tsv`.
  Cross-check three queries per dataset: scan bytes against the sum of their projected columns'
  chunk sizes in the Parquet footers. Write `summary.tsv`: one row per configuration, mode and
  batch size, with the batch time and per-query times.
- [ ] **Step 5:** `vm.sh stop-used`. Copy the VM log into `measurements/`, and the hours into
  `## VM hours`. Report a checkpoint `comet-research: Comet runs and plan dumps`.

### Task 5: Analysis and the cost model

**Files:** `split.md`, `translatability.md`, `cost_model.py`, `test_cost_model.py`, `inputs.tsv`,
`cost.md`, `design.md`

**Interfaces:**
- Consumes: Task 1's five files, `measurements/` (`summary.tsv`, `bytes.tsv`, `s3/summary.tsv`),
  `plans/`.
- Produces: `inputs.tsv`: `name	value	low	high	unit	tag	source`. `cost_model.py` exposes
  `ratio(inputs, dataset, workload, batch, baseline, deployment, data_bytes, tier="nic") -> Band`,
  where `Band` has `low`, `mid`, `high`, `sharing_factor` and `device_factor`. The arguments take:
  `dataset` `tpch` or `tpcds`; `workload` `batched` or `one-pass`; `baseline` `sequential` or
  `concurrent`; `deployment` `1x128`, `8x16` or `16x8`; `tier` `nic`, `local-nvme`, or a tier
  name from `exotic.md`. `sensitivity(...)` takes the same arguments and returns rows of
  `(input, ratio_at_low, ratio_at_high)`, sorted by swing.

- [ ] **Step 1 (one subagent):** given `comet-internals.md`, `plans/` (with `fallbacks.tsv`),
  `peacockdb-core/src/plan/mod.rs` and `architecture.md`'s node set, write two files:
  - `translatability.md`: every operator and expression in the final plans, classed supported,
    adaptable or missing, with counts per query and overall, and the cost of the gap from
    DataFusion 45 to Comet's version;
  - `split.md`: the per-operator placement rule (scan, decode, filter, project, join, aggregate,
    shuffle write and read, Spark-exact expressions), fallback inside one stage, and each query's
    GPU-eligible fraction of its operator time. The fractions are rows Task 5 Step 4 reads.
- [ ] **Step 2: Write the failing tests** in `test_cost_model.py`:
  - `test_nic_bound_scan`: 40 GB over 5 GB/s, with GPU compute 1 s and no other term, is 8 s.
  - `test_spark_overhead_floor`: a query with zero bytes and zero compute still costs its
    `spark_overhead`.
  - `test_sharing_uses_computed_overlap`: two queries reading 10 GB and 6 GB, with union 12 GB,
    move 12 GB in a batch.
  - `test_ratio_factors`: `sharing_factor × device_factor == mid`.
  - `test_parity_pricing`: one H100 node costs the same per hour as eight 16-core nodes and as
    sixteen 8-core nodes, whose total NIC is 8× and 16× the per-node rate.
  - `test_untagged_input_rejected`: a tag outside the three raises `ValueError`.
  - `test_band_from_corners`: with one input that raises GPU time and one that raises CPU time,
    the band equals the min and max over the four corners, not "all low" and "all high".
  - `test_sensitivity_order`: the input with the largest swing comes first.
  - `test_fits_tier`: 60 GB fits VRAM, 150 GB host RAM, 1 TB local NVMe, 10 TB none, on the H100
    node (80 GB VRAM, 200 GB RAM, the local NVMe tier's capacity).
  - `test_one_pass`: `one-pass` is the median, and separately the worst, of the sequential
    run's per-query ratios.
- [ ] **Step 3:** Run `python3 -m pytest llm-wiki/artifacts/comet-research/test_cost_model.py`.
  Expected: FAIL, no `ratio`.
- [ ] **Step 4:** Write `cost_model.py` and fill `inputs.tsv`. The model, per query q, stated in
  `cost.md`'s header:
  - GPU time:
    `spark_overhead(q) + max(wire_bytes(q)/rate(tier), load_bytes(q)/decode_rate, compute(q)) + shuffle_bytes(q)/NIC + cpu_part(q)/(16-core rate)`.
  - `spark_overhead` is `driver_ms` plus `scheduler_ms` from `bytes.tsv` (`measured`).
  - `compute(q)` is the measured sf40 execute time where `benchmark-minimal.md` has one (H200,
    scaled to H100 by the HBM rate). Elsewhere it is Comet's local time × the GPU-eligible
    fraction ÷ a speedup band that spans the measured queries' DuckDB-to-GPU ratios (`modelled`).
  - `cpu_part(q)` is the non-eligible fraction from `split.md`.
  - `wire_bytes` comes from `bytes.tsv`. A batch moves the union of its queries' scan bytes;
    overlap is computed from `bytes.tsv` and the plans' column sets. SCOPE's overlap from
    `batching.md` appears only as a "realistic ETL" sensitivity row.
  - The CPU side uses the measured times, in-region. Only if Task 4 fell back to cross-region
    reads does the S3 rate get an in-region correction, a separate `modelled` input. CPU scaling past sf40 uses `data-path.md`'s published Comet results;
    spill begins where data per node exceeds executor memory. `8x16` and `16x8` are modelled
    from the 1 × 128 measurements and per-node NIC rates.
  - S3 GET counts and egress bytes from `bytes.tsv` are listed beside each ratio, not folded in.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Generate `cost.md` with `python3 cost_model.py > cost.md`, never by hand:
  - first, the headline cells by the spec's verdict rule, with bands and the verdict each implies;
  - ratio tables for batched (3, 10, 20, all) and one-pass (median, worst), against both
    baselines for "all" and the sequential baseline for 3, 10 and 20, for each deployment, by data size (sf40, sf200, 1 TB, 10 TB), each size's tier
    marked, with the sharing and device factors;
  - a row "CPU with shared scans" (modelled): the CPU baseline given the same sharing;
  - the batched table again for the local NVMe tier and each `exotic.md` tier, at its price;
  - per TPC-H query, the NIC rate below which the GPU stalls;
  - the sensitivity table for each headline cell; its top row is the settling measurement.
- [ ] **Step 7 (one subagent):** given all artifacts, write `design.md`. On a go or conditional
  go: the alpha design (seam, IR and translation, `split.md`'s rule, the data path and cache,
  which local-SSD layout first, whether an exotic tier changes it, the batching API, and the
  whole-batch CBO for probe-side holds after query-fuser's item 1a, placed relative to Catalyst,
  AQE and Comet), sized by component against `inventory.md`'s yardstick, naming the hard part.
  On a no-go: the conditions for a go. Ten-line reply.
- [ ] **Step 8:** Report a checkpoint `comet-research: analysis and cost model`.

### Task 6: The report and its audit

**Files:** `llm-wiki/reports/comet-gpu.md`, `audit.md`

- [ ] **Step 1:** Write the report in the spec's order:
  - the verdict, from `cost.md`'s headline cells, with bands;
  - the cost model, its inputs table (tags shown), and its sharing and device factors;
  - groups 1-5, with the exotic architectures;
  - the design, or the conditions for a go;
  - VM hours per VM, and every configuration skipped, cut or with a scan mismatch;
  - follow-up experiments collected from every artifact;
  - sources.

  Each section summarises and links its artifact, and every factual sentence keeps its source
  and date.
- [ ] **Step 2 (one fresh subagent, given the report and the artifact folder only):** it writes
  `audit.md` and replies in ten lines. Its checks:
  - open 15 cited sources at random and say whether each supports its sentence;
  - recompute both headline cells and one NIC-stall row by hand from `inputs.tsv`, and compare
    them with `cost.md`;
  - check every `measured` row in `inputs.tsv` against a file under `measurements/`;
  - say whether the CPU baseline was fair: config, both modes, scan parity, and whether its S3
    reads were in-region;
  - say whether a `modelled` input is presented as measured, and whether a `spark-plugin.md`
    fact went unchecked.
- [ ] **Step 3:** Fix every finding. Re-generate `cost.md` if inputs changed, and re-run the
  Task 2 and Task 5 tests: PASS.
- [ ] **Step 4:** `vm.sh stop-used`, then `vm.sh status` for all three VMs. Every VM the log shows
  used is STOPPED, or used by someone else since our stop (its log shows our `stop`). Copy the
  VM log into `measurements/`. Mark every task done in `-detail.md`.
- [ ] **Step 5:** Report green: the verdict, the headline ratios with bands, the design's
  headline or the conditions for a go, VM hours per VM, and the report's path.
- [ ] **Step 6 (coordinator):** Write the signoff from `audit.md` at the end of the spec, and
  mark the task `done` on the board. Commit `comet-research: report`, push, and open the PR against
  master with the green report as its body and the PR attribution footer.
