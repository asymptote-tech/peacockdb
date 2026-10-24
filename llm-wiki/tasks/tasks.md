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
