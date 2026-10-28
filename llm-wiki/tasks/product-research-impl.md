# product-research Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One report, `llm-wiki/reports/product-direction.md`, that recommends the one product to
build on peacockdb (or Sirius), with a runner-up and the evidence for both.

**Architecture:** A helper session on master is the coordinator. It does the small local steps
itself: setup, the spreadsheets, checks, commits and the checkpoint. It dispatches research to
`general-purpose` subagents. Each subagent writes one artifact file under
`llm-wiki/artifacts/product-research/` and returns at most ten lines. The synthesis reads the
artifacts, not the subagents' transcripts.

**Tech Stack:** Agent tool (`general-purpose`), `anthropic-skills:deep-research`, WebSearch and
WebFetch, `gh`, `git`, and the Google Sheets API in a scratch venv.

**Spec:** [`product-research.md`](product-research.md). Read it first. This plan argues from it.

## Global Constraints

- Desk-only. No benchmarks, no GPU host, and no re-run of the spark-rapids instrumentation. A
  claim that needs a measurement goes into the artifact's `## Follow-up experiments`.
- Nothing outside `llm-wiki/` changes. External repos (Sirius, spark-rapids, …) are cloned
  under the session scratchpad, never into the tree.
- Open only spreadsheets `10kT2kzhNBzP0rHRMoF9JZNWNY0uTkQ8Hm75DWcl_sgM` and
  `1pOgRm0mE8KhRj17GJltqM5lPxfq3ZFPDZ1dJ5II9RKA`. Use read-only scope. Never print, copy or
  commit the key.
- Every factual claim carries a link and an access date, `(accessed YYYY-MM-DD)`, in the host's
  local time zone. A claim about code also carries the commit SHA it was read at. Guesses are
  marked `(estimate)` or `(guess)`.
- Size build cost by component against the inventory's yardstick, never from git history.
- Size every option to an **alpha** (a first user runs it on their own data), on peacockdb and on
  Sirius wherever both are plausible.
- GPU acceleration stays central. Discovery does not range into non-GPU niches.
- A subagent returns at most ten lines: its verdict and its file path. The bulk stays in the file.
- The helper's messages to the human are at most fifteen lines.
- The human reads the result, so the writing is plain: short sentences, ordinary words.

## Review Focus

1. **Research that confirms the lean, or is biased against it.** The lean is an option like
   the others: the same template, the same ratings, a row in the table. No researcher is told it
   is the human's lean. Every researcher, the lean's included, must look for evidence against
   its own option. Option prompts paste the spec sections they need, never the whole spec, so
   the hypothesis section stays unseen. Pinned in Task 4 (prompt rule) and Task 6 (audit
   question).
2. **Build costs that cannot be compared.** Every option uses the same cost table and the same
   yardstick. Pinned in Task 4 (template) and Task 5 (heading check).
3. **Stale or unsourced claims**, especially about Sirius, which changes weekly. Pinned in Task 5
   (source-line check) and Task 6 (sampled link audit).
4. **Thin demand evidence passed off as strong.** The demand file must give counts and say
   "thin" where it is thin. Pinned in Task 4 (template) and Task 6 (audit question).
5. **The helper's window filling with research bulk**, which loses the run's state. Pinned by
   the ten-line return rule, and by `-detail.md` written after every phase (each task's last
   step).

---

