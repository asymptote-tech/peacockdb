# The pool reserves what a binary needs — implementation plan

> **For agentic workers:** the coordinator dispatches one task per developer. Steps use
> checkbox (`- [ ]`) syntax. A task ends by appending its state to
> `llm-wiki/tasks/rmm-pool-budget-detail.md` and handing back — it does **not** commit.

**Goal:** stop every GPU gtest binary reserving 85% of the card, so two CI runs fit on a host we
share with strangers.

**Architecture:** `install_rmm_pool()` takes an explicit byte budget instead of a percentage of
free VRAM. Each binary declares what it measured. No clamp: a host that cannot supply the request
fails to build the pool and the binary runs unpooled, which is the existing degradation and is
loud where it matters.

**Tech Stack:** C++/cuDF, rmm `pool_memory_resource` and `statistics_resource_adaptor`, gtest,
`scripts/build-test-shadgpu.sh`.

**Spec:** [`rmm-pool-budget.md`](rmm-pool-budget.md).

## Global Constraints

- **You do not mutate git state.** Leave work in the tree; the coordinator commits.
- **Every GPU tier stays byte-identical.** This changes reservation, never computation.
- **Nothing here runs on this host.** There is no GPU locally; every measurement and every run is
  `scripts/build-test-shadgpu.sh`, **in the foreground** — a backgrounded shad-gpu cycle is killed
  mid-build with no error.
- **`.clang-format` applies to the lines you changed**, via `git clang-format`, never whole files.
- Comment caps: four lines in a body, ten above a declaration.

---

### Task 1: Measure the six peaks

Nothing is chosen in this task. `peak_allocated_bytes()` already exists in
`cpp/include/peacock/rmm_pool.hpp:180`, reading the statistics adaptor, and `begin_peak_scope()`
beside it makes a peak per test rather than per run.

**Files:**
- Modify (temporarily): the six `main()` functions, to print the peak at exit
- Create: `llm-wiki/tasks/rmm-pool-budget-detail.md` with the table

- [x] **Step 1: Print the peak at exit in each of the six**

`test_tpch.cpp:771`, `test_tpchv.cpp:1190`, `test_cudf.cpp:126`, `test_plan_executor.cpp:1370`,
`test_cudf_nodes.cpp:381`, `test_tpch_streamed.cpp:800`. One line after the gtest run:

```cpp
std::fprintf(stderr, "[rmm] peak_allocated_bytes=%zu\n", peacock::peak_allocated_bytes());
```

- [x] **Step 2: Build and run them on the device**

```bash
scripts/build-test-shadgpu.sh --build --push-binaries --patch --run
```

Run in the foreground and stay in the call. `test_cudf_nodes` and `test_tpch_streamed` are manual
tiers — run them by name rather than expecting the default set to cover them.

- [x] **Step 3: Record the six peaks**

A table in the detail file: binary, dataset, peak bytes, and whether it runs in every CI job or
only by hand. This table is the whole justification for Task 2's constants, so a missing row means
a guessed budget.

- [x] **Step 4: Revert the six print lines, hand back**

They were scaffolding. `peak_allocated_bytes()` stays; it was already there.

---

### Task 2: `install_rmm_pool` takes bytes

**Files:**
- Modify: `cpp/include/peacock/rmm_pool.hpp`
- Modify: `cpp/tests/gpu/{test_tpch,test_tpchv,test_cudf,test_plan_executor,test_cudf_nodes,test_tpch_streamed}.cpp`
- Modify: `cpp/tests/gpu/multi_gpu.cpp` (comment only)

**Interfaces:**
- Consumes: Task 1's measured peaks.
- Produces: `peacock::install_rmm_pool(std::size_t bytes)`, called by six binaries and by the FFI.

- [x] **Step 1: Change the signature and delete the percentage path from it**

`install_rmm_pool(std::size_t bytes)` aligns the request down with `pool_align_down` and builds the
pool at that size. No `cudaMemGetInfo` percentage arithmetic, no clamp. Keep `RmmPoolStatus` as it
is: `Unavailable` on failure is the contract callers already handle.

- [x] **Step 2: Keep the percentage constants for the one caller that still needs them**

