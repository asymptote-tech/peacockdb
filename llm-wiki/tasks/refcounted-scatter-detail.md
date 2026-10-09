# refcounted-scatter — run detail

## Dispatched (2026-10-09)

Branch `ENS-refcounted-scatter`, forked off `ENS-repartition-keys` at `ec1e6d79`. Tasks 1–3 of the
chain are all `done` on master `f0a6ecbf` after this run's rebase; this is the first task the chain
has built since.

**Its place in the chain, which decides what may not change.** `TableResult` takes its final shape
here, once, and `exit-copies` (task 6) and `join-session-cpp` (task 7) are written against it
without changing it again. So the shape is the deliverable as much as the scatter is.

**What the host override takes out of the spec.** The spec's verification bar asks for benchmark
timings before and after, and its device workflow asks for `build-test-shadgpu.sh` two cycles.
shad-gpu is down, so: `--build` only and the staged binaries run directly on nebius-gpu
(`dmitry@89.169.109.150`, L40S, cuDF 25.02), and **every benchmark measurement is deferred** —
`--run-benchmarks`, Nsight, any H200 timing. The spec's own §4 and its Tests section ask for two
things that are *not* benchmark runs and are therefore still in: the per-partition timers dropping
toward zero, and the hand-recorded peak-memory drop from the RMM statistics adaptor. Those are
gtest-scale measurements on the card and they stay.

**The two measurements the spec wants by hand**, because no assertion can hold them: the peak
during the scatter call (the partitioned table stays resident afterwards either way, so a
before-minus-after bound cannot work), and the per-partition timer collapse. Both go in this file.

## Round 1 (2026-10-09) — built, green, and the two hand measurements taken

Host: nebius-gpu `dmitry@89.169.109.150`, L40S 46 GB, cuDF 25.02
(`~/data/miniforge3/envs/rapids-cuda-12.2`), sf1 testdata in `~/peacockdb-J/testdata`. Built with
`./scripts/build-test-shadgpu.sh --build` on the host and the staged binaries run directly, per
the human override; shad-gpu never touched. Every build reported **0 warnings**.

### The four reds, each watched