## Artifact layout (the interface every task shares)

    llm-wiki/artifacts/product-research/
      inventory.md            Task 1: peacockdb by component; the yardstick
      evidence-in-hand.md     Task 1: spark-rapids study and sheet summary
      sheets/*.csv            Task 1: the two spreadsheets, one CSV per tab
      spark-rapids-instrumentation.patch   Task 1: `git show 99a1d01b7`
      discovery.md            Task 2: the long list, with screen verdicts
      options/<slug>.md       Task 4: one per surviving option, option template
      options/lean.md         Task 4: the hypothesis as an option, template plus `## The three parts`
      sirius.md               Task 4: Sirius due diligence, comparison, and Sirius sized by component
      nvidia.md               Task 4: NVIDIA's stance and its other open-source projects
      demand.md               Task 4: user demand and first-conversation candidates

The named options' slugs are `clickhouse`, `lakehouse-iceberg`, `realtime-vram-cache`,
`streaming-flink`, `datafusion-plugin`, `realtime-other`, `distributed-pluggable`,
`spark-plugin`, `other-accelerators` and `vector-sql`. The lean is `lean`. Discovered options get
a slug in `discovery.md`. Every option file is at most 300 lines.

**The option template.** Every `options/<slug>.md` has these headings, in this order:

    # <option name>
    ## Verdict            three lines: what it is, who needs it, whether it fits the budget
    ## Who needs it badly named groups, their pain, threads with links and dates
    ## How it would work  architecture sketch, and the data path end to end
    ## The human's questions   each question from the spec, answered (or "n/a" for discovered options)
    ## Data path and cache     bandwidth numbers: NIC, SSD, decode, PCIe, HBM; what cache is needed
    ## Build cost to alpha     the cost table below, then the hard part
    ## Adoption friction
    ## Platform risk
    ## Funding story
    ## Evidence against   the strongest case against this option
    ## Ratings            one line per criterion: strong | medium | weak, with a reason
    ## Follow-up experiments
    ## Sources

The cost table:

    | Component | On peacockdb: reuse/adapt/new, size | On Sirius: reuse/adapt/new, size |
    Total new work: <x>× engine on peacockdb, <y>× on Sirius (estimate)

The six criteria are the spec's: who needs it badly, build cost, data path, adoption friction,
platform risk, funding story. The "who needs it badly" rating is taken from `demand.md` and cited,
never rated independently. Sirius-side sizes use `sirius.md`'s component table.

---

### Task 0: Set up the run

**Files:**
- Create: `llm-wiki/artifacts/product-research/` (with `options/` and `sheets/`)
- Create: `llm-wiki/tasks/product-research-detail.md`

- [ ] **Step 1:** Create the folders. Write `-detail.md` with one line per task (`Task N: todo`),
  the session scratchpad path, and a `## Checkpoint` heading left empty.
- [ ] **Step 2:** Make a scratch venv in the scratchpad with `google-api-python-client` and
  `google-auth`. Clone Sirius into the scratchpad
  (`git clone --depth 200 https://github.com/sirius-db/sirius`; take the URL from Sirius's own
  page if it has moved). Record both paths and Sirius's HEAD SHA in `-detail.md`.
- [ ] **Step 3: Commit** `-detail.md` with the message `product-research: start the run`.

### Task 1: The yardstick and the evidence in hand (helper and one subagent, in parallel)

**Files:** `inventory.md`, `evidence-in-hand.md`, `sheets/*.csv`, `spark-rapids-instrumentation.patch`

**Interfaces:**
- Produces: the yardstick, defined in `inventory.md` under `## Yardstick`. It is the engine's
  non-test, non-generated source lines (Rust plus C++), per component and in total, plus PR #165
  (optimizer prototype) counted separately. Every later cost is a fraction of the total.

- [ ] **Step 1 (subagent, inventory):** Dispatch with the prompt template below.
  - Job: for each component in the spec's "The engine itself" list, write: its paths, its size
    in yardstick lines (tests counted separately), what it does today, its maturity, and what a
    product could reuse as is. Maturity means GPU coverage, taken from the gpu cells of
    `testdata/cost-registry.csv` and from `testdata/report.html`.
  - Cover multi-GPU, spilling, caching and memory accounting explicitly. They are the lean's
    core.
  - Read PR #165 with `git show origin/ENS-optimizer:<path>` after `git fetch origin
    ENS-optimizer`.
  - End with `## Yardstick`, a table and a total.
- [ ] **Step 2 (helper, in parallel):** Export each tab of the two spreadsheets to
  `sheets/<sheet>-<tab>.csv`. Save `git -C ~/repo/spark-rapids show 99a1d01b7` as the patch.
  Write `evidence-in-hand.md`:
  - what the instrumentation measured, read from the patch;
  - the SF200 table's surviving numbers, against DuckDB, the PoC and Voltron Data;
  - the hardware-cost rows;
  - what is lost.
- [ ] **Step 3: Verify:** `grep -c '^## Yardstick' inventory.md` prints 1. The patch is
  non-empty. There is one CSV per tab, matching the tab count the API reports for each sheet.
- [ ] **Step 4:** Mark Task 1 done in `-detail.md`. Commit with the message
  `product-research: inventory and evidence in hand`.

### Task 2: Discovery and the screen (one deep-research subagent)

**Files:** `discovery.md`

- [ ] **Step 1:** Dispatch one subagent that uses `anthropic-skills:deep-research`.
  - Job: find every GPU-acceleration product direction an engine like this could take. The ten
    named options are the floor, not the list. Name where GPU SQL acceleration is sold,
    requested or abandoned today, including directions the spec does not name.
  - Inputs: the spec's Options section (pasted, without the lean), and `inventory.md`.
  - Output: `discovery.md`, with one table of every option, named and discovered. The columns
    are slug, one-line description, a `pass`/`fail` screen verdict, and the reason, citing a
    source.
  - The screen fails a **discovered** option only for one of these: no sign of anyone needing
    it; plainly over budget for an alpha (more than 2× the yardstick); or not GPU-centred.
  - Named options always pass. A named option that looks weak is flagged `weak` with the
    reason. Only the human may cut it, at the checkpoint.
- [ ] **Step 2: Verify:** the table has all ten named slugs, plus at least one discovered row, or
  a stated reason why none was found.
- [ ] **Step 3:** Mark Task 2 done. Commit with the message `product-research: discovery`.

### Task 3: Checkpoint (the human)

- [ ] **Step 1:** Send the human the screened list, in at most fifteen lines: the passes, the
  fails with one-word reasons, and the planned ranking rule.

  The ranking rule (plan's call, the human may amend): an option whose alpha costs more than 2×
  the yardstick on both bases is out. The rest are ordered by the strength of the demand
  evidence. Ties are broken by adoption friction, then the data path, the funding story and
  platform risk, in that order. Tell the human that adoption friction is where the lean's
  plugin-versus-standalone claim is decided, so its rank in this order matters.
  The `lean` option is kept without being screened. Discovery never saw it, and only the human
  may cut it.
- [ ] **Step 2:** Stop until the human answers. Record the kept list, the cut list and any change
  to the rule under `## Checkpoint` in `-detail.md`. A named option cut here is recorded with
  the human's reason, and the report lists it. Commit with the message
  `product-research: checkpoint`.

### Task 4: The research wave (parallel subagents)

**Files:** `options/<slug>.md` for every kept option, plus `sirius.md`, `nvidia.md`
and `demand.md`.

**Interfaces:**
- Consumes: `inventory.md` (the yardstick) and `evidence-in-hand.md`. Option subagents also
  consume `sirius.md`, `nvidia.md` and `demand.md`, so they start only once those three exist.
- Produces: one file per subagent, in the template its job names.

- [ ] **Step 1:** Dispatch in waves of at most five concurrent subagents.
  - Wave 1 is Sirius, NVIDIA and demand.
  - Option waves, the lean included, start after wave 1 has written its three files, and are
    given their paths.
  - Every job below uses `anthropic-skills:deep-research` for its open questions.
  - **Each option:**
    - Fill the option template.
    - Answer the human's questions for that option, verbatim from the spec.
    - Test the spec's NIC/SSD/RAM-cache thesis for this option.
    - Cost the alpha on peacockdb and on Sirius.
  - **`sirius.md`:**
    - The five due-diligence questions, answered from Sirius's code at the recorded SHA, cited
      by file and line.
    - Activity, through `gh api` (the shallow clone undercounts): commits, contributors and their
      employers, releases, licence.
    - `## Sirius components`: Sirius sized by component, in the yardstick's units, so options
      can cost the Sirius base the same way.
    - Whether a fork is really out.
    - A table comparing peacockdb and Sirius, including PR #165's optimizer, cache and spilling.
      Each row says which is ahead and what it would cost to close the gap.
  - **`nvidia.md`:**
    - NVIDIA's stated stance on competing, from public statements, hiring and roadmap.
    - Every open-source NVIDIA project in this space: cuDF, cudf-polars, spark-rapids, the
      Velox cuDF backend, cuVS, Sirius's backing, and whatever else is found. For each, what it
      means for each option.
  - **`demand.md`:**
    - The GitHub issues and discussions, forums and community channels of every system behind a
      named option, whether or not it was cut, and of every discovered option that passed.
    - One `strong | medium | weak` demand rating per option, with its evidence. Option files cite
      this rating.
    - Counts of requests, users who already accelerate and how, and the use cases.
    - Threads, with links and dates.
    - A `## First conversations` section naming the user groups, companies or thread authors
      that look like the best design-partner leads.
    - "thin" written wherever the evidence is thin.
  - **`options/lean.md`:** the full option template, plus `## The three parts`, each tested
    for and against:
    - Can a multi-GPU, single-host RAM/VRAM cache answer ~100 GB queries in a few hundred ms?
      Use bandwidth arithmetic plus `benchmark-minimal.md` and `dgx-spark.md`.
    - Plugin versus standalone: evidence from adoption of each.
    - Why Voltron Data really shut down, from sources.

    The subagent is given these parts as "a candidate product's premises", not as the human's
    view.

  **The prompt template** (fill the brackets):

  > You are researching one question for a product decision about peacockdb, a GPU SQL engine
  > (Rust/DataFusion planner, C++/cuDF executor). The decision's context: [the spec's Why,
  > Budget and How to judge each option sections, pasted]. Read
  > `llm-wiki/artifacts/product-research/inventory.md`, `evidence-in-hand.md` and [wave-1 paths,
  > for option jobs]. Do not open `llm-wiki/tasks/`. Your job: [job]. Write your findings to
  > [path], using [template], in at most 300 lines. Make the strongest case against your
  > option as well as for it.
  > Desk research only: never run benchmarks or touch a GPU host. Clone repos only under
  > [scratchpad]. Every claim carries a link and `(accessed YYYY-MM-DD)`, plus a commit SHA for
  > code. Mark guesses `(estimate)`. Put anything that would need a measurement under
  > `## Follow-up experiments`. Write nothing else in the repo. Reply in at most ten lines: your
  > verdict and the file path.

  No subagent gets word of the human's lean, or of which option the human favours.
- [ ] **Step 2:** As each subagent returns, append its ten lines to `-detail.md`. Commit the
  wave's files with the message `product-research: research wave <n>`.

### Task 5: Check the artifacts (helper)

- [ ] **Step 1:** Run the heading check over every option file:

  ```bash
  cd llm-wiki/artifacts/product-research
  for f in options/*.md; do for h in Verdict 'Who needs it badly' 'How it would work' \
    "The human's questions" 'Data path and cache' 'Build cost to alpha' 'Adoption friction' \
    'Platform risk' 'Funding story' 'Evidence against' Ratings 'Follow-up experiments' Sources; do
    grep -qF "## $h" "$f" || echo "$f: missing $h"; done
    grep -q 'Total new work:' "$f" || echo "$f: no total"; done
  ```

  Expected: no output.
- [ ] **Step 2:** Run the source check:
  `grep -L 'accessed 20[0-9][0-9]-' options/*.md *.md`. Expected: only `inventory.md` and
  `evidence-in-hand.md`, which cite the repo.
- [ ] **Step 3:** Check by hand that every kept slug and `lean` has a file, that `demand.md` has
  `## First conversations` and a rating for every option, and that `sirius.md` has
  `## Sirius components`. For each failure, re-dispatch the owning subagent with the gap
  named. Commit the fixes.

### Task 6: Synthesis and audit

**Files:** `llm-wiki/reports/product-direction.md`

- [ ] **Step 1: Write the report (helper).** Follow the spec's report outline, in order:
  - recommendation and runner-up, each naming its base, peacockdb or Sirius;
  - the base decision: both cost totals, the human's knowledge of peacockdb's code, the freedom
    to evolve code they own, and NVIDIA's platform risk, weighed in plain words;
  - the comparison table, with one row per option and the six ratings plus both cost totals;
  - the lean, part by part;
  - per-option sections;
  - the options cut at the screen or the checkpoint, one line each with the reason;
  - Sirius and NVIDIA;
  - demand;
  - the follow-up experiments, collected from the artifacts;
  - the questions for a prospective user.

  Apply the checkpoint's ranking rule and show how it decided. Read each option file's Verdict,
  Ratings and cost total first, and the rest only where the report needs it. Each per-option
  section is a summary that links to its artifact. Every factual sentence keeps its source and date from the
  artifact.
- [ ] **Step 2: Audit (one fresh subagent, given the report and the artifact folder, not this
  conversation).** Its job:
  - open 15 sources picked at random and say whether each supports the sentence citing it;
  - say whether the lean was judged like the other options, neither restated nor singled out;
  - say whether any "strong" demand rating rests on fewer than three independent threads;
  - say whether the ranking rule, applied to the table, really gives the stated recommendation.

  It writes `audit.md` in the artifact folder and replies in ten lines.
- [ ] **Step 3:** Fix every finding in the report and the artifacts. Mark Task 6 done in
  `-detail.md`. Commit with the message `product-research: report`.
- [ ] **Step 4:** Tell the human the report is ready: the recommendation, the runner-up, and the
  link, in at most fifteen lines. The task is done when the human has read it. The signoff is
  appended to the spec then.
