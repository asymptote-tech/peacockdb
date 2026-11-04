# comet-research Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One report, `llm-wiki/reports/comet-gpu.md`. It says go, conditional go or no-go on a
GPU backend for Apache Comet against the 2× cost-per-query bar. On a go, it gives the alpha design.

**Architecture:** Chain M's only task, `Kind: prototype` with a PR and no reviewer, in workspace
alpha on `ENS-comet-research`. The developer runs every step. It dispatches desk research and
analysis to its own `general-purpose` subagents. Each subagent writes one artifact file under
`llm-wiki/artifacts/comet-research/` and returns at most ten lines. The developer runs the
experiments itself, in the foreground, through a small VM wrapper that logs every start and stop
and enforces the VM-hour caps. A tested cost model turns measured, published and modelled inputs
into the verdict. The coordinator owns every git write and the PR.

**Tech Stack:** Agent tool (`general-purpose`), `anthropic-skills:deep-research`, WebSearch and
WebFetch, `gh`, the `nebius` CLI, `scripts/gpu-host.sh` (L40S), the AWS CLI (default profile),
Apache Spark standalone with Comet, Python 3 with pytest.

**Spec:** [`comet-research.md`](comet-research.md). Read it first. This plan argues from it.

## Global Constraints

- **Roles.** A step ending "report a checkpoint `<msg>`" is a checkpoint, not the task's green:
  the developer stops and reports; the coordinator commits with that message, pushes, and
  dispatches the developer at the next step. The developer never runs a git write. Only Task 7's
  last step is green: the coordinator opens the PR against master and marks the task `done`. No
  reviewer rounds, no completeness pass and no wait on CI (spec: prototype with a PR). The
  coordinator writes the signoff from `audit.md`.
- **No peacockdb executor runs.** GPU-side times come from existing measurements and models.
  Nothing under `peacockdb-core/`, `peacockdb-ffi/`, `cpp/`, `flatbuffers/`, `docker/` or
  `scripts/` changes. Experiment scripts live in `llm-wiki/artifacts/comet-research/scripts/`.
- **Never open a Nebius credential file** (`~/.nebius/config.yaml`, `credentials.yaml`, any
  `*.pem` under `~/.nebius/`, on any host): no `cat`, `grep`, `sed`, Read or redaction. Learn the
  CLI from `nebius … --help`. S3 is the default AWS profile with `--endpoint-url
  https://storage.eu-north1.nebius.cloud:443`. Never print `~/.aws/credentials` or a key.
- **Hosts:** L40S through `scripts/gpu-host.sh`. H100 `computeinstance-e00e6yena4nzjgaym0`
  (eu-north1, profile `editor-eu`) and CPU VM `computeinstance-e01t2mm4265y8x88ey` (eu-west1,
  profile `eu-west1`) through the `nebius` CLI. Login `dmitry`, the default ssh key.
- **Stop what you started, never what you found running.** `vm.sh` records each VM it starts.
  `vm.sh stop-mine` stops exactly those. A VM already RUNNING when a session begins may be chain
  L's: use it, never stop it. The developer runs `stop-mine` at the end of every measurement
  session. The coordinator runs it before it exits for any reason, and after any developer
  dispatch that ends without a report.
- **VM-hour caps, summed over the task:** CPU VM 6 h, H100 1 h, L40S 1 h. `vm.sh hours <vm>`
  is the count. Before each configuration, if hours used plus the configuration's estimate
  exceed the cap, record it as skipped and go on.
- **VM sessions run in the foreground**, never `run_in_background`. A backgrounded chain gets
  killed mid-run with no error.
- **Conflict with chain L is accepted.** A run cut short by a VM stopping under it is re-run once
  and noted in `-detail.md`.
- **Bar:** at least 2× lower cost per query than Comet on CPU. One H100 node (16 cores, 200 GB
  RAM) costs the same as one 128-core, 512 GB node. Small CPU nodes are priced linearly by cores.
  Cost per query is VM-hours at that parity. S3 charges and idle time are listed beside it.
