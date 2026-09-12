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

### 2026-09-10 — developer, tasks 1-5

**Host at measurement.** The card was idle for the first three binaries, then a foreign tenant
(`kirill`, 51-53 GiB, a python venv) appeared and stayed for the rest of the session. It cannot
distort a peak — the statistics adaptor counts what cuDF *asked for*, not what the device had —
but it does bound what a pool can reserve, and it is why the two-at-once run below is not the
clean experiment the spec wanted.

#### Task 1 — the six measured peaks

Each binary printed `peak_allocated_bytes()` after `RUN_ALL_TESTS()` (scaffolding, since
reverted). The counter is the run's high-water of *live requested* bytes: the adaptor sits above
the pool, and `pop_counters` folds each test's scope into the run's, so the number is the largest
working set the binary ever held.

| Binary | Source | Dataset | Runs | Peak bytes | GiB | Where the peak is |
|---|---|---|---|---|---|---|
| `peacock_gpu_tests` | `test_cudf.cpp` | none — 6-row literals | every gpu-tests job | 912 | 0.0000008 | murmur3 over 6 rows |
| `peacock_plan_tests` | `test_plan_executor.cpp` | `tpch.minimal`, 19 MB | every gpu-tests job | 3,031,456 | 0.0028 | across 27 hand-built plans |
| `peacock_tpch_tests` | `test_tpch.cpp` | `tpch.sf40` | every gpu-tests job | 72,391,864,720 | 67.42 | `Q1GroupByAggregates` |
| `peacock_tpchv_tests` | `test_tpchv.cpp` | `tpch.sf40` + embeddings | every gpu-tests job | 26,677,262,880 | 24.85 | `Q11VectorBruteForce` |
| `peacock_cudf_node_tests` | `test_cudf_nodes.cpp` | `tpch.sf40` lineitem | manual | 9,608,558,464 | 8.95 | `CudfNodes.OperatorTimings` |
| `peacock_tpch_streamed_tests` | `test_tpch_streamed.cpp` | `tpch.sf40` | manual, **26.02 only** | 604,797,680 | 0.56 | `Q1Streamed` |

Per-test peaks, for the two suites where they differ: tpch q6 19.72, q1 67.42, q3 13.96, q8 21.24;
tpchv q11v 24.85, q12v 21.63, q10v 22.68, q9v 24.78; streamed q6 0.21, q1 0.56, q3 0.41, q8 0.33.

The 67.42 GiB figure independently confirms the one already written on #178.

**The two manual binaries needed work to measure at all.** Both are `EXCLUDE_FROM_ALL` and neither
is installed, so `--push-binaries` never carries them. `peacock_cudf_node_tests` builds on the
25.02 leg and was shipped to `$REMOTE_REPO/scratch-manual/`. `peacock_tpch_streamed_tests` does
**not** build on 25.02 at all — it includes `cudf/join/filtered_join.hpp`, which is 26.02-only, as
its own comment says — so it was built from `cpp/build26` against `~/data/miniforge3/envs/rapids`
(26.02) and run against the host's `~/miniforge3/envs/rapids-2602/lib`. Both had their interpreter
patched by hand (`patchelf --set-interpreter /home/info/glibc-2.35/lib/ld-linux-x86-64.so.2`);
neither belongs in `cpp/install/bin`, where the gate's `peacock_*_tests` glob would sweep them in.

#### Task 2 — the budget is NOT the peak, and the device said so

The first pass took each peak, rounded it up, and went red: `peacock_tpch_tests` at a 68 GiB budget
(peak 67.42) and `peacock_tpchv_tests` at 28 GiB (peak 24.85) both died with
`std::bad_alloc: out_of_memory ... Maximum pool size exceeded`.

Root cause, from `pool_memory_resource.hpp` in the 25.02 env: when `initial == maximum`,
`size_to_grow()` returns 0 for any request the free list cannot serve whole, and `try_to_expand`
fails immediately. The pool can never take another upstream block, so its *arena* has to be
bigger than the peak of requests by whatever the allocation pattern leaves unusable. q1 died
asking upstream for 3.53 GiB with 3.27 GiB nominally free inside a 68 GiB pool.

