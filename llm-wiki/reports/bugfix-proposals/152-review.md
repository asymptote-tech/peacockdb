# Review of `152-proposal.md` — GpuHashJoin: the build handle does not survive a streamed probe

Read at master 188c23ce (code identical to the proposal's c18e063a; only `llm-wiki/tasks/` and
the archive moved). Paths relative to `/media/data/peacockdb`. Nothing built or run.

## 1. Verdict

**Needs changes.** The mechanism — one additive symbol `peacock_handle_retain`, a deep copy today,
the two refusal arms deleted — is correct and is the smallest fix the surface allows; what is wrong
is the analysis around it: the minimum query's mode predictions are off (tp4-single refuses too),
and the "cells re-enabled" inference misses the divergence that will actually decide most of the 79
rows — DataFusion chunks a join's output at 8192 rows and the device emits one table per call, and
the device tier compares per-batch lists exactly. That has no ticket and the developer hits it on
the first re-attribution run.

## 2. Findings

### F1 — §5 half one: tp4-single refuses as well; only tp1-single hands the build over — **important**

The proposal says "tp1-single and tp4-single pass (one probe batch)". At tp4-single the probe source
is not one batch per join lane:

- `planner/translator/nodes.rs:383-389` — under `Batching::Off` (tp4-single) `lanes_for` returns
  `target_partitions`; the small-table rule does not apply, so `customer` is 4 lanes, not 1.
- `planner/translator/scan_mapping/partition.rs:45-47` — 2 row groups over 4 lanes gives two lanes
  of one batch and two empty ones (the golden shows the shape for `part`:
  `testdata/goldens/tpch.sf1/tp4-single.plans.txt`, `== q19`, `partition_groups=[[[0]],[[1]],[],[]]`).
- `nodes.rs:267-273` — sides that are not co-partitioned are `merged` (`GpuMergePartitions`,
  which forwards every batch of every lane into one, `plan/mod.rs:827-830`); a DataFusion
  `Partitioned` join instead gets `GpuEmitPartitions` on both sides, and a scatter delivers one
  batch to *every* lane per input batch. Either way each join lane receives one probe batch per
  **source** batch — two for `customer`, four for `lineitem` (`tp4-single.plans.txt` `== hash-join`:
  Load 4 lanes → Merge → Emit 1→4 → join at 4 lanes, `per probe batch: … build copy, batch`).
- The registry agrees: `tpch/q19` is `enabled` at `gpu_tp1_single` only and `disabled` on `152` at
  `gpu_tp4_single` (`testdata/cost-registry.csv:119`).