- **Workload:** batched ETL first (batch sizes 3, 10, 20, all), one-pass single job as the floor.
  The CPU baseline runs each batch one after another and concurrently. Both ratios are quoted.
- **A verdict whose error band straddles 2× is a conditional go**, naming the settling
  measurement.
- Every factual claim carries a link and `(accessed YYYY-MM-DD)`. A claim about code carries the
  commit SHA it was read at. Every measurement gives host, local date and time, and method. Every
  cost-model input is tagged `measured`, `published` or `modelled`. Guesses are marked
  `(estimate)`.
- External repos (Comet, spark-rapids, Gluten, Auron) are cloned under `~/comet-research/ext/` on
  the local host, outside every worktree, never into the tree.
- A subagent returns at most ten lines: its verdict and its file path. The writing is plain:
  short sentences, ordinary words.

## Review Focus

1. **A ratio manufactured by the baseline.** A weak CPU baseline (sequential only, S3 read
   across regions, a mistuned Spark) makes 2× easy. Pinned in Task 5 (both modes, `conf.txt`
   records the tuning), Task 6 (the cross-region correction is a separate, tagged input) and
   Task 7's audit.
2. **Modelled GPU numbers presented as measured.** Pinned by the input tags in Task 6, which a
   test enforces, and the report's inputs table.
3. **`spark-plugin.md` repeated rather than checked.** Every fact taken from it is re-verified at
   the pinned Comet commit. Pinned in Task 1's prompt and Task 7's audit.
4. **A VM left running, or chain L's VM stopped.** Pinned by `vm.sh`'s started-by-me log, tested
   in Task 2, by `stop-mine` at the end of every experiment task, and by Task 7's final check.
5. **The developer's window filling with bulk** (plan dumps, raw timings). Raw output goes to
   files; the developer reads summaries. `-detail.md` is written after every task, so a
   restarted coordinator resumes from it.

---

## Artifact layout (the interface every task shares)

    llm-wiki/artifacts/comet-research/
      comet-internals.md      Task 1: group 2, and group 3's seam
      prior-art.md            Task 1: group 1, and group 4's "how others handle it"
      data-path.md            Task 1: group 4's survey: cloud instances, local SSD, wire savings
      batching.md             Task 1: group 5's API options and prior art, SCOPE overlap numbers
      exotic.md               Task 1: group 4's exotic architectures, with the tier table
      scripts/                Task 2: vm.sh, s3_throughput.sh, comet_setup.sh, comet_run.py, tests/
      measurements/vm-log.tsv Task 2 on: every VM event, the source of the caps
      measurements/<exp>/     Tasks 3-4: raw output, one dir per experiment, plus summary.tsv
      plans/{tpch,tpcds}/     Task 4: Comet's serialized plans, one file per query
      translatability.md      Task 5: the classification and counts
      design.md               Task 5: split rule, seam, IR, data path, batching API, CBO
      cost_model.py, test_cost_model.py, inputs.tsv, cost.md   Task 5
      audit.md                Task 6

Every findings file is at most 300 lines, opens with a five-line `## Summary`, and ends with
`## Follow-up experiments` and `## Sources`.

---

### Task 0: Set up the run

**Files:** `llm-wiki/artifacts/comet-research/` (with `scripts/`, `measurements/`, `plans/`);
`llm-wiki/tasks/comet-research-detail.md`

- [ ] **Step 1:** Create the folders. Write `-detail.md`: one line per task (`Task N: todo`), the
  clone directory, and empty `## VM hours` and `## Rulings` headings.
- [ ] **Step 2:** Clone into `~/comet-research/ext/`: `apache/datafusion-comet`,
  `NVIDIA/cudf-spark`, `apache/gluten`, `apache/auron`. Pin Comet to its latest release tag.
  Record each HEAD SHA, and Comet's tag with its Spark, DataFusion and Arrow versions (from
  `pom.xml` and `native/Cargo.toml`), in `-detail.md`.
