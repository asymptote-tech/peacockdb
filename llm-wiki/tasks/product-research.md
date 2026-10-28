# Choose the product to build on peacockdb

Kind: research. It runs in a helper session on master, with subagents, and is not a chain task.
There is no branch, no PR and no reviewer. The only output is one report.

## Why

The human must choose **one product** to build on the peacockdb engine. It may build on
Sirius, which peacockdb overlaps with heavily. The report is the basis for that choice. The
human reads it alone, then uses it to explain the choice and its trade-offs to partners, a
design partner (a future customer) and contributors. So every trade-off must be stated plainly
enough to repeat, with its evidence.

The right product is the one that **1–2 users need badly today**, in a niche, so that a small
funding round (YC-sized) becomes possible. CUDA acceleration may grow large later. That is the
long-term story, not what wins the first user. Market-size arguments count for little. A named
group of people with a painful problem counts for a lot.

**No design partner exists yet.** So finding candidates is a main job of the demand research.
The report names the user groups, and where possible the companies or public threads, that
look like the best first conversations.

GPU acceleration stays central to the product (helper's call). Discovery stays within it and
does not range into non-GPU niches.

## Budget

The human plus 1–2 weekend contributors, LLM-assisted, about **six months**. The new
engineering may be at most 1–2× what the current engine contains. **What must exist at six
months is an alpha version**: a product that a first user can run on their own data.
Production hardening is out of scope. Size every option to an alpha.

Do not size the existing engine from git history. The project began as a prototype and took
several unnecessary turns, so the history overstates the effort. Size each option by
components instead (helper's call): what it reuses from peacockdb or Sirius as is, what it
adapts, and what it builds new. Express the new work as a fraction of the current engine.

## The hypothesis to test, not to confirm

The human's lean, explicitly a gut feeling:

- **Core:** a smart RAM/VRAM cache across several GPUs on one host, answering queries over
  ~100 GB in a few hundred milliseconds.
- **Packaging:** a plugin inside a database people already run, as Sirius is for DuckDB. Not a
  standalone database that makes users ingest data first.
- **Why:** the belief that Voltron Data shut down because it built a standalone engine that
  required ingestion.

The report must test each part against evidence, including why Voltron Data really failed,
from sources. It must judge the lean by the same criteria as every other option. A report that
merely confirms the lean fails, whatever it recommends.

## Options

**The search must be comprehensive.** The list below is the human's starting point and is
known to be incomplete. A discovery pass looks for directions the human did not name. One
example is the other places where GPU SQL acceleration is sold or requested. Every option it
finds that passes a quick screen against the criteria is evaluated in the same depth as the
named ones. Options that fail the screen are listed with the reason.

Named options, with the human's questions for each:

1. **ClickHouse GPU plugin.** How would Arrow come out of MergeTree parts?
2. **GPU accelerator for data lakes, Iceberg first.** How should data be ingested from the lake
   properly?
3. **Real-time ingestion plus fast analytics.** What would it take to cache data partly in
   VRAM? Compare storing compressed segments with storing uncompressed data before RLE
   decoding.
4. **Streaming analytics,** with Flink as the example and others where relevant.
5. **DataFusion plugin,** to work inside other DataFusion-based systems.
6. **Other systems, especially real-time ones.** VeloDB, for example.
7. **A distributed engine with pluggable infrastructure** in which peacockdb serves as the GPU
   accelerator. Databend, for example.
8. **Spark plugin.** The human's study a year ago (below) found spark-rapids' Scala readers
   ate the GPU's whole advantage. Is that fixed? What do users say? The human's thesis: a
   distributed engine needs a RAM cache for the GPU to pay off, because NIC and SSD throughput
   would otherwise be the bottleneck. Did spark-rapids build one? Test this thesis for **every**
   option, not just Spark.
9. **(Exotic) the cuDF layer rebuilt for other accelerators,** such as AMD or Cerebras.
10. **Combined vector and SQL analytics.** Is there demand? Are there vector joins?

## Sirius and NVIDIA

The human's stance: NVIDIA backs Sirius and denies competing in this field. If that holds,
the product may build on Sirius. A fork is presumed out because NVIDIA develops Sirius
actively. **Verify both claims rather than accept them.** Use public statements, licences,
contributor and commit activity, release cadence, hiring and roadmap, not reassurances.

**Dropping most of peacockdb's engine in favour of Sirius is acceptable,** but only after the
trade-offs are weighed. Two of them are the human's knowledge of peacockdb's code and the
freedom to evolve code they own. So each option is costed both ways, on peacockdb and on Sirius,
wherever both are plausible.

Then compare peacockdb with Sirius as due diligence for building on it:

- The cost-based optimizer: does Sirius have one, and how does it work?
- The RAM/VRAM cache.
- Spilling to host memory: how does it work?
- Substrait: what exactly does Sirius take as Substrait? Can a system other than DuckDB produce
  that plan and have Sirius execute it?
- API surface: can it execute a narrow plan fragment, or call the optimizer narrowly?

Also: **what other open-source projects NVIDIA runs in this space,** and how each bears on
the options.

## User demand

Are people requesting or using GPU acceleration? Look at the GitHub issues and discussions of
popular systems and at their product forums and community channels. Cover at least those in
the option list. For users who already accelerate: how, and with what? For requests: what use
case? Name concrete threads, with links and dates. Count requests where a count is
meaningful, and say when the evidence is thin.

## Evidence already in hand

- **The spark-rapids study.** Two commits by the human on spark-rapids 25.08, in
  `~/repo/spark-rapids`. `934c5ab93` adds a build script. `99a1d01b7` instruments
  `GpuMultiFileReader.scala`, `GpuParquetScan.scala` and `GpuFileScanRDD.scala`. The
  instrumentation showed the Parquet readers spending about 10× more time in Scala code than in
  executing the nodes. The collected numbers are lost.
- **"TPC-H Results" spreadsheet,** `10kT2kzhNBzP0rHRMoF9JZNWNY0uTkQ8Hm75DWcl_sgM`. On tab
  `SF200`, Q1 is the one spark-rapids cell that survived: 22.6 s. Compare 1.8 s plus 1.0 s
  loading for the human's H100 hand-planned PoC, and 2.2 s for DuckDB on 64 cores. The tab
  also holds DuckDB on 16, 32 and 64 cores, and Voltron Data on SF1000 with 2×A100, scaled. The
  other tabs: hardware cost, per-size DuckDB runs, and ClickHouse at SF20.
- **"spark-rapids versions" spreadsheet,** `1pOgRm0mE8KhRj17GJltqM5lPxfq3ZFPDZ1dJ5II9RKA`. The
  setup tested: one H100, Spark 3.5.6, spark-rapids 25.06, JDK 17.
- **Reading the sheets:** the Sheets API with the service-account key
  `~/.config/gcloud/sheets-writer-key.json`, read-only scope, in a scratch venv
  (`google-api-python-client`, `google-auth`). The account can see other, private spreadsheets
  of the human. **Open only these two.**
- **The engine itself.** The map is [`architecture.md`](../architecture.md). Measurements are
  in [`benchmark-minimal.md`](../reports/benchmark-minimal.md) and
  [`dgx-spark.md`](../reports/dgx-spark.md). The components, with where each one's API lives:
  - **Planner** (Rust, DataFusion frontend):
    [`peacockdb-core/src/planner/`](../../peacockdb-core/src/planner/). DataFusion's physical
    plan is translated in [`translator/`](../../peacockdb-core/src/planner/translator/), with
    memory estimation and pipeline shaping beside it. The node vocabulary it emits is in
    [`plan/mod.rs`](../../peacockdb-core/src/plan/mod.rs). Doc:
    [Planning](../architecture.md#planning), [The node set](../architecture.md#the-node-set),
    [Joins](../architecture.md#joins).
  - **Wire format** (planner to GPU): [`flatbuffers/gpu_plan.fbs`](../../flatbuffers/gpu_plan.fbs),
    written by [`peacockdb-core/src/wire/`](../../peacockdb-core/src/wire/). Doc:
    [The wire format](../architecture.md#the-wire-format).
  - **Executor traits:** the batch and executor traits in
    [`executor/mod.rs`](../../peacockdb-core/src/executor/mod.rs). Doc:
    [Traits](../architecture.md#traits).
  - **Driver** (scheduling, partitions, memory accounting):
    [`executor/driver/`](../../peacockdb-core/src/executor/driver/). That is `scheduler`,
    `single_partition`, `partitioned` and `accounting`. Doc:
    [The scheduling rule](../architecture.md#the-scheduling-rule),
    [Memory accounting](../architecture.md#memory-accounting).
  - **CPU backend:** [`executor/cpu_backend/`](../../peacockdb-core/src/executor/cpu_backend/).
    This is the reference and oracle path, and the one the CLI
    ([`peacockdb/src/main.rs`](../../peacockdb/src/main.rs)) runs.
  - **GPU backend** (Rust side):
    [`executor/gpu_backend/`](../../peacockdb-core/src/executor/gpu_backend/), calling the C
    ABI through [`peacockdb-ffi/src/lib.rs`](../../peacockdb-ffi/src/lib.rs).
  - **C++/cuDF executor:** the public surface is
    [`cpp/include/peacock_gpu.h`](../../cpp/include/peacock_gpu.h) (25 ABI symbols) and
    [`partitioning.hpp`](../../cpp/include/peacock/partitioning.hpp). The operators are in
    [`cpp/src/operators/`](../../cpp/src/operators/), and the join session is in
    `join_session.cpp`. Doc: [Interfaces](../architecture.md#interfaces),
    [C++ executor layout](../architecture.md#c-executor-layout),
    [Multi-GPU notes](../architecture.md#multi-gpu-notes-cudf-2602).
  - **Cost model:** [Cost model and DuckDB's cost oracle](../architecture.md#cost-model-and-duckdbs-cost-oracle).
  - **Optimizer prototype, not on master:**
    [PR #165](https://github.com/asymptote-tech/peacockdb/pull/165), branch `ENS-optimizer`,
    open, with merge conflicts. It is a join-order optimizer in the CPU prototype
    `scripts/exec_model/`. It estimates joins from statistics sidecars and Parquet footers,
    reorders inner-join clusters with DPhyp (the new `peacockdb-dphyp` Rust crate), prunes fact
    scans with dynamic filters, and replans when a build misses its estimate. Its answers match
    DuckDB on every corpus query. It has been judged on CPU only, not on the device. Read it
    with `git show origin/ENS-optimizer:<path>`. The entry points are
    `llm-wiki/tasks/join-optimizer.md`, `scripts/exec_model/optimizer.md` and
    `peacockdb-dphyp/design.md`. It is the main input to the comparison with Sirius's
    optimizer.

## How to judge each option (helper's call; the human may amend)

- **Who needs it badly:** a named user group, the pain, and the evidence for it.
- **Build cost** against the budget: reuse, adapt, new. Name the hard part.
- **The data path:** does NIC, SSD, Parquet decoding or ingestion eat the GPU's advantage?
  What cache would fix that?
- **Adoption friction:** a plugin into what users already run, versus a new system to adopt.
- **Platform risk:** NVIDIA, the host project, or an incumbent shipping the same thing.
- **The funding story:** what a design partner would say, and what the round would be raised on.

## Method

Phases are left to the plan, in this order of dependence:

1. An inventory of peacockdb today, by component, so build costs have a yardstick.
2. Discovery of further options, then a **checkpoint**: the human prunes the screened list
   before the deep research starts (helper's call). This caps how many deep research runs
   are made.
3. Research on each option, Sirius and NVIDIA, and user demand. These are independent, so run
   them as parallel subagents. Use `anthropic-skills:deep-research` wherever the question is
   open-ended.
4. Synthesis.

The research is desk-only (helper's call): no benchmarks, no GPU host and no re-run of the
spark-rapids instrumentation. Any claim that would need a measurement to settle, for example
whether Sirius runs Substrait from a system other than DuckDB, is listed in the report as a
follow-up experiment.

**Intermediate artifacts go in a new folder, `llm-wiki/artifacts/product-research/`.** That
covers the inventory, the discovery list, one findings file per research subagent, and the
data pulled from the two spreadsheets. Keeping them out of the session scratchpad means they
survive the session, and the report can link to them. Subagents write their findings there and
return only a short summary and the file path, never the bulk. `product-research-detail.md`
holds only run state: which phases are done, the checkpoint decision, and what a restarted
session needs to resume. (The per-task subfolder is the helper's call.)

## The report

One file: `llm-wiki/reports/product-direction.md`. It opens with the recommendation and the
runner-up, each in a few lines. It follows with:

- a comparison table of every evaluated option against the criteria;
- the lean tested, part by part;
- per-option sections;
- Sirius and NVIDIA;
- user demand;
- the open questions that only a conversation with a prospective user could settle.

Every factual claim carries a source link and the date it was accessed. Guesses and estimates
are marked as such.

## Scope

- **Code:** none. No file outside `llm-wiki/` changes.
- **Component-level API:** none.
- **Files written:** `llm-wiki/reports/product-direction.md`; the intermediate artifacts under
  `llm-wiki/artifacts/product-research/`, a new folder; and this task's `-impl.md` and
  `-detail.md` while it runs. The artifacts stay after the task is archived, because the report
  links to them.

## Done when

The report exists on master and covers every named option, the discovered ones, Sirius and
NVIDIA, and user demand, all to the bar above. It makes one recommendation with its runner-up,
and the human has read it.