So the device refuses half one at tp1-rowgroup, tp4-single and tp4-rowgroup; tp4-sized depends on
whether the estimator packs both row groups into one batch. The same error sits in §7 ("if it keeps
four lanes … only the tp1-rowgroup cell exposes the defect") and softly in §2's "four modes out of
five" explanation, which attributes the multi-batch probe to row-group/sized batching alone: the
rule is *any mode where the probe source yields more than one batch in total*, because a merge or a
scatter hands every source batch to every join lane.

**Correction**: rewrite §5's device prediction to "tp1-single passes; tp1-rowgroup, tp4-single and
tp4-rowgroup refuse at the second probe call; tp4-sized iff `partition_groups` shows two batches";
drop the §7 bullet; state the rule in §2.

### F2 — §6/§7: the next cause on most join rows is a divergence nobody has ticketed — **important**

The device tier asserts the run's rendered text against the cpu-authored section byte for byte
(`tests/common/corpus_gpu.rs:101-105`, `corpus_golden.rs:111-119`), and that text carries the
per-batch lists (`plan_text/run_text.rs:81-87`). The CPU backend's join is DataFusion's
`HashJoinExec` (`cpu_backend/join.rs:245-249`, `single_node.rs:51-56` collects every output batch),
which emits its output in `batch_size` = 8192-row chunks. The device emits one table per call.

Evidence in the committed cpu goldens, `testdata/goldens/tpch.sf1/tp1-single-mini.cpu.txt`:

- `== hash-join`: `GpuHashJoin … in_rows=[[1500000],[6001215]] batch_rows=[[8192,8192,…]]`
  (733 entries), and `GpuAggregate` above it `batch_rows=[[1,1,1,…]]` ×733 — one probe batch in,
  733 output batches out. `== join-int` and `== left-join` are the same shape; `== q13`'s Left join
  is `[[8192,…]]`; `tpcds.sf1/tp1-single-mini.cpu.txt` `== q97`'s two Inner joins are `[[8192,…,843]]`.
- `== q11` (tpcds): four joins show 336 and 88 `batch_rows` entries against `in_rows=[[365],[2751012]]`
  — 2751012 / 8192 = 336. Those are DataFusion's **output chunks of a one-batch probe**, not probe
  batches; the join above each of them sees 336 probe batches on the CPU and one on the device. That
  is why T19 saw q11 reach the unload at tp1-single where "the rule says #152 must fire"
  (`tests/common/corpus_cases.inc:221-227`, "the batch-count rule LOST … see #152") — the symptom
  was recorded and not diagnosed.
- No ticket names it: `grep -n "8192\|batch_size\|chunk" llm-wiki/tickets.md llm-wiki/tasks/active-tickets.md`
  is empty. The six device cells enabled today never meet it: q6 has no join, and q19's per-call
  outputs are tiny (`tp1-rowgroup-mini.cpu.txt` `== q19` join `batch_rows` max 7).

Consequences for the proposal:

- §6's "possibly `q13` and `q97`" is wrong — both fail on batch lists at tp1-single the moment #152
  clears, whatever #185 does. `left_join`, `hash_join`, `join_int` and every fan-out join at
  tp1-single land here too, ahead of or beside #183/#187. Only `tpch/q19` ×4 stays a real candidate
  (small per-call outputs at every mode).
- §7's cost bullet ("`tpcds/q11` takes 336 and 88 probe batches against fact-sized builds") is the
  wrong direction: on the device those joins take **one** probe batch at tp1-single. The real B is
  the probe source's batch count (49 for lineitem at tp1-rowgroup, ×4 lanes at tp4-rowgroup).
- The "slow part" of §8 — the 79-row re-attribution — has a wall the proposal does not name, and the
  decision behind it is not #152's: relax the comparator (compare Σ rows per node rather than the
  per-batch list — but the list is what §3.7 calls the agreement check), or have the CPU backend
  `concat_batches` a join call's output into one batch (matches the device, moves every join
  `.cpu.txt` section and the CPU's own memory behaviour), or a device-side chunking nobody wants.

**Correction**: file the ticket before the registry pass, name it in §6 as the recorded next cause
for every join whose per-call output exceeds 8192 rows, and shrink §6's "likely green" to q19 ×4.
Replace §7's q11 numbers with the source-batch rule.

### F3 — §3.5: the extra `n_bytes` for `BatchCopy` over-charges and departs from the CPU model — **minor**

The batch copy is consumed by the key-project call and destroyed when that `execute_one` returns
(`cpp/src/peacock/operators.h:56-58`: inputs owned for the call), before the join call retains the
build. The two transients never overlap, so the pre-call bound is `max(2·n, build + n)`, which the
existing `build_bytes + n_bytes` already covers whenever the build is at least a batch. The CPU twin
keeps `build_bytes + n_bytes` (`cpu_backend/backend.rs:258-262`) and the device reports no measured
scratch to compare against (every GPU call returns `CallStats::default()`; `executor/mod.rs:145`),
so nothing would ever show the extra term right or wrong. `probe_reads_build`'s own doc
(`gpu_backend/join.rs:154-156`) is the argument against it: "a transient charged for a read that
never happens refuses work that fits".

**Correction**: leave `scratch_bytes` as it is; say in the doc why one build copy plus one batch is
the whole footprint.

### F4 — doc and coordination edits the proposal does not list — **minor**

- `cpp/src/plan_executor.h:119-120` and `cpp/include/peacock_gpu.h:175-176` say `slice_handle`
  consumes "as every operation on a resident table does / is" — false once `retain` exists; both
  sentences need the exception (`result_from_handle` already was one, loosely).
- `#152` in comments that outlive the fix: `wire/join.rs:44`, `wire/mod.rs:130`,
  `wire/tests.rs:166`; `scripts/exec_model/operators/recipe.py:27, 360, 398`, `recipe_join.py:17,
  283`. A sweep at close, since the ticket archives.