| step | the red, as it printed | where |
|---|---|---|
| 1. the handle's shape | `'owning' is not a member of 'peacock::TableResult'`, three sites | compile |
| 2. a slice's string bytes | `Expected: (first) < (all), actual: 488160 vs 488160`; `first + second` 976320 vs `all` 488160 | `VarlenBytes.ASliceCountsOnlyItsOwnRows` |
| 3. one handle (#197) | `two handles were concatenated` | `Scatter.TwoInputHandlesAreRefusedNotConcatenated` |
| 4. the partitions share | `use_count()` 1, expected 4; and the peak bound | `Scatter.PartitionsShareTheirTableAndSurviveTheirSiblings`, `…HoldsTheInputOnlyUntilTheTableIsPartitioned` |

Task 3's `Scatter.EmptyLanesAreZeroRowSlices` and Task 4's `Slice.OwnsItsRowsSoTheBatchCanGo`
were green from the start and stayed green, which is what the plan asked of them.

No producer was refused by `owning()`: review focus 1 (a zero-column table) and 2 (a name count
that does not match) found nothing to fix, with the whole gpu tier green behind the two checks.

### Measurement 1 — peak device memory during the scatter call

From the RMM statistics adaptor `main()` installs above the pool, scoped with
`push_counters`/`pop_counters` around the one call (`allocated_by` in the test file). The input is
allocated in a scope of its own, so it contributes 0 to the call's counters and freeing it reads
as negative. `partition alone` is the same `spark_hash_partition` call in a scope of its own —
the partitioned table plus the scatter's own temporaries, measured rather than estimated.

All figures bytes, 122,880 rows, four lanes, `peacock_plan_tests`.

| shape | input | partition alone | call peak before | call peak after | call total before | call total after |
|---|--:|--:|--:|--:|--:|--:|
| fixed-width (Int64, Int32, Decimal128) | 3,486,720 | 4,962,592 | **6,973,648** | **4,962,592** | 8,942,384 | 5,455,456 |
| string (Int64, Utf8) | 3,717,136 | 9,621,376 | **9,621,376** | **9,621,376** | 14,341,728 | 10,624,352 |

Read it this way.

- **The copies cost exactly one input of allocation, and they are gone.** Both shapes drop their
  requested total by 1.000x the input — 3,486,928 and 3,717,376 bytes. That is the shape of the
  win, and it is independent of the table.
- **The peak drops on the fixed-width shape** from 2.00x the input to 1.42x: 6,973,648 to
  4,962,592, and 4,962,592 is `partition alone` to the byte. The arm's peak is now the peak of
  partitioning, with nothing on top.
- **The peak does not move on the string shape, and that is cuDF, not us.** `cudf::partition`'s
  gather for a strings column peaks at 9,621,376 — 2.59x the input — which is already above
  `parted` plus the copies (2.00x). So the copies never set the peak there, and removing them
  cannot lower it. The spec's bound ("the partitioned table plus hashes, the partition map, a
  gather map, string offsets") under-counts cuDF's own string gather by about threefold; a hand
  formula written from it would have failed a correct arm. `Scatter.AStringColumnsGatherCostsMoreThanTheCopiesDid`
  pins the fact so the next reader does not rediscover it.
- The drop the plan predicted, `peak_before − peak_after >= 0.8 x input`, does not hold and
  cannot: `partition alone` is itself 1.42x the input on the fixed shape, so the most the peak can
  fall is 0.58x. It fell 0.577x. Nothing survived.

### Measurement 2 — the per-partition timers of p1..N−1

`NodeTiming::Events`, four lanes, `collect_node_regions()`, three runs each side. `device_us`.

| shape | p0 (the shared scatter) | p1 | p2 | p3 |
|---|--:|--:|--:|--:|
| string, before | 13274 / 13125 / 13615 | 390 / 417 / 405 | 142 / 120 / 121 | 120 / 118 / 119 |
| string, after | 13106 / 12492 / 12982 | 56 / 55 / 54 | 56 / 68 / 52 | 55 / 56 / 52 |
| fixed, before | 402 / 377 / 386 | 107 / 138 / 122 | 106 / 106 / 110 | 103 / 104 / 117 |
| fixed, after | 374 / 360 / 372 | 81 / 80 / 81 | 81 / 80 / 89 | 81 / 81 / 83 |

p0 is unchanged either side, which is right: it carries the scatter, and the scatter did not
change. p1..N−1 collapse to a floor that no longer depends on the data — 52-56 µs over a string
column against 118-417 before. The floor is the CUDA event pair itself: between a p>0 region's two
events there is now only `whole.slice(...)`, which is host-side bookkeeping. "Toward zero" is as
far as it goes; an event pair is not free.

### The cost of sharing, stated

One undrained lane keeps **all** of the partitioned table alive, not just its own rows, so a
skewed hash is worse than N copies were: the peak falls but the tail lengthens. And the driver
prices each lane's batch alone (`driver/accounting.rs`), so a released lane's bytes leave the
model while the device still holds them behind a sibling — the model under-reports, filed as
**#265** in `llm-wiki/tickets/memory.md`.

The spec asks for **two** sentences, and the second is the per-partition timers: p1..N−1 collapse
to the cost of a CUDA event pair, because between a `p > 0` region's two events there is now only
a `slice`, which is host-side bookkeeping. p0 still carries the scatter. All three are in
`architecture.md`'s "What sharing costs" — the timer one was added by the completeness pass, which
found this line claiming two sentences were there and naming the wrong pair.

### Deferred by the human override, not by a finding

Benchmark timings before and after, `--run-benchmarks` and the `bench_` cases, Nsight captures,
any H200 timing, and the sf40 binaries (`peacock_tpch_tests`, `peacock_tpchv_tests`). shad-gpu is
down, so `--run` and `--pull-results` were never available. The two measurements above are
gtest-scale and were taken.

### Still-copying sites, and why each says so

Three, each with the reason at the site. The mid-plan limit's `slice_handle` and the sorted
merge's fetch copy on purpose — a view there would pin the whole batch, which is the memory those
copies exist to give back — and the test's `adopt` case needs a handle no node produced, where a
share would be one table under two handles. The operator exit paths still deep-copy columns into
a fresh table; that is #154 and untouched.

### Traps worth keeping

- `strings_column_view::chars_size` reports the **unsliced parent's** bytes
  (`strings_column_view.hpp`: "does not reflect a sliced parent column view"). `offsets()` returns
  the whole child and `offset()` indexes into it, so a slice's own bytes are
  `at(offset + size) - at(offset)`, read by two 4-byte `cudaMemcpyAsync` and one sync — not
  `cudf::get_element`, which allocates a device scalar and launches a kernel and cost 4.3x. An
  unsliced view short-circuits to `chars_size`, so only a scatter partition pays the second read.
  Either way a node with STRING outputs synchronizes; `plan_executor.h`'s comment says so.
- `cudf::column`'s constructor from a `column_view` "accounts for the `column_view`'s offset", so
  the old per-partition copies were compacted — N copies summed to one input, not N.
- `cpp/build` in this worktree is an empty root-owned directory and cmake dies at configure
  blaming itself; `/tmp/dkb-cppbuild` with `-DCMAKE_INSTALL_PREFIX=/tmp/dkb-cppinstall` is the
  local C++ build and survived another round.

## Round 2 (2026-10-09) — review round 1 on PR #173: 0 blocking, 2 important, 4 nits

### Important 1 — what `varlen_content_bytes` costs per string column

The reviewer was right and the regression was real: round 1's `cudf::get_element` allocates a
device scalar through RMM and launches `device_single_thread` per read, and it ran twice per
string column on a path the driver takes for **every node output of every batch**
(`gpu_backend/mod.rs` always passes a stats buffer). Measured on the card, 2,000 reps, two runs,
`ns` per string column of customer row group 1 (27,120 rows):

| path | ns/column | against the old |
|---|--:|--:|
| `chars_size` — the pre-task code, unsliced only | 7,201 / 7,375 | 1.00x |
| round 1: `cudf::get_element` x2 | 30,851 / 31,665 | **4.3x** |
| **now**, unsliced short-circuit — every column but a scatter partition | 7,682 / 7,736 | **1.05x** |
| **now**, sliced: two async copies, one sync — scatter partitions only | 11,212 / 11,186 | **1.55x** |

Both fixes compose and both are in. The short-circuit is
`sv.offset() == 0 && sv.size() + 1 == offsets.size()`, which is every column that is not a
scatter partition, so the common case is back to parity — the 5% is the branch plus the fact that
the two "now" rows are timed through `varlen_content_bytes`'s own column loop while the other two
call the strings path directly, which makes the parity figure conservative. The sliced path is two
4-byte `cudaMemcpyAsync` and one `cudaStreamSynchronize`: one more copy than `chars_size`, one
sync either way, no device allocation and no kernel — 2.8x cheaper than round 1.

Correctness is unchanged and the two branches check each other: in
`VarlenBytes.ASliceCountsOnlyItsOwnRows`, `all` reads the unsliced column (short-circuit) while
`first` and `second` read slices (two-copy path), and `first + second == all` only holds if the
two agree. The reviewer verified the formula against cuDF's `offsets_begin()` for slice-of-a-slice,
single row, non-zero parent offset and the empty early return; none of those moved. Every memory
figure in Measurement 1 above came out byte-identical after the change.

### Important 2 — the copy list is four sites, not two

`operators/limit.cpp` and `operators/sort.cpp` both slice into an owning table for the same
reason `slice_handle` does, and carried no comment. One line at each now says why, in
`slice_handle`'s wording. The four are: the session's `slice_handle`, the sorted merge's fetch,
the limit operator's slice and the sort operator's fetch. `architecture.md` names all four — that
edit is the coordinator's, left alone here.

### Nit 6 — the finding kept without a cuDF constant

`EXPECT_GT(probe.peak, scan.net * 2)` was a fact about cuDF's strings gather and would have gone
red on a *cheaper* gather, which `verify-26.02` exists to run. Replaced by one helper,
`expect_the_partitions_add_nothing`, asserting on both shapes that the arm's peak **and** its
requested total are the peak and total of partitioning alone — measured on that very input, so no
cuDF cost is baked into a constant. Both bounds discriminate: before the fix the total was red on
both shapes (8,942,384 vs 5,454,880 fixed; 14,341,728 vs 10,623,968 string) and the peak was red
on the fixed shape. The string case is `Scatter.AStringScattersPartitionsAddNothingEither`, and
why its peak does not move is printed by `report_scatter` and recorded above rather than asserted.

### Nits 3 and 4

The repartition arm's in-body comment is back to 4 lines, and
`Scatter.HoldsTheInputOnlyUntilTheTableIsPartitioned`'s to 3; the "measured, not estimated"
rationale moved to the helper's doc comment, where the cap is 10.

### Re-proved, exact tree

`node_session.cpp` md5 `e87b2d43…` and `test_plan_executor.cpp` md5 `b61c3b4d…` identical local
and remote. Build 0 warnings. `peacock_gpu_tests` 2/2, `peacock_plan_tests` **68/68**
(`--gtest_list_tests` 68, so build-test.md's row is unchanged), `test_gpu_corpus` 79/0 including
the six string-keyed tp4 cells run again on their own (q1 x3, shuffle-additive-avg x3),
`test_node_timing` 1/0, `peacockdb_core_gpu_lib gpu_tests::` 580/0,
`peacock_gpu_benchmarks --skip bench_` 8/0. No golden moved: the host's goldens hash
`7a8b50b8…` after the corpus run, identical to the tree's.

## Round 3 (2026-10-09) — review round 2: 0 blocking, 0 important, 3 nits

**Nit 1, the dropped copy returns** (`node_session.cpp:237-247`). Both `cudaMemcpyAsync` returns
now fold into the one check that already guarded the sync. The failure mode the reviewer named is
real and silent: a synchronous `cudaErrorInvalidValue` never enqueues and is not sticky, so the
following `cudaStreamSynchronize` returns success and `edges` stays `{0, 0}` — zero content bytes
reported as an answer. One `cudaError_t` threaded through three calls, one throw site, and the
comment says why rather than what.

**Nit 2, the stale sentence** (`plan_executor.h:68`): "reads two offsets back" became "reads an
offset back". The common unsliced path reads one since the short-circuit landed; two is the
scatter-partition path only, and the claim the sentence exists to make — a STRING output
synchronizes regardless — is unchanged. `build-test.md` and the "Traps worth keeping" section
above are the coordinator's corrections and were left alone.

**Nit 3, the three untested refusals, each watched fail.** `select`'s out-of-range ordinal,
`select`'s empty ordinals and `with`'s row-count mismatch now have a case, appended inside the
existing `TableResult.*` bodies beside the call each guards, so the count stays at 68
(`--gtest_list_tests` confirms) and `build-test.md` does not move. Red-green run rather than
assumed, since an untested guard is what the nit was about:

| guard | how it was watched fail |
|---|---|
| `with`: a column of 1 row beside 4 | guard deleted → `SlicesAndSelectionsShareTheirColumnsOwners` red, "Expected: `t.with(int64_column({9}), "d")` throws" |
| `select`: no ordinals | guard deleted → `ATableOfNoColumnsIsRefused` red, "Expected: `t.select({})` throws" |
| `select`: ordinal out of range | proved by its own message — `TableResult::select: ordinal 7 of 1 columns`. Deleting this one is an out-of-bounds `std::vector` read rather than a clean red, so the guard's own text is the proof its line ran; no other code on that path throws `std::runtime_error`. |

Both deletions restored and re-proved green. `exit-copies` and `join-session-cpp` are written
against these three constructors, which is why the guards are worth a case at all.

**Re-proved, exact tree.** md5 local = remote: `table_result.cpp` `e19ffa7f…`,
`node_session.cpp` `276984c1…`, `plan_executor.h` `ccd7f5cf…`,
`test_plan_executor.cpp` `90333542…`. Build 0 warnings. `peacock_plan_tests` **68/68**
(`--gtest_list_tests` 68), `TableResult.*:VarlenBytes.*` 4/4, `peacock_gpu_tests` 2/2, and the six
string-keyed tp4 cells 3+3 passed. The scatter's memory figures printed byte-identical to rounds 1
and 2 (fixed 4,962,592 / 5,455,456; string 9,621,376 / 10,624,352), so neither nit moved a number.
No golden moved.

## Round 4 (2026-10-09) — completeness pass: the name count is checked at the registry

Both readings found the same gap from opposite ends, and it was real. The branch claimed #164's
name-count half closed on the strength of `TableResult::owning`, but `owners`, `columns` and
`column_names` are public fields of a plain aggregate, and `exit-copies-impl.md` — already
committed — assembles handles through them with no constructor. So the claim held for the tree as
committed and would have stopped holding at the next task, with the out-of-bounds
`column_names[i]` reads #164 names still there (`filter.cpp:42`, `join.cpp:260/338/343`,
`aggregate.cpp:169`, `window.cpp:47`).

**The fix is a boundary, not a second constructor check.** `NodeSession::Impl::register_handle`
(`node_session.cpp:364`) now owns handle allocation: it refuses a handle of no columns and one
whose names or owners do not number its columns, then takes `next_handle++`, notes the producer
and emplaces. `next_handle++` appears in exactly one place now. Seven call sites went through it —
the scan-map arm, the collapse arm, the scatter loop, the per-partition map arm,
`execute_scan_rowgroups`, `slice_handle` and `adopt` — which is every way a handle can reach the
registry, so no consumer can read a handle that has not been checked. That is what a constructor
check could not give: it covers whatever task 6 assembles by hand.

Two things beyond the letter of the ask, both reported: the check also holds
`owners.size() == columns.size()`, since a view with no owner is the same class of hand-assembly
defect and the worse one (a dangling view rather than a bad name lookup); and the refusal is
reachable publicly through `adopt`, which is the harness's upload and exactly the "assembled by
hand" shape, so the case needs no test-only hook.

**Watched fail, both guards disabled together.**

| guard | the red |
|---|---|
| no columns | `ATableOfNoColumnsIsRefused`: "Expected: `session.adopt(peacock::TableResult{})` throws an exception of type std::runtime_error" |
| the counts disagree | `NamesMustMatchColumns`: "Expected: `session.adopt(std::move(handed))` throws…", `handed` being a one-column handle with its names cleared |

Restored and re-proved green. Both cases live inside the two existing refusal bodies, where each
is that case's own subject, so the count stays at **68** and `build-test.md` does not move.

**Re-proved.** md5 local = remote (`node_session.cpp` `f882ad29…`, `test_plan_executor.cpp`
`b984aea5…`), build 0 warnings, `peacock_gpu_tests` 2/2, `peacock_plan_tests` **68/68**
(`--gtest_list_tests` 68), `test_gpu_corpus` **79/0 whole** rather than the six cells — a check on
the registration path sits under all 79 — `test_node_timing` 1/0,
`peacockdb_core_gpu_lib gpu_tests::` 580/0, `peacock_gpu_benchmarks --skip bench_` 8/0. No golden
moved (`7a8b50b8…` both sides). Handle numbering is unchanged on the success path: the refusals
throw before `next_handle++`.

## CI, for the `done` transition (2026-10-09)

Run 37918173680 on `ef9d3476`: cudf 25.02 pass (20m9s), cudf 26.02 pass (24m28s) — so the handle
compiles and the whole Rust side passes against both cuDF legs, which no local run covers — GPU
build pass, Cost report pass, S3 check pass, Pages skipped. The only red is `GPU Tests (remote)`
on `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`, which is shad-gpu
and which the host override exempts by name; its work was done on nebius-gpu and is recorded above.
`completeness approved` → `done`.