`kDiscreteInitialPercent` and its three siblings stay, with a comment saying they now serve
`multi_gpu.cpp` alone — manual, two GPUs, never in CI, and so no part of #178. Do not convert that
file: it is the one caller this host cannot run.

- [x] **Step 3: Give each binary a named budget beside its `main()`**

One constant per binary, with the measurement in the comment. For example:

```cpp
// 19 MB of parquet and hand-built plans; measured peak 0.4 GiB (llm-wiki/tasks/rmm-pool-budget-detail.md).
constexpr std::size_t kPoolBytes = 2ull << 30;
```

Round up to something a reader can defend against the measured peak. The four that share a CI job
must sum to well under the device.

- [x] **Step 4: Build both cuDF legs**

```bash
scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12
```

Expected: clean. The header is included by seven translation units, so a signature change that
compiles in one may not in another.

- [x] **Step 5: `git clang-format`, append the budget table to the detail file, hand back**

---

### Task 3: The FFI symbol takes the same argument

**Files:**
- Modify: `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp:107-120`
- Modify: `peacockdb-ffi/src/` — the extern declaration and any Rust caller

**Interfaces:**
- Consumes: Task 2's signature.
- Produces: `peacock_install_rmm_pool(size_t bytes, PeacockRmmPoolInfo* out_info)`.

- [x] **Step 1: Add the parameter, keeping the return contract**

`PEACOCK_RMM_POOL_INSTALLED` / `_UNAVAILABLE` do not change meaning. Update the doc comment to say
the caller chooses the size and that an unsatisfiable request is `_UNAVAILABLE`, not a smaller pool.

- [x] **Step 2: Do not touch `gpu_memory_limit`**

It is stored at `gpu_executor.cpp:99` and never read. Making the pool honour it is
[#148](../tickets.md#t148) and a decision about the product; this task must leave it exactly as it
found it. Say so in the detail file so the next reader knows it was considered.

- [x] **Step 3: Build the FFI and run its test**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-ffi --test test_ffi
```

- [x] **Step 4: Append and hand back**

---

### Task 4: Prove it on the device, twice at once

**Files:** none — this task runs things.

- [x] **Step 1: The full GPU tier, byte-identical**

```bash
scripts/build-test-shadgpu.sh --build --push-binaries --patch --run
```

Every GPU tier passes and no golden moves. A pool size cannot change an answer; if one does, the
budget is too small and something is silently degrading rather than failing.

- [x] **Step 2: Two at once — the thing this task exists for**

On shad-gpu, start `peacock_tpch_tests` twice concurrently and wait for both.

```bash
ssh shad-gpu 'cd <REMOTE_DIR> && (./peacock_tpch_tests & ./peacock_tpch_tests & wait)'
```

Expected: both pass. Before this change the second dies in `pool_memory_resource` with
`std::bad_alloc`. This has never been run deliberately, so record the output in the detail file
whichever way it goes.

- [x] **Step 3: Record what the pool actually reserved**

`RmmPoolStatus` carries `initial_bytes` and `maximum_bytes`; confirm they are the declared budget
and not a percentage of anything.

- [x] **Step 4: Append and hand back**

---

### Task 5: The ticket

**Files:**
- Modify: `llm-wiki/tickets.md` (#178)

- [x] **Step 1: Mark #178 tentatively closed, in place**

Not archived and not deleted: the host is shared with work outside this repo, so this cannot be
proven closed from here. Say what changed — the pool no longer sizes itself against the device, so
two runs fit — and what remains unprovable.

- [x] **Step 2: Carry the instruction for whoever meets it next**

If a GPU tier fails with `std::bad_alloc` in `pool_memory_resource`: add a dated line naming the
run and the binary, re-run the job once, and do not debug it. The evidence accumulates until it
says whether the sizing was wrong or the neighbour was greedy.

- [x] **Step 3: Check the ticket still fits its cap**

Fifteen lines, one header, at most two stating the problem. If the update pushes it over, cut the
history rather than the instruction.

- [x] **Step 4: Confirm `prompts.md` already carries the coordinator's half**

The Coordinator section gained the same rule when this task was defined, because a coordinator does
not read `tickets.md`. Verify it is there and says the same thing; do not add a second copy.

- [x] **Step 5: Append and hand back for the completeness pass**
