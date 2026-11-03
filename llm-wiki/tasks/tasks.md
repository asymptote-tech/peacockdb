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
to master in one small commit once that task is `done`.

### 1. [`containers-rebase.md`](containers-rebase.md) — closes [#226](../tickets/benchmarks.md#t226) — state: new

`ENS-containers-master`: containers' 16 commits replayed on master, plus the optimizer's benchmark
commit they build on (`02f6a4d8`) and nothing of the join optimizer; case list regenerated from
master's registry; green on the CPU gates; images published. Chain L absorbs the containers task;
the agent-container launcher, the embeddings' S3 path and the scratch-chain run are dropped.

### 2. [`new-hosts.md`](new-hosts.md) — closes no ticket — state: new

The `gpu-host` scripts renamed `nebius`; `nebius.sh --instance-id/--profile` for the H100
(eu-north1) and the 128-core CPU VM (eu-west1), 1-hour idle watchers with per-VM credentials, the
hourly GH watch over all three VMs, sf40 data on both, `run-gpu.sh` on the H100 over `nebius.sh`
(no host adapters), and `provision-vm.sh`/`bootstrap-vm.sh` verified on the CPU VM.

### 3. [`pinned-loads.md`](pinned-loads.md) — closes [#149](../tickets/performance.md#t149) — state: new

The parquet load reads into pinned host memory on a discrete GPU, measured on the H100.

### 4. [`bench-rows.md`](bench-rows.md) — closes no ticket — state: new

Benchmark results carry per-node `batch_rows`/`batch_bytes`, and `load_us`, `execute_us`,
`peak_bytes` in the trailer.

### 5. [`sf40-baseline.md`](sf40-baseline.md) — closes no ticket — state: new

TPC-H q1-q22 and the 82 TPC-DS corpus queries at sf40 tp1_single on the H100, and DuckDB on 128
cores for the same set; queries that fail at sf40 are skipped.

### 6. [`dag-driver.md`](dag-driver.md) — closes no ticket — state: new

Production: the driver runs plans with shared sources and several roots, without deadlock, with
per-consumer limits and an optional concurrency cap.

### 7. [`query-fuser.md`](query-fuser.md) — closes no ticket — state: new

Prototype: fuse the first 3/10/20/all queries into one DAG sharing scans, with mergeability and
pipelining hints and a memory cap; run them on the H100.

### 8. [`batching-report.md`](batching-report.md) — closes no ticket — state: new

`llm-wiki/reports/query-batching.md`: fused vs unfused vs DuckDB per batch size.

### 9. [`tp4-sized.md`](tp4-sized.md) — closes no ticket — state: new

The same query set at sf40 tp4_sized on the H100, unfused, and its sums added to the report.

### 10. [`containers-only.md`](containers-only.md) — closes [#129](../tickets/testinfra.md#t129) (part a) — state: new

Retire the non-container paths: CI in the build images (25.02, and a CI-only 25.10), the L40S's `gpu-tests` through `run-gpu.sh`,
local and verda builds through the image, the conda install and init scripts deleted, one path in
the wiki. shad-gpu stays as it is, since it has no Docker.
