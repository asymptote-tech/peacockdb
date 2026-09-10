# rmm-pool-budget — run detail

Spec: [`rmm-pool-budget.md`](rmm-pool-budget.md). Plan: [`rmm-pool-budget-impl.md`](rmm-pool-budget-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 3. Branch `ENS-rmm-pool-budget`, forked at `d0670c3e`.

**Its PR targets `master`, not task 2's branch.** Tasks 1 and 2 are merged: `master`,
`origin/master` and `ENS-drop-mode-name` all sit on `d0670c3e`, and the human squashed task 2's
work into `beb7455e` there. `ENS-module-layout` is the stale pre-squash branch and has an
unrelated history (merge-base `54497ae8`), so aiming this PR at it would put the whole squash
delta in the diff. Master is the parent that exists.

## Host state at dispatch (2026-09-10)

- **shad-gpu** up: `llm-gpu0h200`, NVIDIA H200, 143771 MiB total, **143084 MiB free** — the card
  was idle, so peaks measured now are not distorted by a neighbour.
- **verda** up: `wide-hand-falls-fin-03`, 8 cores, 31 GiB. Nothing in this task needs it; the FFI
  test is a local run.
- ssh from a coordinator's Bash tool needs the sandbox disabled; the developer's does not appear to.

## Facts checked against the tree before dispatch

Every line number the plan cites still holds at `d0670c3e`:

| Claim | Where |
|---|---|
| six `install_rmm_pool()` callers | `test_tpch.cpp:771`, `test_tpchv.cpp:1190`, `test_cudf.cpp:126`, `test_plan_executor.cpp:1370`, `test_cudf_nodes.cpp:381`, `test_tpch_streamed.cpp:800` |
| the function itself | `cpp/include/peacock/rmm_pool.hpp:114`, percentage arithmetic at 136-141 |
| `pool_align_down` / `peak_allocated_bytes` / `begin_peak_scope` | same header, 55 / 180 / 189 |
| FFI symbol | `cpp/src/gpu_executor.cpp:107`, declared `cpp/include/peacock_gpu.h:104`, extern in `peacockdb-ffi/src/lib.rs:73` |
| `gpu_memory_limit` stored and never read | `cpp/src/gpu_executor.cpp:99` — leave alone, that is #148 |

`multi_gpu.cpp` is the seventh caller and installs its own per-device pools (line 64-68) from
`kDiscreteInitialPercent` / `kDiscreteMaximumPercent`. It keeps them. **Its comment at line 56
becomes false** — it says the percentages are shared so "the three installers" agree, and after
this task it is the only one left. That comment is part of the change.

## Remote invocation facts

`scripts/lib/shadgpu-env.sh`: `REMOTE=shad-gpu`, `REMOTE_REPO=/home/info/peacockdb`. Binaries land
in `$REMOTE_REPO/cpp/install/bin/peacock_*_tests`. A binary needs the patched loader path, applied
per command and never exported (this host's coreutils segfault against glibc-2.35):

    PATCHED_LD=/home/info/peacockdb/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:$HOME/miniforge3/envs/rapids-cuda-12.2/lib:$LD_LIBRARY_PATH

and the sf40 suites need `PEACOCK_TESTDATA_DIR=/home/info/peacockdb/testdata`,
`PEACOCK_TPCH_SF40_DIR=/home/info/peacock-datasets/testdata/tpch.sf40`,
`PEACOCK_TPCH_GOLDEN_DIR=/home/info/peacockdb/testdata/goldens/tpch.sf40`,
`PEACOCK_TPCH_VEC_PARAMS=/home/info/peacockdb/testdata/tpch-vec-queries/query_params.jsonl`.
Without them they fall back to a relative golden path and fail as a mis-provisioned run.

`--build`, `--push-binaries`, `--patch --run` are three separate foreground calls, not one chain:
a backgrounded shad-gpu cycle has been killed mid-build with no error and nothing staged.

## Run log

### 2026-09-10 — dispatch 1

Board set to `building`. Developer dispatched with the plan's five tasks. Nothing measured yet.