- [ ] **Step 3:** Report a checkpoint `comet-research: start the run`.

### Task 1: Desk wave (five parallel subagents)

**Files:** `comet-internals.md`, `prior-art.md`, `data-path.md`, `batching.md`, `exotic.md`

- [ ] **Step 1:** Dispatch all five at once, each with the prompt template below.
  - **comet-internals:** spec group 2 in full, plus group 3's first and last bullets: the exact
    seam where a GPU executor could take a plan fragment, what it receives and returns, and how
    GPUs map onto executors and tasks. Read from the Comet clone at the pinned SHA, cited by
    file and line. Include the protobuf schema's operator and expression lists, as a table
    Task 5 can classify. Say how to make Comet write each query's serialized plan to a file,
    for Task 4.
  - **prior-art:** spec group 1, and group 4's "how others handle it": each accelerator's data
    path (reader, shuffle, cache, NIC handling). Speedata, spark-rapids, Gluten and Velox-cuDF,
    Auron and Photon at least. Any GPU or hardware effort on Comet itself, from issues, PRs, the
    mailing list and talks.
  - **data-path:** spec group 4's NIC, wire, RAM-cache and local-SSD bullets. A table of cloud
    GPU and CPU instances (AWS, GCP, Azure, Nebius) with cores, RAM, NIC, local NVMe (count,
    rated and published measured read rates), GDS support and on-demand price. The wire savings
    (compression with GPU decompression, pruning, NVMe cache, shuffle volume), each with a
    published number. The local-SSD product-option bullet in full, with its three layouts and
    evidence for each. Whether a RAM cache is possible inside Comet and Spark (Comet's
    experimental cache, Spark's cache manager and BlockManager, executor lifetime under dynamic
    allocation).
  - **batching:** spec group 5's API, CBO-placement and prior-art bullets. Microsoft's SCOPE
    work (CloudViews, BigSubs and successors): the measured overlap between production jobs, as
    numbers the cost model can use. What Spark already shares within and across queries.
  - **exotic:** spec group 4's exotic-architectures bullet in full. Add CXL vendors (Samsung,
    Astera Labs, Liqid, UnifabriX, Panmnesia), product-research's Grace option file
    (`llm-wiki/artifacts/product-research/options/`), and Spark's fabric paths (spark-rapids' UCX
    shuffle, SparkRDMA). End with the tier table the cost model reads:
    `tier	GB_per_s_to_GPU	capacity_GB	price_per_hour	cloud|on-prem	source`.
- [ ] **Step 2:** As each returns, append its ten lines to `-detail.md`.
- [ ] **Step 3: Verify**, in the artifact folder:
  `for f in comet-internals prior-art data-path batching exotic; do for h in Summary Sources
  'Follow-up experiments'; do grep -qF "## $h" $f.md || echo "$f: $h"; done; grep -q 'accessed
  20' $f.md || echo "$f: no dates"; done` prints nothing, and `exotic.md` has the tier table's
  header row. Re-dispatch the owner for any gap.
- [ ] **Step 4:** Report a checkpoint `comet-research: desk wave`.

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

**Files:** `scripts/vm.sh`, `scripts/s3_throughput.sh`, `scripts/comet_setup.sh`,
`scripts/comet_run.py`, `scripts/tests/test_vm_log.sh`, `scripts/tests/test_comet_run.py`