So the budgets were bisected on the device — a rebuild of one target, an rsync of one binary, a
`patchelf`, a run, per step:

| Binary | Peak | Fails at | Passes at | Declared | Why that number |
|---|---|---|---|---|---|
| `peacock_tpch_tests` | 67.42 GiB | **68 GiB** | **69 GiB** | 69 GiB | the threshold, and the most that lets two share a 139.7 GiB card |
| `peacock_tpchv_tests` | 24.85 GiB | **28 GiB** | **29 GiB** | 30 GiB | the round step above the threshold |
| `peacock_cudf_node_tests` | 8.95 GiB | — | 10 GiB | 10 GiB | confirmed on device first try |
| `peacock_tpch_streamed_tests` | 0.56 GiB | — | 2 GiB | 2 GiB | covers the `PEACOCK_STREAM_CHUNK_MB` sweep |
| `peacock_gpu_tests` | 912 B | — | 1 GiB | 1 GiB | a floor; nothing here can approach it |
| `peacock_plan_tests` | 2.9 MiB | — | 1 GiB | 1 GiB | the same floor |

The four CI binaries sum to 101 GiB against a 139.7 GiB device, and they run one at a time.

**`peacock_tpch_tests` is the binary that cannot be made comfortable.** Its budget has ~1.6% over
its peak because the device caps it: 70 GiB would be a kinder margin and two of those do not fit.
Whichever way that trade is taken, one of the spec's two asks gives — and the ask the spec calls
"the failure this task exists to make unreachable" is the two-at-once one.

**The integrated percentages were deleted, not kept.** The plan said all four constants stay; only
`kDiscreteInitialPercent` / `kDiscreteMaximumPercent` have a reader after this change, and
`multi_gpu.cpp` takes the discrete pair unconditionally (multi-GPU means discrete parts). A
constant nothing reads cannot carry the comment the spec asks for — "they serve `multi_gpu.cpp`
alone" — so the integrated pair went to `llm-wiki/archive/historical-comments.md`, which is where
`coding-style.md` sends reasoning whose code has gone. `llm-wiki/reports/dgx-spark.md` keeps the
measured row it justified.

