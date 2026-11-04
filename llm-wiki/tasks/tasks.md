# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain J (base: master)

> **Host override, from 2026-10-08 until the human lifts it.** It overrides the specs and
> `build-test.md` wherever they disagree.
>
> **GPU host:** nebius-gpu through `scripts/gpu-host.sh`, `gpu-host-sync.sh` and `gpu-host-test.sh`
> (`build-test.md`, "GPU host"). The L40S list only; sf40 and benchmarks are out and deferred.
> The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
>
> - **This chain's tree on the VM is `~/peacockdb-J`**: sync with `gpu-host-sync.sh --dest
>   peacockdb-J`. Its sf1 data is already there.
> - **Large tests and benchmark measurements are out of every task.** That means the sf40
>   binaries (`peacock_tpch_tests`, `peacock_tpchv_tests`), `--run-benchmarks`, Nsight captures,
>   and any H200 timing a spec asks for. Record each skipped item in the task's detail file as
>   deferred; it does not block the task.
> - **On resuming the chain, reset the board first, in one commit:**
>   - duckdb-oracle: `completeness approved` → `building`;
>   - pbench: `completeness approved` → `building`;
>   - repartition-keys: `blocked(reviewing)` → `building`;
>   - stale-cells: `blocked(approved to build)` → `approved to build`.
>
>   Then take the first task, duckdb-oracle, and finish it on nebius-gpu's card under the rules
>   above. The GPU halves those tasks deferred for want of a device are now runnable.
>
> - **stale-cells moves after refcounted-scatter** (resequenced 2026-10-08), and branches from it
>   rather than forking the chain off duckdb-oracle. The order is now duckdb-oracle, pbench,
>   repartition-keys, refcounted-scatter, stale-cells, exit-copies, join-session-cpp, join-backend,
>   verify-26.02.
> - **Cost regressions are accepted for repartition-keys, join-session-cpp and join-backend.** For
>   these three, a red cost-report job does not block `done`:
>   - for repartition-keys, the +423-byte lane-split regression its developer diagnosed;
>   - for join-session-cpp and join-backend, any regression they bring, approved in advance.
>
>   Name each regression in the task's detail file and its PR, with the queries and the byte
>   delta, so the acceptance has a record. For every other task the gate still decides. Master's
>   gate now fails only past +10%, so a rebase onto master turns small regressions green anyway.

The join rewrite, and what it stands on. Joins leave the recipe architecture for a C++ session
(built once, probed per batch, finished once) behind four new C symbols; every non-join node keeps
its recipes. Shared design: [`join-rewrite-design.md`](join-rewrite-design.md). Replaces chain A's
refcounted-tables, whose scatter half is refcounted-scatter and whose join half the session makes
unnecessary.

### 1. [`verify-26.02.md`](verify-26.02.md) — closes no ticket — state: new

Every tier on shad-gpu's cuDF 26.02 environment, fixes, and the tpch sf40 benchmark of the cells
the chain turned on, against 25.02; the evidence #244 waits on. On hold: not to be built for now.

## Chain L (base: master)

Query batching. Fuse several queries' plans so each sf40 table loads once, and measure what that
buys on a Nebius H100 against DuckDB on 128 cores. The first task forks containers (PR 192) onto master
without `ENS-optimizer`; every later task stacks on it. The chain
takes over containers' device run (as `scripts/env/run-gpu.sh`) and its 80 GB benchmark, tp1_single
and tp4_sized only. new-hosts' rename of the `gpu-host` scripts to `nebius` and its VM guarding go
to master in one small commit, `ENS-nebius-guard`, mid-task, once their offline tests pass; the
human lands it, and new-hosts' live three-VM watch check is dispatched from master after that.

- **containers-rebase's history surgery stays in the roles.** The coordinator runs every git
  command (cherry-pick, rebase, each stop's `add` and `continue`, push); at each stop the
  developer resolves the files and compiles. The coordinator's `building` commit is kept: the
  plan tags it and re-applies it over the replay, never resets the branch under it.
- **Hosts.** new-hosts writes the three VMs' roles into the standing rules (`prompts.md`,
  `build-test.md` "Nebius host"): the L40S for correctness, the H100 for sf40, benchmarks and
  Nsight, the 128-core VM for DuckDB and the H100's staged install; the coordinator pre-starts
  no VM and hard-stops all three. No host override here: the tasks after new-hosts follow those
  rules.
- **Checkpoints.** Each plan (`<task>-impl.md`) has them; read its Global Constraints first.
  - A developer's report naming a plan's *Commit* step ("report a checkpoint `<msg>`") is a
    checkpoint, not the task's green: commit it, push, dispatch the developer at the next step.
    Only a plan's last step is green (PR, `reviewing`). A *(coordinator)* step is yours; the
    developer stops before it and says what it needs.
  - containers-rebase: every git write (tag, cherry-pick, rebase and each stop's `add` and
    `continue`, the branch moves, the push) is yours; a stop's compile green is a checkpoint.
    Before Task 1 Step 2, write the surgery state into the git-ignored
    `.claude/ensemble/L.surgery` (never the tracked detail file while HEAD is off the branch);
    after Step 5's cherry-picks, fold it into `containers-rebase-detail.md` as `## Surgery` and
    keep both through Step 10. In Task 1 a checkpoint is committed, not pushed: its only push is
    Step 10.
  - new-hosts: after Task 1 (rename) and Task 5a (host rules) are committed and pushed, re-read
    `prompts.md`, `.claude/commands/ensemble.md` and `build-test.md` in place, and write "standing
    rules re-read; next: <step>" into the detail file before dispatching. Never exit for it (the
    board stays `building`, so a restart spends the watchdog's idle allowance); until Task 1,
    the old names. A developer's exit 75: retry that VM's `scripts/nebius.sh <vm options> start`
    yourself every 15 minutes for up to 10 hours, then dispatch the developer at once from its
    first remote command; never re-dispatch the developer to retry. Task 7's mid-task push, Task 12's
    `ENS-nebius-guard` branch and push, and every workflow dispatch are yours. Task 6 Step 5
    waits on the human landing ENS-nebius-guard (read on master by content); it holds `done`,
    never `reviewing`.
  - containers-only, containers-rebase Task 6: every push, dispatch and CI read is yours.
  - **Restarts.** A restarted coordinator resumes from its own branch's board (`git show
    ENS-<task>:llm-wiki/tasks/tasks.md` when the working tree shows another) and the task's
    detail file, never from master's board. If `ENS-<task>` exists on origin, it never branches
    anew. A rebase or cherry-pick in progress, a detached HEAD, or HEAD on `containers-replay`
    means containers-rebase is mid-surgery: resume from `.claude/ensemble/L.surgery` (or the
    detail file's `## Surgery` once Step 5 has folded it in).