**Interfaces:**
- Consumes: `comet-internals.md`'s recipe for writing serialized plans to files.
- Produces:
  - `vm.sh <h100|cpu128|l40s> <status|start|stop|ip|ssh <cmd…>>`, `vm.sh stop-mine`,
    `vm.sh hours <vm>`. l40s delegates to `scripts/gpu-host.sh`. The other two call `nebius
    --profile <p> compute instance {get,start,stop} --id <id>` and take the IP from
    `network_interfaces[].public_ip_address`, as `gpu-host.sh` does. ssh uses a per-instance
    `HostKeyAlias` (`comet-<vm>`). Exit codes as `gpu-host.sh`'s (0, 75, 69, 1).
  - `measurements/vm-log.tsv`, appended by `vm.sh` only: `local_time	vm	event`, event one of
    `start` (this chain started it), `found-running` (it was already up), `stop`. A VM is
    "mine" when its last event is `start`. `stop-mine` stops each such VM and logs `stop`.
    `hours <vm>` sums start-to-stop intervals, counting an open `start` up to now. The log path
    is overridable by `COMET_VM_LOG`, for the test.
  - `s3_throughput.sh <bucket> <prefix> <streams>`: run on a VM, prints one TSV row:
    `vm	streams	bytes	seconds	GB_per_s	source(s3|local)`.
  - `comet_setup.sh`: on the CPU VM, installs Java, the pinned Spark and Comet, and copies both
    sf40 datasets to local disk. Idempotent.
  - `comet_run.py --dataset tpch|tpcds --source s3|local --layout 1x128|8x16
    --mode sequential|concurrent --batch K --runs R --out DIR`. It starts a Spark standalone
    application with the layout's executors (8 × 16 pins each executor to its own 16 cores with
    `taskset`). It writes `DIR/runs.tsv` (`query	run	ms`, plus one `batch` total row per run)
    and `DIR/conf.txt` (every non-default Spark and Comet setting, versions, host, local start
    time). `--dump-plans DIR` writes each query's serialized Comet plan and its Comet-annotated
    explain text. `--matrix` prints the spec's configurations in priority order, one per line,
    with its time estimate, and runs nothing.

- [ ] **Step 1: Write the failing tests.**
  - `test_vm_log.sh`, with `nebius` replaced by a stub on `PATH` and `COMET_VM_LOG` set to a temp
    file:
    - a VM the stub reports STOPPED, then `start`: the log's last line is `start`, and
      `stop-mine` stops it;
    - a VM the stub reports RUNNING, then `start`: the log says `found-running`, and `stop-mine`
      calls no stop on it;
    - `hours` over a log of two start/stop pairs of 30 min each prints `1.00`.
  - `test_comet_run.py`: `--matrix` lists TPC-H local (both layouts) first and TPC-DS 8 × 16
    last, in the spec's order; `sequential` with `--batch 3` takes q1-q3 of the list order.
- [ ] **Step 2:** Run both. Expected: FAIL, the scripts do not exist.
- [ ] **Step 3:** Write the four scripts.
- [ ] **Step 4:** Run both tests. Expected: PASS. Then `vm.sh h100 status` and `vm.sh cpu128
  status` each print a state and log nothing.
- [ ] **Step 5:** Report a checkpoint `comet-research: experiment scripts`.

### Task 3: Experiment 1, VM-to-S3 throughput

**Files:** `measurements/s3/` (`<vm>.tsv`, `summary.tsv`, `notes.md`)

- [ ] **Step 1:** For each VM in turn (`l40s`, `h100`, `cpu128`): `vm.sh <vm> start`, copy the
  script over, run `s3_throughput.sh tpch-sf40 <lineitem's prefix> <n>` for n = 1, 4, 16, 64, and
  once against local disk. Record the provider's stated NIC rate for the instance type in
  `notes.md`. `vm.sh stop-mine` after each VM, except the CPU VM when Task 4 starts at once.
- [ ] **Step 2:** Write `summary.tsv`: best rate per VM and source. Mark the CPU VM's S3 rate
  `cross-region`. Copy each VM's hours so far into `-detail.md`'s `## VM hours`.
- [ ] **Step 3:** Report a checkpoint `comet-research: S3 throughput`.

### Task 4: Experiments 2 and 3, Comet on the CPU VM, and plan dumps