`PEACOCK_RMM_POOL_INIT_PCT` went with them: it overrode a percentage that no longer exists, and
with `initial == maximum == the budget` the question it was there to answer ("is this cost pool
growth?") is answered by construction — there is no growth. `dgx-spark.md` says so now.

#### Task 3 — the FFI

`peacock_install_rmm_pool(uint64_t bytes, PeacockRmmPoolInfo* out_info)`. `uint64_t` rather than
the plan's `size_t`: every other byte count in `peacock_gpu.h` is `uint64_t`, and one name for one
thing across the FFI is the rule that matters here. No Rust caller exists — the extern declaration
in `peacockdb-ffi/src/lib.rs` is the whole Rust surface — so nothing else moved.

**`gpu_memory_limit` was considered and left exactly as found** (`gpu_executor.cpp:99`, stored and
never read). A pool's size is the natural home for it, which is precisely why touching it is #148
and a decision about the product: it would change where every shipping query's memory comes from.
The doc comment on `peacock_install_rmm_pool` now says the entry point does not read it.

#### Task 4 — on the device

Full GPU tier, `scripts/build-test-shadgpu.sh --run`, exit 0: C++ 11 + 5 + 27 + 4 + 4 = 51 cases,
rust 4 + 8 + 31 + 10 + 10 = 63 cases, no skips, no golden moved. Every golden this tier asserts is
read inside those tests; nothing regenerated anything. The pool lines read exactly the declared
budgets — `1.0`, `1.0`, `69.0`, `30.0 GiB reserved` — so `initial_bytes == maximum_bytes ==` the
constant, and no percentage of anything survives in the output.

**Two at once, first form: two `peacock_tpchv_tests`, both passed.** With the foreign tenant still
holding 53 GiB, instance A saw 86.9 GiB free and took its 30 GiB; instance B saw 56.9 GiB free and
took *the same 30 GiB*, which is the whole point — under the old rule B would have asked for 85% of
what A had left. Both exited 0, 4 tests each.

**Two at once, second form: two `peacock_tpch_tests`, and the second failed.** A took its 69 GiB;
B found ~21 GiB, could not build a 69 GiB pool, said so, and ran on the default resource, where
three of its four tests died with `cudaErrorMemoryAllocation`. That is the designed degradation
working — loud, at the pool line, before any test ran — but it is **not** a clean test of the
spec's requirement: 69 + 69 = 138 GiB needs an otherwise-idle 139.7 GiB card, and a stranger held
a third of it. The experiment wants a re-run on an idle device; see the run log entry that follows
for whether one was possible.

#### Local verification

- 25.02 C++ build: clean, no new warnings.
- 26.02 C++ build (`cpp/build26`): clean; the only warnings are the two pre-existing
  `cudf::round` / `cudf::strings::like` deprecations in `src/expr.cpp`.
- FFI: `scripts/cargo-cudf.sh test -p peacockdb-ffi --test test_ffi` — 2 passed, 0 failed.
- `git clang-format` reports no modifications.

**A trap for the next developer on this host.** `scripts/build.sh --configure` passes only
`-Dcudf_ROOT`, and `find_package(cuvs REQUIRED CONFIG)` takes no hint from it, so a cold
`cpp/build` in a fresh worktree fails at configure with "Could not find a package configuration
file provided by cuvs". CI does not hit it because the rapids container has the conda prefix on
`CMAKE_PREFIX_PATH`. Export `cuvs_ROOT=$CUDF_ROOT` before the first configure of a new build dir.

#### What is left undone: two `peacock_tpch_tests` on an idle card

The card was polled for 25 minutes after the gate went green (13:03-13:28 local) and the foreign
tenant never dropped below 53 GiB, so the spec's exact experiment — two `peacock_tpch_tests`, both
pooled, both green — was not reachable in this session. What it needs is an idle device and one
command:

```bash
ssh shad-gpu 'PATCHED_LD=/home/info/peacockdb/cpp/install/lib:/usr/local/cuda-12.5/compat:/home/info/glibc-2.35/lib:$HOME/miniforge3/envs/rapids-cuda-12.2/lib:$LD_LIBRARY_PATH
run() { env LD_LIBRARY_PATH="$PATCHED_LD" PEACOCK_TESTDATA_DIR=/home/info/peacockdb/testdata \
  PEACOCK_TPCH_SF40_DIR=/home/info/peacock-datasets/testdata/tpch.sf40 \
  PEACOCK_TPCH_GOLDEN_DIR=/home/info/peacockdb/testdata/goldens/tpch.sf40 \
  /home/info/peacockdb/cpp/install/bin/peacock_tpch_tests > /tmp/tpch.par.$1.log 2>&1; echo "$1 rc=$?"; }
(run A & run B & wait)'
```

The arithmetic it is testing: 69 + 69 = 138 GiB against 139.74 GiB free on an idle card, so the
second pool has about 1.2 GiB of slack once the first has its 69 GiB and both processes hold a
CUDA context. It should fit and it is close, which is the honest state of it — the same sentence
that says the budget could not be given a kinder margin. If it does not fit, the second run says
so at its pool line and carries on unpooled, which is the failure mode the design chose.

The tpchv pair is the same experiment one size down and it passed with a stranger on the card,
which is the part that could be proven here.

#### Host left as found

`cpp/install/bin` on shad-gpu holds the five CI binaries and nothing else; the two scratch
directories used for the manual binaries (`scratch-manual`, `scratch-2602`) were removed. The
patched binaries on the host are the ones this branch built.

### 2026-09-10 — rebase onto master, on the control file's word

The control file said `rebase` while the developer was still in its dispatch. Committed the work
first, then rebased `ENS-rmm-pool-budget` from `d0670c3e` onto `f6dcde07` — ten commits, and
**every one of them documentation**: `llm-wiki/coding-style.md`, `tasks.md`, the two layout specs
and their impl plans, and a new task 6 (`visibility.md`) with its plan. No code, no test, no
golden, no workflow.

So the rebase re-verifies nothing and the task keeps the state it held. `pipeline.yml` skips a
wholly-documentation diff for the same reason, so a CI re-run would report green having built
nothing.

No conflict. Master's copy of the board carries the task list and the prose — the human unblocked
tasks 4 and 5 and added task 6 — and this branch's copy carries task 3 at `building`; the two
edits did not touch the same lines. Nothing above this task has a branch, so there was nothing to
mark `rebase needed`. Control file cleared.

### 2026-09-10 — pushed, PR #144, reviewing

Base `master`, three commits — the board write, the change, the rebase note. Commit count checked
against the task, since a PR aimed past its parent carries the earlier tasks' commits.

**The tpch pair is still unproven and it is a neighbour, not a bug.** Checked the card at the
transition: `1763830 /home/kirill/sdg-moe-transfer-.../python` holding 53075 MiB of 143771, leaving
90009 free. 69+69 needs 138, so the run cannot be attempted, and #178's own instruction says not to
spend a dispatch on a machine we do not own. Retry when the card is idle:

    ssh shad-gpu 'nvidia-smi --query-gpu=memory.free --format=csv,noheader'

and the two-at-once command is recorded above under the developer's Task 4.

**Open question for the review**, and the one thing in this diff that is not simply measured: 69 GiB
is 49% of the card, so two tpch runs fit only where nothing else is resident. That number is pinned
from below by q1 failing at 68, and it is pinned there because `initial == maximum` makes the pool
unable to grow. Whether a growable pool — a smaller initial with the budget as the maximum — is the
better shape was not asked in the spec, and the answer changes what "the pool reserves what a binary
needs" means.

### 2026-09-10 — review round 1: 1 blocking, 2 important, 6 nits

The code change itself came through clean: no clamp survives, every budget traces to the peak
table, `gpu_memory_limit` is untouched, the percentages are confined to `multi_gpu.cpp`, and the
four CI binaries run one at a time inside `pipeline.yml`'s `for` loop, so their 101 GiB never
coexists.

**Blocking — the retry-don't-debug instruction was keyed to a signature this change inverts.**
Both directions were wrong after the branch. A neighbour can no longer produce `std::bad_alloc` in
`pool_memory_resource`: the pool is taken whole up front, so a greedy tenant now fails at *install*
with `[rmm] pool of N GiB could not be built`, and the tests die of `cudaErrorMemoryAllocation` —
exactly what this branch's own two-at-once run recorded. Meanwhile `Maximum pool size exceeded`
from `pool_memory_resource` now means the opposite: the pool *was* built and the declared budget is
too small. That is ours, it reproduces every time, and with `peacock_tpch_tests` 1.6% over its peak
it is the failure most likely to arrive next — so "re-run once and do not debug it" would have
buried a real budget regression under dated lines forever.

Fixed in both texts, keyed on the pool line rather than the exception: `llm-wiki/tickets.md` #178
(still at its fifteen-line cap; the `pipeline.yml:448` reference paid for it) and the Coordinator
section of `llm-wiki/prompts.md`.

**Important — `rmm_pool.hpp` justified reporting-instead-of-aborting with two false claims.** It
said the correctness binaries are still right without a pool and that any caller taking a timing
refuses the run itself. Nothing acts on `Unavailable` — the six `main()`s discard the return and
the FFI only forwards it — and
`tpch_golden.hpp:182` silently switches its peak source to a free-memory delta. The branch's own
run shows an unpooled `peacock_tpch_tests` losing three of four tests. Carried over from master,
but master's percentage sizing made `Unavailable` nearly unreachable and this change makes it the
ordinary shared-card outcome, so the sentence became load-bearing and false. Rewritten to say what
happens.

**Important — the growable pool, answered.** A smaller initial with the budget as the maximum does
*not* solve this better. `initial == maximum` is what buys the loud early failure; the alternative
puts the failure back mid-query as `bad_alloc` inside `pool_memory_resource`, which is the precise
signature #178 was filed about and the one a coordinator is told not to debug. It also reinstates
growth events, which `reports/dgx-spark.md` measures at 5x. What the shape costs is margin — 69 GiB
is pinned from below by q1 and from above by the two-at-once arithmetic, on one bisection, one
dataset, one cuDF version — and that cost is tolerable only because a margin failure is now
legible. The blocking fix is what makes it legible.

Nits dropped, except three that cost nothing where I was already editing: the declaration comment
in `rmm_pool.hpp` was 11 lines against the 10-line cap, `IDEMPOTENT` was capitals for emphasis, and
`reports/dgx-spark.md`'s "(default)" row is no longer a default.

### 2026-09-10 — round 2 out, CI armed, card still held

Round 1's three findings were applied by the coordinator rather than a developer: all of them were
markdown or code comments, which are the coordinator's to write. Commit `305c0da1`, pushed. The
same reviewer was asked to confirm the fixes rather than a fresh one, so it checks its own wording
against what landed.

CI run `34529171969` is on `305c0da1`, the first head since `4f73be96` that carries a code file —
the two heads between them were documentation and took the changed-paths skip, which is not a gate.
That distinction is what `done` turns on, so the run to read is this one.

shad-gpu re-checked twice: pid `1763830` still holds 53066 MiB, 90009 free of 143771. 69+69 needs
138, so the tpch pair remains unrunnable. The first ssh timed out and the second answered — the
flaky link `build-test.md` warns about, not a host that went away.

### 2026-09-10 — round 2 closed, task at `completing`

The reviewer checked `305c0da1` against what it meant and confirmed all three: the strings the two
instruction texts now name match the code literally, the catch comment is at its four-line cap and
its three claims are ones the tree supports, and the growable-pool argument is recorded as it was
made. No blocking or important finding is outstanding.

Two corrections to my own counting, neither actionable: the declaration comment is 10 lines, not 9
— at the cap with no spare line — and #178 is 14 non-blank lines against a file whose other tickets
sit at 15.

One residual imprecision it would have dropped, fixed anyway because I was already in the line:
"no caller inspects the status" is loosely false, since `gpu_executor.cpp:123-130` switches on
`status.state` to fill `out_info`. What is true is that nothing *acts* on `Unavailable` — the six
`main()`s discard the return and the FFI only forwards it. Both the comment and the paragraph above
now say that.

### 2026-09-10 — the completeness pass: two readings, eight findings

**What is wrong** (reviewer) and **what is missing** (analyst), taken separately and compared
here. Nits dropped on both sides. The two lists overlapped on exactly one thing — the deleted
integrated regime — which each reached from its own end.

Applied by the coordinator, all markdown or comments:

- **blocking** — `rmm_pool.hpp:53-56` still carried, verbatim from master, the claim round 1
  recorded as fixed: "the gtest binaries carry on regardless … a caller taking timings asserts".
  Round 1's rewrite landed only on the catch block, so the header argued both sides, and the false
  half sat above `State::Unavailable` itself, where a reader looks first. `peacock_gpu.h:77-78` had
  the matching residue — "sizes in bytes, 0 unless INSTALLED" — while `free_bytes` is deliberately
  filled on failure and copied out at `gpu_executor.cpp:132`. Both rewritten.
- **blocking** — `build-test.md` said nothing about a change to how the tree is tested, which is
  the one page that correction is owed in the same commit. A GPU binary no longer adapts to what is
  free: below its budget it does not shrink, it runs unpooled, and for the sf40 pair that is red
  rather than slow. Added the budgets, the line to read first, and that these are H200 numbers
  verda-gpu has never been sized against.
- **important** — [#148](../tickets.md#t148) pointed at deleted code. Its "size by host kind" care
  note named an implementation that went with the percentages, and "a pool's `maximum` IS that
  bound" describes the growable shape #178 rejected. Both are now open questions on the ticket
  rather than settled instructions.
- **important** — the archive heading read "(removed 2026-09-10)", which makes a live constraint
  look like history. The integrated *reason* did not go away: every budget is an H200 number, so on
  GB10 `peacock_tpch_tests` asks for 69 GiB of a machine whose 121.7 GiB is also its system RAM,
  which is the case the 25% initial existed to prevent.
- **important** — #178 claimed "two of ours fit a 139.7 GiB device" and offered only the tpchv pair
  as evidence. It now says the tpch pair is arithmetic and not yet a run.

Handed to a developer, because they are code rather than comments — see the next entry.

**`architecture.md` was falsified nowhere**, and that is an answer rather than a gap. All four
passages that mention the pool or the ABI describe where the symbol lives and who calls it; the
page never described the sizing rule, so nothing about it moved. It does not describe the new rule
either, and that is deliberate — growth on that page waits for a human to ask.

**On the unproven tpch pair, both readings landed in the same place**: it does not undermine the
change, but it leaves the headline number a prediction. The tpchv pair proves the rule — a fixed
budget does not shrink when a neighbour arrives, instance B took its declared 30 GiB from 56.9 GiB
free — and says nothing about the number: 30+30 has 80 GiB of slack where 69+69 has 1.2.

### 2026-09-10 — the completeness pass, the code half: five changes and four device runs

The markdown and comment findings were applied by the coordinator (entry above). These five are
code, and each was driven by a failing check before it was written.

| # | Change | Where | Proved red by |
|---|---|---|---|
| 1 | the failure line carries what the device had | `rmm_pool.hpp` | observation on device, below |
| 2 | `PEACOCK_RMM_POOL_BYTES`, explicit bytes | `rmm_pool.hpp` + 3 comments | override ignored: 1.0 GiB with the var set to 2 |
| 3 | a request that aligns down to zero is `Unavailable` | `rmm_pool.hpp` | `test_ffi.rs`, INSTALLED with a 0-byte pool |
| 4 | the pool is the declared budget | `test_cudf.cpp` | percentage sizing restored: 79759834880 vs 1073741824 |
| 5 | `PEACOCK_TPCHV_K` named in the budget comment | `test_tpchv.cpp` | — comment |

**The override is `pool_budget_bytes(declared)`, read once at the top of `install_rmm_pool`.** The
test in (4) asserts against that function rather than against `kPoolBytes`, so it holds under a
sweep instead of failing on one — and it still fails the moment a pool stops being the request.
`peacock_gpu_tests` is the only binary with a case in it, which is the point: it is in every
gpu-tests job and costs nothing.

**Zero is rejected before the idempotence latch**, so a caller that asked for no pool has not
spent the one installation the process gets. `PEACOCK_RMM_POOL_BYTES=0` is therefore a way to run
any of these unpooled, and it says so on stderr.

#### On the device — shad-gpu, 90009 MiB free of 143771, pid 1763830 (`kirill`) holding 53066

The tenant never moved, so **two `peacock_tpch_tests` was not attempted**: 69+69 needs 138 GiB.
Unchanged from the entry above; that experiment still wants an idle card.

**The GPU tier is green and one case larger.** `scripts/build-test-shadgpu.sh --build`,
`--push-binaries`, `--patch`, `--run` as four foreground calls, exit 0. C++ 11 + **6** + 27 + 4 + 4
= 52 cases (was 51; the new one is `RmmPool.ReservesTheDeclaredBudget`), rust 4 + 8 + 31 + 10 + 10
= 63, no skips, no golden moved. The four pool lines read `1.0`, `1.0`, `69.0`, `30.0 GiB reserved
of 87.4 GiB free`. Run twice, identically: the second cycle was for two comment-only hunks landed
after the first, so the green tier is the tree as it stands rather than the tree minus two comments.

**`peacock_tpch_tests` in benchmark mode at 69 GiB — the run that had never been made — passes.**
`PEACOCK_BENCHMARK=1 PEACOCK_BENCHMARK_RUNS=5`, 4 tests, exit 0. Six executions of q1's closure
through a pool that cannot grow do not fragment its free list: the peak is 67.42 GiB, the same
number one execution gives, because each run frees what it took before the next asks. That was
the open risk at 1.6% headroom and it is now measured rather than hoped.

| q | execute ms (2nd-min of 5) | all five | peak |
|---|--:|---|--:|
| q6 | 63.610 | 60.0 63.6 76.7 99.5 111.5 | 19.72 GiB |
| q1 | 529.478 | 525.9 529.5 538.9 552.8 560.6 | 67.42 GiB |
| q3 | 138.869 | 138.5 138.9 177.7 184.2 186.6 | 13.96 GiB |
| q8 | 286.586 | 237.4 286.6 290.3 292.2 294.8 | 21.24 GiB |

**Do not publish those times.** They are 2.2-6x `reports/benchmark-minimal.md`'s H200 column
(19.2 / 239.9 / 40.1 / 47.7 ms) because a foreign tenant was computing on the card throughout.
The 2nd-minimum protocol filters a jittery neighbour, not a resident one. The peaks are unaffected
— the statistics adaptor counts what cuDF asked for — and the peaks are what this task is about.

**The two sweep configurations the reports publish, with the override.** Both were run at the
knob the report names, and both peaks are new to this file.

| binary | knob | peak | budget it needs | result |
|---|---|--:|---|---|
| `peacock_cudf_node_tests` | `PEACOCK_NODES_ROWS=100000000` | **17.90 GiB** | 24 GiB via the override | 1 test, exit 0 |
| `peacock_tpch_streamed_tests` | `PEACOCK_STREAM_CHUNK_MB=512` | **0.56 GiB** (q1; q6 0.21, q3 0.41, q8 0.33) | its declared 2 GiB is enough | 4 tests, exit 0 |

The node sweep is the case the override exists for: at its declared 10 GiB the published 100M-row
configuration dies with `Maximum pool size exceeded` after reaching 7.57 GiB, and nothing but a
rebuild could have run it before this change. 24 GiB was a first try, not a bisection.

**The streamed sweep does not need it here, and that is a host fact rather than a knob fact.**
`benchmark-minimal.md`'s 2340 MiB q1 peak at 512 MiB chunks is a GB10 number; the same chunk size
on H200 peaks at 577 MiB, four times smaller. So the 2 GiB budget holds on shad-gpu and would not
hold on GB10 — which is the same sentence `build-test.md` already carries about these being H200
numbers, now with a measurement under it. Both reports were corrected.

**Also measured, since each is one line of evidence for a claim in the diff**: a 1 PiB request
prints `[rmm] pool of 1048576.0 GiB could not be built with 87.4 GiB free (...)` — the figure
finding (1) asked for, without which that line is a constant; and a 100-byte request prints the
new granularity line and the binary carries on, 2 tests passing on the default resource.

#### For the coordinator: one page this change makes incomplete

`build-test.md`'s budget bullet is yours, and it now needs the override, since a GPU binary swept
past its default is unrunnable without it. Suggested, after "verda-gpu has never been sized against
them": *A run that sweeps a knob past its default sets `PEACOCK_RMM_POOL_BYTES=<bytes>` for that
run — explicit bytes, no percentage — which is also the only way a non-H200 host runs these at
all.* `#178` and the Coordinator section of `prompts.md` quote `[rmm] pool of N GiB could not be
built`, which is still a literal prefix of the line; the free figure now follows it, so the
attribution those two texts ask a reader to make is finally supported by what is printed.

#### Host left as found

`scratch-rmm` removed; `cpp/install/bin` holds the five CI binaries and nothing else. The two
manual binaries were built here (25.02 `cpp/build` for the node timings, 26.02 `cpp/build26` for
the streamed one, run against the host's `rapids-2602` env) and shipped to that scratch directory,
never to `install/bin`, where the gate's glob would sweep them in.

### 2026-09-10 — completeness approved, waiting on CI

Signoff written at the end of the spec. `build-test.md` gained the override sentence the developer
flagged, so the page now says both that a budget is a hard floor and how to move it.

**The run that decides `done` is `34533749485`, on `a02e8cb5`.** It is a full run, not a skip: the
changed-paths gate compares the *push* range rather than the PR, and that push carried `c3b4910b`
— the code — together with the documentation head above it. The five earlier green runs on this
branch are a mix of full runs and documentation skips, so none of them substitutes for it.

If a restart lands here: the task is `completeness approved`, everything is pushed, and the only
remaining transition is `done` once that run is green. Nothing else is outstanding except the tpch
pair, which is named in the signoff and is not a blocker.

### 2026-09-10 — done

CI run `34533749485` on `a02e8cb5` is green on every job: both cuDF legs, the 25.02 GPU build, the
remote GPU tier, the cost report, changed-paths and the S3 metadata check. The GPU tier is the one
worth naming — it ran the new budgets on shad-gpu with the foreign tenant still holding 53 GiB, so
the four CI binaries took 1, 1, 69 and 30 GiB out of what was left and passed.

`f946df6d` sits above it and is documentation, so its own run is a changed-paths skip. That is not
a gate, and the head that carries code is the one `done` asserts.

Terminal for the ensemble. The human merges.

### 2026-09-11 — rebased onto master `02069415`; `rebase needed(done)` until re-proven

The control file said `rebase`. Master carried `62335cff` (`cpp/CMakeLists.txt`: `cudf_ROOT`
seeds `CMAKE_PREFIX_PATH`) and `02069415` (`build-test-shadgpu.sh` and `setup-glibc.sh` patch to
the build host's glibc) past `188c23ce`, plus a docs-only report commit. The rebase applied
clean, 12 commits, no conflict. Not documentation alone, so the task is `rebase needed(done)`
until a developer re-runs the shad-gpu cycle from this box — now Ubuntu 24.04 / glibc 2.39, the
host class `02069415` exists for — and CI is green on the rebased PR #144. The worktree has no
`cpp/build26` or `target-cudf-*` yet, so this cycle's builds are cold.

### 2026-09-11 — re-proven after the rebase: the same 52 + 63, at 2.39, in 12 minutes

The proving cycle from `d41f223a` on the reprovisioned dev box (Ubuntu 24.04, glibc 2.39, 8 cores,
31 GiB), four phases as four calls, every one exit 0. Timestamps are UTC and cross midnight.

| phase | exit | elapsed | note |
|---|--:|--:|---|
| `--build` (cold: no `cpp/build26`, no `target-cudf-*`, ccache 0 hits) | 0 | 9m55s (23:58:37 → 00:08:32) | configure found `cuvs 25.02.01` and `Arrow 19.0.1` under `cudf_ROOT` — `62335cff` holds; 81 ninja steps, 5 gtest binaries, 5 rust binaries staged, no warnings |
| `--push-binaries` | 0 | 9s | 0 rsync retries; `sha256sum` of all 11 shipped files agrees on both hosts |
| `--patch` | 0 | 3s | `glibc 2.39 already installed in /home/info/glibc-2.39; skipping the build`; every shipped executable verified against `/home/info/glibc-2.39/lib/ld-linux-x86-64.so.2` — `02069415` holds, no `version GLIBC_2.3x not found` anywhere |
| `--run-detached` + `--run-status` | 0 | 73s (run `20260912T001212-111418`, 00:12:12 → 00:13:25) | `GPU test run OK` |

Pool lines, in run order: `1.0`, `1.0`, `69.0`, `30.0 GiB reserved of 103.0 GiB free`. C++
11 + 6 + 27 + 4 + 4 = 52. Rust `test_gpu_abi` 4, `test_gpu_corpus` 8, `test_gpu_executors` 31,
`test_gpu_recipe_walk` 10, `test_inc2_conformance` 10 = 63. No skips, no failures, no golden
written. Counts identical to the green cycle before the rebase.

Cheap guards first, on the rebased tree in `./target`: `test_ci_coverage` 7 passed,
`test_module_layout` 11 passed, both `--features rust-only`, both exit 0.

What differed from the previous cycle, none of it a finding: the free figure is 103.0 GiB rather
than 87.4 because the neighbour (pid 1814433) held 37 GiB instead of 53; the binaries are patched to
2.39 rather than 2.35 because this is a 24.04 host, which is the point of the rebase; the cold build
took ten minutes rather than the hour budgeted, since this box has 8 cores and 31 GiB. A CI job
(`peacockdb-ci/run-34659402333-1`, its own tree) was running `test_gpu_corpus` when the build
started and had finished before the gate launched, so no collision and no #178 line. The host's
stale `/home/info/peacockdb/cpp/build/` still holds three old gtest binaries that `--patch` also
rewrites; the gate globs `cpp/install/bin` and never sees them.

Host left as found: 37 GiB neighbour, 106 GiB free, nothing of ours running. Local tree clean but
for this entry; `cpp/build26`, `cpp/install`, `target-cudf-rapids-cuda-12.2` are gitignored build
products and stay warm for the next cycle.
