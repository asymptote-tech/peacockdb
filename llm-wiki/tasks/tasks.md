# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain J (base: master)

> **Host override, from 2026-10-08 until the human lifts it.** It overrides the specs and
> `build-test.md` wherever they disagree.
>
> **GPU host:** nebius-gpu through `scripts/nebius.sh`, `nebius-sync.sh` and `nebius-test.sh`
> (`build-test.md`, "Nebius host"). The L40S list only; sf40 and benchmarks are out and deferred.
> The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
>
> - **This chain's tree on the VM is `~/peacockdb-J`**: sync with `nebius-sync.sh --dest
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

## Chain M (base: master)

Should peacockdb become a GPU backend for Apache Comet, and how. One research task, run in
workspace alpha alongside chain L. It drives the H100 and the CPU VM through the `nebius` CLI
directly (new-hosts has not landed), stops only VMs it started, and keeps to its VM-hour caps
(CPU VM 6 h, H100 1 h, L40S 1 h). Every git write and the PR are the coordinator's.

### 1. [`comet-research.md`](comet-research.md) — closes no ticket — state: approved to build

Prototype with a PR against master, no reviewer. `llm-wiki/reports/comet-gpu.md`: go, conditional
go or no-go against 2× lower cost per query than Comet on CPU, from desk research, S3 throughput on
three VMs, Comet on 128 cores (1 × 128 and 8 × 16) and a cost model; on a go, the alpha design.

## Chain N (base: master)

vm-setup (PR #205, rk00's `ENS-vm-setup`) landed on master after chain L's squash, with Sirius out of
`run-gpu.sh` and in a script of its own on vm-setup's runner. Offline only: no VM is started and no
workflow dispatched. `ENS-vm-setup` is never pushed to. Task 1 of the plan commits the replay with
Roman as its author; every later commit is the coordinator's own.

### 1. [`vm-setup-rebase.md`](vm-setup-rebase.md) — closes no ticket — state: approved to build

vm-setup replayed onto master as Roman's commit; `scripts/env/run-sirius.sh` (`prepare`, `run`) on
a client library shared with `run-gpu.sh`; Sirius runs under the card lock, stamped for `pull`, its
image run by ID; `bench-sirius.sh` and the docs follow. A new PR supersedes #205.