**Files:** `measurements/comet/<dataset>-<source>-<layout>-<mode>-b<batch>/`,
`measurements/comet/summary.tsv`, `measurements/comet/skipped.tsv`, `plans/{tpch,tpcds}/`

The matrix is the spec's, in its priority order (`comet_run.py --matrix`):
- TPC-H q1-q22 on `tpch-sf40` (its TPC-H columns only); TPC-DS corpus queries on `tpcds-sf40`.
  The TPC-DS list order is the corpus order in `testdata/` (the dataset's query files, sorted).
- Per configuration: `sequential` once with all queries in list order (batch sizes 3/10/20 are
  its prefix sums, plan's call), and `concurrent` per batch size 3/10/20/all.
- One warm-up and two measured runs (agreed with the human); the faster measured run is reported.
- A query Comet fails on or falls back to Spark for is recorded with the reason, not dropped.

- [ ] **Step 1:** `vm.sh cpu128 start`. Run `comet_setup.sh`. Record versions and local-disk
  free space in `-detail.md`.
- [ ] **Step 2:** Plan dumps: `comet_run.py --layout 1x128 --source local --mode sequential
  --runs 1 --dump-plans plans/<ds>` for both datasets. **Verify:** one plan file per query that
  ran, and failures listed in `plans/<ds>/failed.txt`.
- [ ] **Step 3:** Run the matrix in priority order, in the foreground, one configuration per
  call. Before each, check the cap (Global Constraints) and write a skipped one to
  `skipped.tsv`. After each, check `runs.tsv` has the expected rows, and append one line to
  `-detail.md`.
- [ ] **Step 4:** Write `summary.tsv`: one row per configuration, mode and batch size, with the
  batch time, and per-query times from the sequential run. Note each fallback and its reason,
  from Comet's explain output.
- [ ] **Step 5:** `vm.sh stop-mine`. Copy the hours into `## VM hours`. Report a checkpoint
  `comet-research: Comet runs and plan dumps`.

### Task 5: Analysis and the cost model

**Files:** `translatability.md`, `design.md`, `cost_model.py`, `test_cost_model.py`,
`inputs.tsv`, `cost.md`

**Interfaces:**
- Consumes: Task 1's five files, `measurements/*/summary.tsv`, `plans/`.
- Produces: `inputs.tsv` with columns `name	value	low	high	unit	tag	source`. `tag` is one of
  `measured`, `published`, `modelled`; `source` is a link, a file path, or the model's one-line
  formula. `cost_model.py` exposes
  `ratio(inputs: dict, workload: str, batch: int, baseline: str, data_bytes: float, tier: str = "nic") -> tuple[float, float, float]`
  (low, mid, high), with `workload` in `{batched, one-pass}`, `baseline` in
  `{sequential, concurrent}`, and `tier` either `nic` or a tier name from `exotic.md`.

- [ ] **Step 1 (subagent, translatability):** given `comet-internals.md`'s operator and
  expression table, `plans/`, `peacockdb-core/src/plan/mod.rs` and `architecture.md`'s node set.
  It classifies every operator and expression occurring in the dumps as supported, adaptable or
  missing, with counts per query and overall, and says what the gap from DataFusion 45 to
  Comet's version costs. Output `translatability.md`, ten-line reply.
- [ ] **Step 2: Write the failing tests** in `test_cost_model.py`:
  - `test_nic_bound_scan`: 40 GB over 5 GB/s with GPU work at 1 s gives 8 s, the max of the
    wire, PCIe and compute terms.
  - `test_batching_divides_wire_bytes`: with full overlap, a batch of 10 moves the shared table's
    bytes once, not ten times.
  - `test_overlap_fraction_scales_sharing`: with overlap 0.5, half the scan bytes are shared.
  - `test_parity_pricing`: one H100 node costs the same per hour as eight 16-core nodes, whose
    total NIC is eight times one node's.
  - `test_untagged_input_rejected`: an input whose tag is not one of the three raises
    `ValueError`.
  - `test_band_ordering`: `low <= mid <= high` for every workload, batch, baseline and tier.
  - `test_fits_tier`: a data size maps to the tier it fits (VRAM 80 GB, host RAM 200 GB, local
    disk, none) on the H100 node.
- [ ] **Step 3:** Run `python3 -m pytest llm-wiki/artifacts/comet-research/test_cost_model.py`.
  Expected: FAIL, no `ratio`.
- [ ] **Step 4:** Write `cost_model.py` and fill `inputs.tsv`. GPU per-query times come from
  `benchmark-minimal.md` (H200 sf40, scaled to H100 by HBM and PCIe rates, tagged `modelled`) and
  `inventory.md`'s PCIe and decode rates. The CPU VM's S3 rate gets an in-region correction as a
  separate `modelled` input. Overlap comes from `batching.md`'s SCOPE numbers.
- [ ] **Step 5:** Run the tests. Expected: PASS.
- [ ] **Step 6:** Generate `cost.md` with `python3 cost_model.py > cost.md`, never by hand:
  - ratio tables for batched (3, 10, 20, all) and one-pass, against both baselines, at parity
    with one 128-core node and with eight 16-core nodes, by data size (sf40, sf200, 1 TB,
    10 TB), each size's tier marked;
  - the batched table again for each tier in `exotic.md`, at that tier's price;
  - per TPC-H query, the NIC rate below which the GPU stalls: the rate at which that query's
    bytes take as long to arrive as the GPU takes to process them.
- [ ] **Step 7 (subagent, design):** given all artifacts so far, it writes `design.md`. On a go
  or conditional go, the alpha design: seam, IR and translation, the CPU/GPU split rule, the data
  path and cache (which local-SSD layout first, and whether an exotic tier changes it), the
  batching API, and the whole-batch CBO for probe-side holds (buffer against re-read, after
  query-fuser's item 1a) and where it sits relative to Catalyst, AQE and Comet. Sized by
  component (reuse, adapt, new) against `inventory.md`'s yardstick, naming the hard part. On a
  no-go, the conditions for a go. Ten-line reply.
- [ ] **Step 8:** Report a checkpoint `comet-research: analysis and cost model`.

### Task 6: The report and its audit

**Files:** `llm-wiki/reports/comet-gpu.md`, `audit.md`

- [ ] **Step 1:** Write the report in the spec's order: the verdict with both ratios (batched
  ETL, one-pass) and their bands, the cost model and its inputs table (tags shown), groups 1-5
  with the exotic architectures, the design or the conditions for a go, the VM hours used and any
  configuration skipped under a cap, follow-up experiments collected from every artifact,
  sources. Each section summarises and links its artifact. Every factual sentence keeps its
  source and date.
- [ ] **Step 2 (one fresh subagent, given the report and the artifact folder only):** open 15
  cited sources at random and say whether each supports its sentence. Say whether the CPU
  baseline was fair (tuning, both modes, both layouts, the cross-region correction), whether any
  `modelled` input is presented as measured, whether a `spark-plugin.md` fact went unchecked,
  and whether `cost.md` gives the stated verdict. Writes `audit.md`, ten-line reply.
- [ ] **Step 3:** Fix every finding. Re-generate `cost.md` if inputs changed, and re-run the
  tests of Tasks 2 and 5: PASS.
- [ ] **Step 4:** `vm.sh stop-mine`. Then `vm.sh <vm> status` for all three: every VM whose last
  log event is `start` or `stop` by this chain shows stopped. Mark every task done in
  `-detail.md`.
- [ ] **Step 5:** Report green: the verdict, both ratios with bands, the design's headline or
  the conditions for a go, VM hours per VM, and the report's path.
- [ ] **Step 6 (coordinator):** Commit `comet-research: report`, push, open the PR against
  master with the green report as its body, write the signoff from `audit.md` at the end of the
  spec, and mark the task `done`.
