# Task board

One `##` section per chain, lettered, naming its base. States and transitions: the "Board protocol"
section of `llm-wiki/prompts.md`. A coordinator writes this file only on its own chain
branch, which is why two chains need no locking.

## Chain J (base: master)

> **Host override, from 2026-10-08 until the human lifts it.** It overrides the specs and
> `build-test.md` wherever they disagree.
>
> - **shad-gpu is down.** Build and run the GPU tests on **nebius-gpu**,
>   `dmitry@89.169.109.150`: an NVIDIA L40S with 46 GB, Ubuntu 24.04 and glibc 2.39, so it needs
>   no glibc patch. Use the address; not every host has the alias.
> - **Only what needs the GPU goes there.** The device build (`build-test-shadgpu.sh --build`)
>   and the device runs happen on nebius-gpu. Every CPU build and run stays local, as
>   `build-test.md` describes: rust-only, the CPU tiers, the C++ CPU tests, cost-report.
> - **cuDF 25.02, as on shad-gpu.** Its env is `~/data/miniforge3/envs/rapids-cuda-12.2`, the
>   path `shadgpu-env.sh` names. cuDF 26.02 there is red for reasons outside this chain:
>   [#260](../tickets/system-hardening.md#t260) and [#94](../tickets/corpus-coverage.md#t94).
> - **Sync the working tree, uncommitted, before every build. Do not commit to build.** Every
>   commit stays green as before, and no work-in-progress push spends a CI run. From the workspace
>   root:
>   `rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./ dmitry@89.169.109.150:peacockdb-J/`.
>   `--delete-after`, not `--delete`: otherwise the first sync deletes before the `.gitignore` files
>   arrive, and so deletes the ignored data and build dirs it should keep.
>   The filter keeps build dirs, target dirs and generated data on both sides.
> - **On nebius-gpu**, in `~/peacockdb-J`, after `. ~/peacock-env.sh`:
>   - the sf1 data is already in `testdata/` there (generated 2026-10-08). Regenerate only if it
>     is missing: `testdata/generate_testdata.sh --bench tpch` and `--bench tpcds`;
>   - build: `./scripts/build-test-shadgpu.sh --build`;
>   - run each staged binary directly, never `--run`, which ssh-es to shad-gpu. Set
>     `LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib` and
>     `PEACOCK_TESTDATA_DIR=$PWD/testdata`, and pass `--test-threads=1` to each Rust binary:
>     - `cpp/install/bin/peacock_gpu_tests` and `cpp/install/bin/peacock_plan_tests`. Not
>       `peacock_cpu_tests`, which runs locally, and not the sf40 pair, `peacock_tpch_tests` and
>       `peacock_tpchv_tests`;
>     - `cpp/install/rust-tests/test_gpu_corpus`;
>     - `cpp/install/rust-tests/test_node_timing`;
>     - `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::`;
>     - `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_`.
> - **If nebius-gpu runs short of disk** (96 GB in all), the developer finds what is using it
>   (`du -xh -d2 ~ | sort -rh | head`) and cleans up what can be rebuilt. That means stale build
>   and target dirs, the conda package cache (`~/miniforge3/bin/conda clean -a`), and the #260
>   debug build in `~/peacockdb` (`cpp/build26`, `target-cudf-rapids`). Never delete the two cuDF
>   envs or `~/peacockdb-J/testdata`. Record what was removed in the detail file.
> - **Large tests and benchmark measurements are out of every task.** That means the sf40
>   binaries (`peacock_tpch_tests`, `peacock_tpchv_tests`), `--run-benchmarks`, Nsight captures,
>   and any H200 timing a spec asks for. Record each skipped item in the task's detail file as
>   deferred; it does not block the task.
> - **Done** means CI is green except the `gpu-tests` job, "GPU Tests (remote)", which runs on
>   shad-gpu. The GPU tests above must also have passed on nebius-gpu, with the run recorded in
>   the detail file.
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

### 1. [`duckdb-oracle.md`](duckdb-oracle.md) — closes [#235](../tickets/corpus-coverage.md#t235) — state: approved to build

Every corpus line names its `duckdb_oracle`; a comparison case per line against
`duckdb-result.txt`, over `mini.result.txt` and the device's new `gpu-result.txt`; a fingerprint
for sections over the 256 KB cap. First, so every cell a later task enables meets DuckDB.

### 2. [`pbench.md`](pbench.md) — closes [#227](../tickets/corpus-coverage.md#t227) — state: approved to build

A third dataset, sf1 only, committed: NULL keys on both sides, `NOT IN` over NULLs, every
hashable key type, skew, empty sides. One query per demonstrable ticket of the chain, cells off;
each later task turns its own on. Folds in #227: both engines check declared non-nullability.

### 3. [`repartition-keys.md`](repartition-keys.md) — closes [#201](../tickets/corpus-coverage.md#t201), [#206](../tickets/corpus-coverage.md#t206), [#240](../tickets/corpus-coverage.md#t240), [#95](../tickets/corpus-coverage.md#t95), [#189](../tickets/corpus-coverage.md#t189) — state: approved to build

The murmur gate onto the production lane rule first; then float, boolean, timestamp and decimal
keys on the device (decimals hashed as 16 bytes on both engines), and the rollup's grouping id
out of the shuffle. #243's pins.

### 4. [`refcounted-scatter.md`](refcounted-scatter.md) — closes [#145](../tickets/corpus-coverage.md#t145), [#197](../tickets/corpus-coverage.md#t197) — state: approved to build

`TableResult` takes its final shape, one owner per column; a scatter's partitions share them. Peak
3× → 2× during the partition, 1× after. Files the accounting ticket on landing.

### 5. [`stale-cells.md`](stale-cells.md) — closes no ticket — state: approved to build

The 16 device cells four tpch rows keep off under closed tickets (#183, #187), never run since:
run, enabled or ticketed, tags struck.

### 6. [`exit-copies.md`](exit-copies.md) — closes [#154](../tickets/corpus-coverage.md#t154) (outside `join.cpp`) — state: approved to build

The 11 operator exit copies outside `join.cpp`, `expr.cpp`'s `ColumnRef` copy the costliest.

### 7. [`join-session-cpp.md`](join-session-cpp.md) — closes no ticket on its own (the C++ half of #136, #153, #160, #215, #63; #154's `join.cpp` sites) — state: approved to build

`CudfJoin`, the four symbols, `join.cpp` rewritten per the design's §3; a gtest matrix. Nothing
calls it until join-backend.

### 8. [`join-backend.md`](join-backend.md) — closes [#155](../tickets/joins.md#t155), [#152](../tickets/joins.md#t152), [#173](../tickets/joins.md#t173), [#212](../tickets/joins.md#t212), [#159](../tickets/joins.md#t159), [#59](../tickets/joins.md#t59), [#80](../tickets/joins.md#t80), [#137](../tickets/joins.md#t137), [#220](../tickets/joins.md#t220), [#190](../tickets/joins.md#t190), [#207](../tickets/joins.md#t207), [#208](../tickets/joins.md#t208), and, with task 7's C++, [#136](../tickets/joins.md#t136), [#153](../tickets/joins.md#t153), [#160](../tickets/joins.md#t160), [#215](../tickets/joins.md#t215), [#63](../tickets/joins.md#t63), [#154](../tickets/corpus-coverage.md#t154) — state: approved to build

Planner, wire, both executors and the driver onto the session; every join refusal lifted but #250's off-spine IN; the
`NOT IN` rewrite; the corpus and pbench cells turned on, compared with the committed estimate.

### 9. [`verify-26.02.md`](verify-26.02.md) — closes no ticket — state: approved to build

Every tier on shad-gpu's cuDF 26.02 environment, fixes, and the tpch sf40 benchmark of the cells
the chain turned on, against 25.02; the evidence #244 waits on.

## Chain K (base: master)

> **GPU on nebius-gpu, from 2026-10-09 until the human lifts it.** It replaces the earlier "No
> GPU" note and overrides the specs, `build-test.md` and the coordinator's `done` rule wherever
> they disagree.
>
> - **GPU tests run on nebius-gpu**, `dmitry@89.169.109.150`, under chain J's host override
>   above, with every rule there unless it is named here: GPU builds and runs only, CPU work stays
>   local; cuDF 25.02; the `rsync --delete-after` sync, uncommitted; `build-test-shadgpu.sh
>   --build` and the staged binaries run directly, never `--run`; sf40 and benchmarks out.
> - **This chain works in `~/peacockdb-K`, never `~/peacockdb-J`.** The sf1 data is already in its
>   `testdata/`, hard-linked from chain J's copy; never edit those files in place.
> - **Sharing the card with chain J is allowed.** If a run fails on memory while chain J's is on
>   the card, `nvidia-smi` shows it; wait for chain J's run to end and rerun, rather than
>   ticketing it.
> - **Watch the disk; two chains now build there.** Run `df -h /` before every GPU build. Below
>   20 GB free, follow chain J's cleanup rule first; below 10 GB, do not start a build, and record
>   it in the detail file as an obstacle. Never delete anything under `~/peacockdb-J`.
> - **On resuming the chain, reset the board first, in one commit:** distinct-companions,
>   limits and empty-sorts go from `done` to `building`. Their GPU halves are now runnable:
>   distinct-companions' device cells ([#262](../tickets/corpus-coverage.md#t262)), and limits'
>   and empty-sorts' device changes and cases ([#281](../tickets/corpus-coverage.md#t281)). Each
>   task runs its GPU tests, turns on the cells that pass, and tickets those that fail.
>   guard-checks stays `done`: its GPU-side edits are to shad-gpu's script and CI job, which
>   nebius-gpu cannot exercise.
> - **`done`** is chain J's rule: CI green except the `gpu-tests` job, and the GPU tests passed
>   on nebius-gpu, recorded in the detail file.
> - **The cost gate's expected rises are accepted** (the human, 2026-10-08). limits and
>   empty-sorts are `done` when the cost-report job's only regressions are the sections their
>   detail files list from the local `--cost-diff` run: nested-limits at tp1-rowgroup and
>   tp4-rowgroup for limits (expected +228 bytes), q17 at tp1-single and tp1-rowgroup for
>   empty-sorts (expected +24 bytes). Any other regression is a finding, as usual.

Corpus-coverage tickets and #62, proved on the cpu first while chain J held the GPU host; their
GPU halves now run on nebius-gpu beside chain J. limits and empty-sorts were added after the chain
started.

### 1. [`guard-checks.md`](guard-checks.md) — closes [#233](../tickets/corpus-coverage.md#t233), [#174](../tickets/corpus-coverage.md#t174) — state: done — PR #170

The validator checks a pass-through node's column count; the Rust and C++ row-range clamps read
one shared case table, `testdata/fixtures/row-range-clamp.txt`; the driver's mock calls the
shipped clamp.

### 2. [`distinct-companions.md`](distinct-companions.md) — closes [#62](../archive/archived-tickets.md#t62) — state: reviewing — PR #171

A DISTINCT aggregate lowers to two aggregate sequences beside any per-column companion, with or
without grouping sets; #144 and #261 refused by name; the wire's `distinct` field deprecated; q28,
`tpch/rollup-distinct` and `tpch/distinct-functions` on the cpu corpus, their device cells off.

### 3. [`limits.md`](limits.md) — closes [#186](../tickets/corpus-coverage.md#t186), [#234](../tickets/corpus-coverage.md#t234) — state: rebase needed(building) — PR #172

A scan's limit becomes a `GpuLimit` above the scan and leaves both readers; the driver counts a
limit's emitted rows rather than its input; scan-limit's tp1 cpu cells on.

### 4. [`empty-sorts.md`](empty-sorts.md) — closes [#205](../tickets/corpus-coverage.md#t205) — state: rebase needed(building) — PR #174

The cpu's accumulating sort and merge answer one zero-row batch over zero-row batches; an empty
answer keeps its columns; q17's result section gains its header.

## Chain L (base: master after chains J and K have both merged)

> **Base: J and K merged, not master as it stands.** Branch the first task from master only once
> both chains are on it. J closes #152, #220 and #189, which hold most of the device cells this
> chain turns on, and adds pbench's `empty` table; K adds the DISTINCT lowering, whose device
> cells (#262) and `tpch/rollup-distinct` this chain runs. Until both have merged, no task here
> starts.
>
> **GPU host: nebius-gpu**, `dmitry@89.169.109.150`, cuDF 25.02, as chain J's header describes:
> only what needs the GPU runs there, the working tree synced uncommitted into `~/peacockdb-L`
> (its sf1 data copied from `~/peacockdb-J/testdata`), every CPU build and run local. Chain J will
> have finished with it.
>
> - **One sync per plan task** (each numbered task of a `-impl.md`) to the host, with as many
>   back-to-back builds as its red/green pairs need, the red build first. This overrides each spec's "one GPU cycle per round".
> - **Large tests and benchmark measurements are out of every task**, as in chain J: the sf40
>   binaries, `--run-benchmarks`, Nsight captures, any H200 timing. Record each skipped item in the
>   task's detail file as deferred.
> - **Done** means CI is green except the `gpu-tests` job, "GPU Tests (remote)", which runs on
>   shad-gpu, and the task's device tests have passed on nebius-gpu, the run recorded in the
>   detail file.
> - **Ticket numbers:** take the next free number from master's `tickets.md` as it stands after
>   J's merge, which reconciles J's 263–279 with K's 280 on.

The Welford and aggregate tickets of corpus-coverage, and the column references the device
aggregate reads by counting.

### 1. [`grouping-id.md`](grouping-id.md) — closes [#65](../tickets/corpus-coverage.md#t65), [#55](../tickets/corpus-coverage.md#t55), [#262](../tickets/corpus-coverage.md#t262) — state: new

The device's grouping-set id folds and widens as DataFusion's; walk tests prove q66's quotient and
the two-stage DISTINCT on a device; `tpch/rollup-grouping` added; the rollup, q66 and DISTINCT
device cells run and enabled where they pass.

### 2. [`keyless-identity.md`](keyless-identity.md) — closes [#199](../tickets/corpus-coverage.md#t199), [#282](../tickets/corpus-coverage.md#t282) — state: new

`empty_state` beside `state_type`; every aggregate init a batch accumulator that skips zero-row
batches and, through `AtDoneIfNothingOut`, answers what it owes over no rows; a scan of no row
group maps one lane with no batch; six pbench `empty-*` queries; the q96, q88 and q90 tp4 cells
on.

### 3. [`aggregate-arms.md`](aggregate-arms.md) — closes [#164](../tickets/corpus-coverage.md#t164), [#225](../tickets/corpus-coverage.md#t225), [#280](../tickets/corpus-coverage.md#t280) — state: new

The device aggregate reads every state column through its `args` and names every column from the
wire's `state_names`; one request builder; the dead arms go; `ColumnRef`s checked for bounds and
name; the scan's names checked at plan time; `tpch/rollup-stddev` added.

### 4. [`welford-device.md`](welford-device.md) — closes [#216](../tickets/corpus-coverage.md#t216), [#94](../tickets/corpus-coverage.md#t94) — state: new

A keyless stddev or var on the device through the request builder on a constant key; a NULL-free
Welford state; the `MERGE_M2` count type gated on the cuDF version; `tpch/global-stddev` added.