### 1. [`containers-rebase.md`](containers-rebase.md) — closes [#226](../tickets/benchmarks.md#t226) — state: approved to build

`ENS-containers-rebase`: containers' 16 commits replayed on master, plus the optimizer's benchmark
commit they build on (`02f6a4d8`) and nothing of the join optimizer; case list regenerated from
master's registry; green on the CPU gates; images published. Chain L absorbs the containers task;
the agent-container launcher, the embeddings' S3 path and the scratch-chain run are dropped.

### 2. [`new-hosts.md`](new-hosts.md) — closes no ticket — state: approved to build

The `gpu-host` scripts renamed `nebius`; `nebius.sh --instance-id/--profile` for the H100
(eu-north1) and the 128-core CPU VM (eu-west1), 1-hour idle watchers with per-VM credentials, the
hourly GH watch over all three VMs, sf40 data on both, `run-gpu.sh` on the H100 over `nebius.sh`
(no host adapters), `provision-vm.sh`/`bootstrap-vm.sh` verified on the CPU VM, and the three
hosts' roles in the standing rules.

### 3. [`pinned-loads.md`](pinned-loads.md) — closes [#149](../tickets/performance.md#t149), [#232](../tickets/performance.md#t232) — state: approved to build

The parquet load reads table files kept resident in a pinned host file cache (on by default on
a discrete GPU, off with no CUDA device; session option `pinned_host_cache_bytes`, default three
quarters of min(host RAM, the cgroup memory limit), `0` = off) through a one-chunk
`chunked_parquet_reader`, with cuDF's pinned pool configured if the profile asks; each fix
measured alone and together on the H100. Each benchmark process prefills every case's files once,
before its first case, and every round reads them pinned; every later H100 run records the cache
and the prefill in the trailer (`pinned_cache=`, `prefill_files=`, `prefill_bytes=`, `prefill_us=`).