- `llm-wiki/build-test.md` rows 21-22 also carry test **counts** (test_gpu_executors 31,
  test_gpu_abi 4) and the gtest row its own; the proposal names only the prose.
- `refcounted-tables.md` is committed and on the board (`tasks.md`, chain ENS-refcounted-tables,
  state `new`, "closes #145, #152"). A committed spec is frozen; carving §2-§4 out of it and
  renaming the chain is the human's edit, and the proposal should say so rather than list it among
  the developer's wiki changes.
- Two approved specs plan `bug_` tests around #152 that this fix would turn green:
  `operator-cases.md:37` / `operator-cases-impl.md:998-1121` (Left/Full one-batch cases, "unknown
  input handle" for a second probe) and `walk-drives-every-plan.md:81-94`. Whichever lands second
  deletes them — worth one line in §3.8.

### F5 — citation drift — **minor**

`cpp/src/node_session.cpp` registry is at `:176`, not `:179`; `architecture.md` "What a streamed
probe costs" is `:448-457`, not `:496-502`, and the `GpuHashJoin` row is `:771`, not `:762`;
`test_gpu_abi.rs:1-7` does not say "three per-call symbols" (that phrase is `build-test.md:22`).
All resolve to the right text; noted so the consolidator does not re-open them.

### F6 — a new gtest suite goes into a file already past the cap — **minor**

`cpp/tests/gpu/test_plan_executor.cpp` is 1372 lines against `coding-style.md`'s 1000. Not
enforced by any test; a `HandleRetain` suite there is the surrounding idiom, so it is a note, not a
correction.

## 3. Claims verified

- Root cause, both halves: `registry` is `unordered_map<uint64_t, TableResult>` with
  `unique_ptr<cudf::table>` (`node_session.cpp:176`, `plan_executor.h:15-18`); every read arm moves
  and erases (`:267-272`, `:364-369`, `:453-457`; `slice_handle` `:529-530`); `take_input` moves by
  value (`dispatch.cpp:102-110`), `execute_one` owns the vector (`operators.h:56-58`);
  `execute_hash_join` reads views only (`join.cpp:42-46`). Sixteen symbols in the header, none
  yielding a handle without consuming one; `result_from_handle` does not consume but yields IPC.
- Recipe table: `wire/join.rs:45,57` (`BuildSideCopy` iff `probe_streams`), `:75-79`
  (`BatchCopy` when a per-call join exists), `:89`, `:110` (finish takes `BuildSide`), `:349`,
  `attach.rs:333`; capability `plan/join.rs:416` (Inner streams), `:426` (Left/Full stream with a
  finish).
- Refusal sites and their shape: `gpu_backend/join.rs:251, 256-259, 293-299, 304-314`; `probes` is
  used only by the message (`:124, 139, 165, 311`). `GpuBatch::consume` skips the release
  (`gpu_batch.rs:13-16`); `GpuBatch` carries no accountant hold (`executor/mod.rs:82-87`), and the
  accountant counts driver-held batches only (`driver/accounting.rs:117-137`).
- The drop-release path exists: the driver calls `finish_and_fetch` for every join
  (`driver/single_partition.rs:353-365`), and a join with no finish returns at `join.rs:186-188`,
  dropping `build` through `Drop` (`gpu_batch.rs:28-32`).
- CPU twin: `cpu_backend/join.rs:238-249` clones at `:239` and `:247`; `build_bytes` at `:216-218`.
- Accounting stays golden-free: `render_run` renders rows, bytes and per-batch lists, no
  resident/peak figure (`run_text.rs:23-101`); no GPU test pins `resident_bytes`/`scratch_bytes`.
- Registry: 79 rows carry `152`; 78 have all five gpu cells `disabled`; `tpch/q19` is the one
  exception (`gpu_tp1_single` enabled). Seven rows name no other open ticket (`q41 q71 q75 q97`,
  `tpch/q13 q19 left_join`; #97/#115/#116 archived). `build copy` appears 69 times in
  `tpch.sf1/tp1-rowgroup.plans.txt`.
- Tests that must move are the ones named: `test_gpu_executors/join.rs:53-88` and `:249-312`
  (`message.contains("#152")` at `:85` and `:309`); the walk's `resolve` `:212-224`, the
  one-probe-batch assert `:440-447`, `driven` `:789-791`, `PROVEN` `:798-813`; comments at
  `test_cpu_end_to_end.rs:333`, `test_cpu_corpus.rs:88`; `wire/tests.rs:147-207` assert inputs
  only and stay.
- The Left test's expected padded row is producible on the device today: the pad project writes a
  bare typed-NULL literal (`wire/join.rs:282`), which takes `build_scalar`'s null path
  (`expr.cpp:453-457`, `:831-833`) — #198 hits only `col <op> NULL`.
- `peacock_handle_release` is documented idempotent (`peacock_gpu.h:197`), so a new id rather than a
  count is right. The FFI crate has no symbol list beyond `raw` (`peacockdb-ffi/src/lib.rs`), and
  CMake exports nothing by list.
- "No golden regenerates": the names stay, `recipe-payloads.txt` is fb-only, `.cpu.txt` is
  cpu-authored, and the plan text keeps `build copy`.

## 4. Corrected proposal — the sections that change

### 2. Root cause (one sentence added)

Why the corpus sees "four modes out of five": a join lane receives one probe batch per batch the
**probe source** produced, whatever the mode, because `GpuMergePartitions` forwards every lane's
batches into one and `GpuEmitPartitions` scatters every input batch to every lane. tp1-single is the
one mode where a scan is one batch, so it is the one mode where the join gets one probe call —
unless the probe is a union of scans (q71, q2) or the type copies its probe batch (Left, Full).

### 3.5 Accounting

`resident_bytes` and `scratch_bytes` unchanged. The build side really is resident for the whole
stream now, as the CPU already reports; the deep copy is a transient inside the call and
`build_bytes + n_bytes` is its whole footprint — the batch copy for a Left/Full key project is
destroyed before the join call retains the build, so the two never coexist.

### 5. Minimum corpus query

Same two queries. Half one on the device: **tp1-single passes**; **tp1-rowgroup, tp4-single and
tp4-rowgroup refuse** at the second probe call in `gpu_backend/join.rs:304-314` (the two `customer`
row groups reach the join lane as two batches through a merge or a scatter); tp4-sized refuses iff
`partition_groups` shows two batches. Half two unchanged (refused at every mode at the first call).
Both have per-call outputs under 8192 rows, so both also match the cpu golden's per-batch lists once
they run — which is what makes them fit for a corpus cell, unlike the joins in F2.

### 6. Cells re-enabled

All 79 rows leave `152`. Before the runs, file one ticket: *a join's per-call output is one table on
the device and 8192-row chunks on the CPU, and the device tier compares the lists* — it is the
recorded next cause for every join whose per-call output exceeds `batch_size`, at every mode where
that happens (tp1-single for the fact-table probes: `hash_join`, `join_int`, `left_join`, `q13`,
`q97`, `q11`, …). Likely green from this fix alone: `tpch/q19` ×4 (per-call outputs of at most a few
rows at every mode; risk #185 on the merge). `q13` and `q97` are not candidates. The rest of the
attribution as the proposal had it, with the new ticket beside #183/#185/#187.

### 7. Risks (two bullets replaced)

- The copy's cost is B × lane build bytes where B is the **probe source's** batch count — 49 for
  lineitem at tp1-rowgroup, and at tp4-rowgroup each of 4 lanes sees all 49 against a quarter-build.
  `q11`'s "336 and 88" are CPU output chunks, not device probe batches (F2). Every Inner join now
  copies its build once even at one probe batch (`wire/join.rs:45`: `handed_over` is false whenever
  the type streams), including the one join cell enabled today.
- (drop the "whether `customer` really drops to one lane" bullet — F1.)

### 3.9 additions

`plan_executor.h:119-120`, `peacock_gpu.h:175-176` lose "as every operation on a resident table
does"; the `#152` comment sweep in F4; `build-test.md` counts; `refcounted-tables.md` is the
human's edit, not the developer's; the `bug_` tests in `operator-cases` / `walk-drives-every-plan`
go with whichever lands second.

## 5. Complexity

**M**, as proposed, for the code and its tests. The registry half is bounded below what §6 hoped:
with the chunking divergence unticketed, the runs mostly re-attribute rather than enable, and the
decision that unblocks them (comparator vs CPU-side concat) is a separate task, not part of this M.
