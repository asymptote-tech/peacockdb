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
**#265** in `llm-wiki/tickets/memory.md`. Both sentences are in `architecture.md`.

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
  `at(offset + size) - at(offset)` through `cudf::get_element`. Two reads back instead of one, so
  a node with STRING outputs still synchronizes; `plan_executor.h`'s comment says so.
- `cudf::column`'s constructor from a `column_view` "accounts for the `column_view`'s offset", so
  the old per-partition copies were compacted — N copies summed to one input, not N.
- `cpp/build` in this worktree is an empty root-owned directory and cmake dies at configure
  blaming itself; `/tmp/dkb-cppbuild` with `-DCMAKE_INSTALL_PREFIX=/tmp/dkb-cppinstall` is the
  local C++ build and survived another round.