### 4. [`bench-rows.md`](bench-rows.md) — closes no ticket — state: approved to build

Benchmark results carry per-node `batch_rows`/`batch_bytes`, and `load_us` (device-side), `execute_us`,
`peak_bytes`, the pinned host cache's state (`pinned_cache`) and the prefill (`prefill_files`,
`prefill_bytes`, `prefill_us`) in the trailer.

### 5. [`sf40-baseline.md`](sf40-baseline.md) — closes no ticket — state: approved to build

TPC-H q1-q22 and the 82 TPC-DS corpus queries at sf40 tp1_single on the H100, and DuckDB on 128
cores for the same set; queries that fail at sf40 are skipped.

### 6. [`dag-driver.md`](dag-driver.md) — closes no ticket — state: approved to build

Production: the driver runs plans with shared sources and several roots, without deadlock, with
per-consumer limits and an optional concurrency cap.

### 7. [`query-fuser.md`](query-fuser.md) — closes no ticket — state: approved to build

Production (its code under `src/prototypes/`): fuse the first 3/10/20/all queries into one DAG
sharing scans, with mergeability and pipelining hints and a memory cap; run them on the H100.

### 8. [`batching-report.md`](batching-report.md) — closes no ticket — state: approved to build

`llm-wiki/reports/query-batching.md`: fused vs unfused vs DuckDB per batch size.

### 9. [`tp4-sized.md`](tp4-sized.md) — closes no ticket — state: approved to build

The same query set at sf40 tp4_sized on the H100, unfused, and its sums added to the report.

### 10. [`containers-only.md`](containers-only.md) — closes [#129](../tickets/testinfra.md#t129) (part a) — state: approved to build

Retire the non-container paths: CI in the build images (25.02, and a CI-only 25.10), the L40S's `gpu-tests` through `run-gpu.sh`,
local and verda builds through the image, the conda install and init scripts deleted, one path in
the wiki. shad-gpu stays as it is, since it has no Docker.

## Chain M (base: master)

Should peacockdb become a GPU backend for Apache Comet, and how. One research task, run in
workspace alpha on dev alongside chain L. It drives the CPU VM through the `nebius` CLI
directly (new-hosts has not landed), never touches the H100, force-stops every VM it used, and
keeps to its VM-hour caps (CPU VM 2 h, L40S 1 h). Every git write and the PR are the coordinator's.

- **Read the plan's Global Constraints first** (`comet-research-impl.md`). They override the
  standing GPU-host rules for this chain.
- **Checkpoints.** A developer report naming "report a checkpoint `<msg>`" is a checkpoint:
  commit it with that message, push, and dispatch the developer at the next step. Only Task 6
  Step 5 is green; then do Task 6 Step 6 (signoff, `done`, commit, push, PR against master). No
  `reviewing`, no reviewer, no completeness pass, no CI wait.
- **VMs.** Pre-start none. Once `llm-wiki/artifacts/comet-research/scripts/vm.sh` exists, run
  `vm.sh stop-used` at startup when no dispatch is live, after any dispatch that ends without a
  report, and before every exit. It replaces `gpu-host.sh stop --force` for this chain, and
  covers the L40S. On exit 75, retry `vm.sh <vm> use` every 15 minutes for at most 2 hours, then
  tell the developer the experiment is skipped. Never stall on it.
- **Never exit voluntarily at a checkpoint**, only with no VM in use and when the window forces it.

### 1. [`comet-research.md`](comet-research.md) — closes no ticket — state: building

Prototype with a PR against master, no reviewer. `llm-wiki/reports/comet-gpu.md`: go, conditional
go or no-go against 2× lower cost per query than Comet on CPU, from desk research, S3 throughput on
two VMs (L40S, CPU VM in-region), Comet on 1 × 128 cores and a cost model; on a go, the alpha design.
