# Digest C — #152, #185, #175, #173, #190, #47, #60

Read at master 188c23ce, read-only; nothing built or run. Every contested file:line was opened;
where a review contradicted its proposal I say which side the code supports. Paths relative to
`/media/data/peacockdb`; crate cites are the cargo-registry copies of DataFusion 45.0.0 / arrow 54.x;
cuDF cites are `third_party/cudf` (25.10a) unless a conda env is named. One measurement is mine: the
#60 five-row split was re-emulated in Python over the committed result golden.

Context for #152, #173, #175: commit 188c23ce archived `empty-answers.md` (PR #138 closed
2026-09-10, `archive/archived-tasks.md:23`: "Superseded by `empty-build.md`, which showed the
capability it proposed is one the engine does not need") — it had bundled #173 with #175 behind
`output_schema` on the wire plus a C++ `make_empty_column` arm, a new `ProjectRole` pad and a
routing change. `tasks/empty-build.md` (task 12, `approved to build`, impl plan written) now owns
#175 alone: keep the scatter's typed zero-row batch where the join above owes rows, one derived
index field, one conditional drop, nothing else. `tasks/refcounted-tables.md` (chain
ENS-refcounted-tables, state `new`) owns #152 with #145: `shared_ptr` `TableResult` (39 sites, 11
files), a new ABI symbol `peacock_handle_retain`, the `Input` rename, accounting divergence deferred.

## #152 — GpuHashJoin: the build handle does not survive a streamed probe

- **Status after research:** live — the largest gpu wall (79 registry rows carry `152`, 78 with
  all five gpu cells off; `tpch/q19` `gpu_tp1_single` is the one join cell enabled).
- **Issue:** the device refuses the second probe batch of every streaming join
  (`executor/gpu_backend/join.rs:304-314` `build_copy`: "probe batch N has no build side left …
  (#152)") and the first probe batch of a Left or Full join whatever the count (`:293-299`
  `copy_of`, unconditional; the key project names `Input::BatchCopy`, `wire/join.rs:75-79`). Off:
  every gpu cell of the 79 rows except q19 tp1-single; also `tpch/q13`, `left_join`, tpcds `q2`,
  `q71`, `q97` at tp1-single (Left/Full, or a probe that is a union of scans).
- **Root cause:** `NodeSession::Impl::registry` is `unordered_map<u64, TableResult>` holding a
  `unique_ptr<cudf::table>` (`cpp/src/node_session.cpp:176`, `plan_executor.h:15-18`); every read
  arm moves and erases (`:272`, `:369`, `:456-457`; `slice_handle` `:530`); `take_input` and
  `execute_one` take inputs by value (`operators/dispatch.cpp:102-110`, `peacock/operators.h:56-58`).
  None of the 16 ABI symbols (`include/peacock_gpu.h:129-201`) yields a handle without consuming
  one. The recipe names `BuildSideCopy` per probe batch whenever `probe_streams` (`wire/join.rs:45-57`)
  and `BatchCopy` for the Left/Full key project (`:75-79`); the CPU makes both copies as `Arc`
  clones (`cpu_backend/join.rs:239`, `:247`).
- **Fix (as corrected by the review):** one additive C ABI symbol
  `int peacock_handle_retain(executor, handle, uint64_t* out)` — `peacock_gpu.h` after
  `slice_handle` (`:181`), body in `gpu_executor.cpp` shaped like `result_from_handle` (set
  `last_error`, return 1, no session reset), `NodeSession::retain` in `node_session.cpp` after
  `:542` doing `make_unique<cudf::table>(view)` (a deep copy until #145; a new id, because
  `peacock_handle_release` is documented idempotent `peacock_gpu.h:197`), one `extern` in
  `peacockdb-ffi/src/lib.rs`. `gpu_backend/join.rs`: delete `copy_of`, `build_copy`, the
  `probes` field; the `BatchCopy`/`BuildSideCopy` arms of `make` (`:251`, `:256-259`) call
  `retain` and leave the original in place; `Input::BuildSide` unchanged; a join with no finish
  releases the build through `GpuBatch::Drop` at end of stream. Accounting unchanged
  (`gpu_backend/backend.rs:287-293` stays `build_bytes + n_bytes` — review F3). Frozen surface: the
  C ABI gains its seventeenth symbol, additively; fbs, wire, declared-schema contract: none.
  Goldens: none — `Input` names stay so all ten `.plans.txt` are byte-identical (`build copy` ×69
  in `tpch.sf1/tp1-rowgroup.plans.txt`), `recipe-payloads.txt` is fb-only, `.cpu.txt` is
  cpu-authored. Tests: a `HandleRetain` gtest suite in `cpp/tests/gpu/test_plan_executor.cpp`
  (1372 lines, already over the 1000 cap — F6); `test_gpu_abi.rs` +1-2; `test_gpu_executors/join.rs:53-88`
  and `:249-312` flip from refusal pins to positives; `test_gpu_recipe_walk.rs` `resolve`
  (`:212-224`) gets distinct arms, a `MANY_BATCHES` knob, and the join arm loops over probe batches
  (red today with `NodeSession::execute_node: unknown input handle`). Docs that move:
  `plan_executor.h:119-120` and `peacock_gpu.h:175-176` ("as every operation on a resident table
  does" — false once retain exists), `wire/mod.rs:135-144`, `architecture.md:448-457`, `:771`,
  `:791-800`, `:838-846`, `:885-890`; `build-test.md` rows 21-22 (prose and counts); a `#152`
  comment sweep at close (`wire/join.rs:44`, `wire/tests.rs:166`, `exec_model/operators/recipe.py:27,360,398`,
  `recipe_join.py:17,283`); `test_cpu_end_to_end.rs:333`, `test_cpu_corpus.rs:84-88`.
- **Relation to the specs:** the proposal is a strict prefix of the approved `refcounted-tables.md`
  — its §2 (the symbol), §3 (retain per call, release the original once), §4 minus the rename, and
  the recipe-walk half of §6 — with `retain` as a deep copy and no `shared_ptr` refactor; #145 then
  becomes a C++ cost change behind the same symbol, and the `…Again` rename (ten plan goldens)
  goes with it. The spec is committed and frozen, so carving it and renaming the chain is the
  human's edit, not the developer's (review F4). Rejected specs: none touch #152; the spec's own
  "expect #183 at the unload, which no task owns since `casts.md` was dropped" is current.
- **Contested points:** (1) F1 — proposal: the half-one query passes at tp4-single; review: it
  refuses. **Code supports the review**: `planner/translator/nodes.rs:383-389` returns
  `target_partitions` under `Batching::Off` (no small-table demotion at tp4-single), `:267-273`
  merges non-co-partitioned sides, and a merge forwards every source batch to the one join lane,
  so `customer`'s two row groups are two probe batches; registry row 119 has q19 disabled on 152 at
  `gpu_tp4_single`. (2) F2 — the 8192-row chunking wall. **Code supports the review** (cross-ticket
  section), but "no ticket names it" is wrong: it is #185 under a wrong title. (3) F3 — extra
  `n_bytes` for `BatchCopy`: **code supports the review** (`operators.h:56-58` — the copy is
  destroyed when the key project's `execute_one` returns, before the join call retains the build;
  CPU model identical at `cpu_backend/backend.rs:258-262`). (4) The review's own claim in §3 that
  "the Left test's expected padded row is producible on the device today (the pad's bare typed-NULL
  literal takes `build_scalar`'s null path)" is **contradicted by the code**: `operators/project.cpp:45-49`
  tests `is_ast_able` before `build_column`, `expr.cpp:414` returns true for a non-string literal,
  and `build_expr`'s literal arms build every scalar valid (`:157-255`), so the rewritten positive
  Left test answers `(b,5,NULL,0)` on its Int64 probe column. Sequence after `typed-nulls.md` or
  land it as a `bug_` test naming #198 (see #173 F1).
- **Minimum corpus query:** half one — `SELECT n.n_nationkey, c.c_custkey FROM nation n JOIN
  customer c ON c.c_nationkey = n.n_nationkey WHERE c.c_acctbal > 9990` (tpch sf1; gpu). Device:
  tp1-single passes; tp1-rowgroup, tp4-single and tp4-rowgroup refuse at the second probe call in
  `join.rs:304-314`; tp4-sized iff `partition_groups` shows two batches. Half two — `SELECT
  c.c_custkey, o.o_orderkey FROM customer c LEFT JOIN orders o ON o.o_custkey = c.c_custkey WHERE
  c.c_custkey < 20`: refused at every mode at the first probe call (`:293-299`). Neither is in
  testdata; the corpus carriers are `tpch/q19` (Inner) and `tpch/left_join` (Left). Both emit under
  8192 rows per call, so both fit a device cell once they run.
- **Cells re-enabled / next wall:** all 79 rows lose `152`. Likely green from this fix alone:
  `tpch/q19` ×4 (per-call outputs of a few rows). With #185's concat landed first, also candidates
  `tpch/q13` ×5 and `tpcds/q97` ×5 (int-only sinks); `q41`, `q71`, `q75`, `left_join` land on
  #183/#187 by sink schema; the rest on #183 (49 rows), #187 (8), #185 (9), #191 (`tpch/q8`), and
  the cpu-side #175/#180/#189 rows. One shad-gpu run per row, in batches of about five.
- **Overlaps and dependencies:** #145 (the spec's other half; retain becomes O(1)); #136 (rehash
  per probe — untouched); #140; #155's two rows. #185 must land first or every join row
  re-attributes to it. #175 Change 2 must not turn a kept build-side empty into a probe call (its
  §5 hazard); #173 B3's probe-once rule is what keeps a guarded lane off #152. `operator-cases.md:37`
  / `-impl.md:998-1121` and `walk-drives-every-plan.md:81-94` plan `bug_` tests on #152 — whichever
  lands second deletes them. `operator-harness.md` adds `peacock_handle_from_arrow` — reconcile the
  ABI count in `architecture.md`. `tasks.md`: the layout chain may not run beside this one.
- **Complexity:** M — seven files, ~60 lines added / 40 deleted, one additive ABI symbol, no golden
  regenerated; the 79-row registry pass is the slow part and mostly re-attributes.
- **Unticketed defects found:** the review's pad-NULL claim (above; the defect is #198's, its
  reach is not); `test_plan_executor.cpp` over the file cap; nothing else.

## #185 — `GpuAggregateBatches` reports its own output as `in_rows`

- **Status after research:** misdiagnosed — real cause: the CPU join executor hands the driver
  one `CpuBatch` per `RecordBatch` DataFusion's stream yielded, the device answers one table per
  call; `in_rows` is the driver's arithmetic and is right on both. Live; the fix is one file.
- **Issue:** nine gpu tp1-single cells (tpcds q38 q48 q87 q88 q93 q96, tpch q3 q14 join_int;
  registry rows 39 49 88 89 94 97 103 114 128) fail `assert_section` on the first differing line,
  which in text order is the merge's `in_rows`. The same divergence sits behind every join cell that
  #152/#183/#187 will free (cross-ticket section).
- **Root cause:** `CpuProbingJoin::probe_and_fetch` (`executor/cpu_backend/join.rs:236-250`) and
  `finish_and_fetch` (`:254-265`) return `declared(...)`, one batch per chunk (`:269-273`);
  `execute_single_node` collects every batch of the stream (`cpu_backend/single_node.rs:51-56`).
  DataFusion 45's `HashJoinExec` bounds matched pairs per chunk at `batch_size` = 8192
  (`hash_join.rs:1472-1479`; `lib.rs:25-36` leaves the default) and appends unmatched rows for
  outer types (q93's Right chunks are ~82K — review 3); `CrossJoinExec` emits one batch per build
  row; a build-side semi finish emits empties per probe range plus the answer (tpch q4:
  `[0×18, 52523]`). The driver queues every element (`driver/partitioned.rs:307-313`) and records
  `in_rows` as Σ over popped batches (`:596-614`, `:809-811`; `test_corpus_goldens.rs:250` pins
  the law). The device: `gpu_backend/join.rs:164-179` ends `out.extend(prior)`; `:185-198` one
  batch; `execute_node` accepts one output per call (`gpu_backend/mod.rs:217-236`). Golden proof:
  `tpch.sf1/tp1-single-mini.cpu.txt:813-814` — `GpuHashJoin in_rows=[[1500000],[6001215]]
  batch_rows=[[8192,…]]` ×733 over a one-batch probe. `line_difference` prints only the first
  differing line plus "(+N more lines)" (`tests/common/golden_text.rs:158-187`), which is where the
  ticket's "in nothing else" came from. Every other CPU executor keeps one-out (`CpuExec::exec`
  `cpu_backend/mod.rs:173-201`; `one_batch` in `accumulate.rs`; the source). `architecture.md`
  already forbids this ("no executor may return more than one batch per call per output lane",
  Grouping sets; "queues are self-bounding at one batch per lane", Scheduling; "batch boundaries
  are a pure function of the plan", Determinism).
- **Fix (as corrected by the review):** `cpu_backend/join.rs` only: `declared(...) -> Vec<CpuBatch>`
  becomes `one_declared(...) -> CpuBatch` — each chunk through `declared_as`, then `concat_batches`
  (zero chunks → `RecordBatch::new_empty(schema)`, `arrow-select/src/concat.rs:281-283`); used at
  `:250` and `:265`; the early returns for `per_call: None` and `finish: None` stay; the doc says
  "whatever DataFusion chunked", never "8192" (review 3). Regression test in
  `cpu_backend/tests/join.rs` with `SessionConfig::new().with_batch_size(1)`: Inner → one batch,
  cross join → one batch where DataFusion yields three, LeftAnti finish → one. Not touched: the
  driver (no ≤1-output guard), the GPU backend, `GpuAggregateBatches`, the `Vec` return type,
  `without_build`. Frozen surfaces: none. Goldens: all ten `<mode>-mini.cpu.txt` and ten `.cost.txt`
  regenerated (dataset host, `UPDATE_CANONICAL=1`), read for two invariants — `output_rows`
  unchanged except at partial aggregates above joins and merges' `in_rows`, `mini.result.txt`
  byte-identical; plan/payload/memory sections untouched; `nested-limits` loses `abandoned=[92]`, so
  add a `test_golden_format` case for the parser arm (`golden_text.rs:277`; conservation law
  `test_corpus_goldens.rs:268-270`) — review 7. Registry: nine rows drop `185`; `gpu_tp1_single`
  enabled only after a shad-gpu run; `corpus_cases.inc` comments at 27-33, 142-149, 210-216,
  222-228 (the q11/#152 puzzle resolves: 336 and 88 are CPU chunk counts), 260-264, 272-275;
  `build-test.md:44`; the ticket archived with the corrected diagnosis; `architecture.md` gains no
  sentence — three become true.
- **Contested points:** (1) the proposal's first minimum query `SELECT count(*) FROM region, nation`
  — review 1: refused at plan time naming #158. **Code supports the review**: `CrossJoinExec`
  statistics multiply the sides (`cross_join.rs:386-395`), `Precision::multiply` keeps Exact×Exact
  (`datafusion-common/src/stats.rs:146-148`), `AggregateStatistics` replaces the aggregate with
  `ProjectionExec(PlaceholderRowExec)` (`aggregate_statistics.rs:49-85`), and the planner refuses
  that (`test_planner_join_refusals.rs:118-128`). #158's "the rule cannot fire … JOIN" is false for
  a cross join. (2) "all nine already run to completion with matching results" — review 2:
  `assert_section` panics before `assert_result` (`corpus_gpu.rs:101-111`), so no result was ever
  compared. **Code supports the review.** (3) Review 5 (the driver-guard rationale) and 6 (cost gate
  `is_regression` is `new > old`, `cost-report/src/main.rs:1195`; every figure falls) — minor, review
  right. (4) S vs M — M.
- **Minimum corpus query:** `SELECT count(*) FROM supplier s JOIN nation n ON s.s_nationkey =
  n.n_nationkey` — tpch sf1, any mode, both backends. Today CPU: `GpuHashJoin batch_rows=[[8192,1808]]`,
  `GpuAggregate [[1,1]]`, `GpuAggregateBatches in_rows=[[2]]`; device `[[10000]]`, `[[1]]`, `[[1]]`.
  After the fix both read the device's. Not in testdata (nearest: `tpch/join_int`, off on `152 185`).
- **Cells re-enabled / next wall:** the nine gpu tp1-single cells are candidates confirmed only by
  one shad-gpu run of `test_gpu_corpus` (their results were never compared); a cell failing past the
  join stays off with a fresh ticket. Their other modes stay on #152; q96/q88 tp4 on #180.
- **Overlaps and dependencies:** this is the wall #152's, #190's and #47's reviews name. Land before
  #152's registry pass and before #190's and #175's new sections are authored (one regen, not two).
  Touches nothing #175/#173 touch. #199 (a global aggregate on an empty lane) is a different
  batch-list divergence on the same page.
- **Complexity:** M — code S (one file, ~15 lines, one ~50-line test); the five-mode cpu regen read
  for two invariants and a never-compared device run make it M.
- **Unticketed defects found:** #158's text is wrong for a cross join; `abandoned` leaves every
  committed golden after the fix (parser arm uncovered without the new case); the CPU join violates
  three `architecture.md` invariants today (this ticket, retitled).

## #175 — an empty build side leaves three join types owing rows they cannot make

- **Status after research:** live; the approved `empty-build.md` covers one of its two corpus
  shapes (`tpch/q16`); `tpcds/q77` is outside the spec and lands on #189 afterwards. The ticket
  names `q21` — wrong; q21 is enabled at all five cpu modes (registry `:121`).
- **Issue:** `LaneCall::NoBuild` fires when the build side ends with no batch
  (`executor/driver/single_partition.rs:183-185`); `without_build` refuses for Right/Full/RightAnti on
  both backends (`gpu_backend/join.rs:103-114`, `cpu_backend/join.rs:184-192`; the table is
  `plan/join.rs:452-461`). Off: `tpch/q16` and `tpcds/q77` × cpu tp4-single/rowgroup/sized
  (registry 116, 78; `corpus_cases.inc:95`, `:243`) and their gpu cells behind; q77 out of the
  end-to-end cover with q2 standing in (`test_cpu_end_to_end.rs:357-365`); the injection matrix's
  hash dimension off for any plan with an owing join (`tests/common/injection.rs:664-667`, `:767-777`).
- **Root cause:** two shapes. (a) q16: the RightAnti's build is 4 filtered `supplier` rows scattered
  on `s_suppkey` (`tpch.sf1/tp4-single.plans.txt:946-975`); both emitters build a typed zero-row
  batch (`cpu_backend/emit.rs:73-75`; `node_session.cpp:405-421`) and `driver/partitioned.rs:380-386`
  drops it, so the coalesce below the join emits nothing (`cpu_backend/accumulate.rs:138-141`,
  `gpu_backend/accumulate.rs:209-212`). (b) q77: the Right's build is `Project ← AggregateBatches ←
  Aggregate ← Project ← HashJoin{Inner}(store ⋈ store_returns)` with no scatter between
  (`tpcds.sf1/tp4-single.plans.txt`, `== q77`); the `store` scatter leaves lane 2 empty in every
  tp4 golden (`tp4-single-mini.cpu.txt`: `batch_rows=[[5],[5],[],[2]]` ×15), the Inner takes
  `NoBuild` and drains (`single_partition.rs:187-192`), the grouped merge emits nothing, and the
  Right is asked at `NoBuild` — before it can learn that no probe row will ever arrive either.
- **Fix (spec + proposal, as corrected by the review):** Change 2 is the spec: `driver/index.rs`
  computes once per node whether its output reaches the build child (`children[0]`) of a join whose
  type owes rows when empty; `partitioned.rs:380-386` drops an empty only where that is false. With
  a zero-row build the lane takes `SetBuild` and `operators/join.cpp:295-302` already answers
  (Right = `left_join(probe, build)` with `NULLIFY`, which is the pad). RightAnti needs a C++ guard
  the spec's "not touched" list excludes: on ≥25.10 `filtered_join` is built directly (`join.cpp:159-161`,
  `:176-178`) and `filtered_join.cu:105-130` launches `grid_size(build.num_rows())` = 0 blocks.
  Guard all four semi/anti arms (`:122-124`, `:143-145`, `:159-161`, `:176-178`): when the table
  `filtered_join` would be built from is zero-row, anti → `thrust::sequence` over the other side,
  semi → empty. Never call the free `cudf::left_*_join` inside the `PEACOCK_HAVE_FILTERED_JOIN`
  branch: `[[deprecated]]` on 25.10 (`third_party/cudf/cpp/include/cudf/join/join.hpp:216`) and
  absent from the local 26.02.01 (`~/data/miniforge3/envs/rapids/include/cudf/join/join.hpp`, zero
  hits); shad-gpu builds against 25.02 (`scripts/lib/shadgpu-env.sh:14`, no `join/` dir), so the
  device run proves only the `#else` path. Driver test: make `MockAcc` `CoalesceAll` emit nothing
  when nothing arrived (`driver/mock.rs:421-430` emits a batch regardless, so `empty-build-impl.md`
  Task 2's `SetBuild`-not-`NoBuild` assertion passes before the fix) or assert the emitter's `Emit`
  count. CPU unit: Right, RightAnti **and Full** over `set_build(RecordBatch::new_empty)` (F10).
  Change 1 (proposal, outside the spec): `owes_probe_when_build_empty` on `IndexedNode`,
  `LaneState::OwedProbe` / `LaneCall::OwedProbe`, delete `JoinExecutor::without_build`
  (`executor/mod.rs:204-211`), both backend arms, `Calls.empty_build_answers_nothing`
  (`cpu_backend/join.rs:40-43, :79, :99, :163`), the mock knob (`mock.rs:85-91, :149, :519-525`, plus
  `single_partition/tests.rs:259` — F8), the injection gate (`injection.rs:576-615, :664-667,
  :767-777`, `test_layout_injection.rs:96,:102`); refuse only on a probe batch with rows, release a
  zero-row one (F7). Frozen surfaces: none (an internal trait loses a method under Change 1).
  Goldens: no existing section moves — both proposal and review count 0 kept empties under the
  guard in every enabled section (the spec's 244 and the impl plan's "low hundreds" count every
  permanently-empty lane whatever its consumer); `tpch.sf1/tp4-{single,rowgroup,sized}-mini.cpu.txt`
  and `.cost.txt` gain `== q16`; `tpch.sf1/mini.result.txt:453` q16 `mode=tp1-rowgroup` →
  `tp4-sized` (F3; `tests/common/corpus.rs:394-405`, `test_corpus_goldens.rs:574`). Registry: q16
  cpu tp4 → enabled, drop 175; q77 `175 → 189` only after a hand run records the `Unsupported data
  type in hasher: UInt8` signature (F11). Comments: `cpu_backend/accumulate.rs:129-137`,
  `gpu_backend/accumulate.rs:186-191`, `cpu_backend/tests/accumulate.rs:67-69`. Wiki:
  `architecture.md:606-607` gains the exception, `:855-857` narrowed; #175 corrected (q16/q77) and
  narrowed, not closed; #173 cut to its one refusing site; `build-test.md:42` "q77 three by #175" →
  #189; `test_cpu_end_to_end.rs:357-365` reason → #189.
- **Relation to the specs:** Change 2 is `empty-build.md` §2 plus the C++ guard; the proposal and
  review show the spec (i) never reaches q77, (ii) mispredicts the golden delta, (iii) omits the
  guard and (iv) the result-golden line, and that the impl plan's Task 2 test is vacuous. Change 1
  is outside the spec's Restriction ("no `without_build` signature change … code changes are limited
  to the conditional drop") — the spec's own rule makes that a finding for the human to amend
  Scope/Restriction/§6 before dispatch, not a scope increase (F5); the fallback "keep `without_build`,
  defer its `Err`" is outside it too. Rejected `empty-answers.md` §2 (a build-side `ProjectRole` pad,
  routing `RightAnti` without a call, `without_build(table)`, make-empty-of-schema on the wire): the
  proposal needs none of it — the C++ and DataFusion already compute the answer over a zero-row
  build — so the rejection stands.
- **Contested points:** F1 (the "smallest form" does not compile on 26.02) — **code supports the
  review** (headers checked above). F2 (the guard is unprovable on shad-gpu) — supports the review.
  F3 (result line) — supports the review. F4 (vacuous driver test) — supports the review
  (`mock.rs:421-430`). F5/F6 — review right: #175 keeps a residual (an owing join whose build got
  nothing because a join below drained, `single_partition.rs:187-192`, or a mid-plan limit dropped
  everything, `cpu_backend/accumulate.rs:407-410`, with probe rows) that Change 1 refuses by name at
  the first probe batch; narrow, do not close. F9 (`collect_statistics` is on by default,
  `ListingOptions::new` `collect_stat: true`) — review right. F10 (`q87` is not injected) — review
  right (`test_cpu_end_to_end.rs:378-390`).
- **Minimum corpus query:** shape (a): `SELECT count(*) FROM partsupp WHERE ps_suppkey NOT IN
  (SELECT s_suppkey FROM supplier WHERE s_suppkey = 1)` — tpch sf1, tp4-single (also rowgroup/sized),
  cpu and gpu. Plans as `tpch/anti-join`'s RightAnti (`tp4-single.plans.txt:29-38`) with a one-row
  build over four lanes; three lanes refuse "this lane's build side is empty … (#175)"; tp1 modes
  pass; expected 799,920. Shape (b) (needs Change 1): the proposal's q77-minus-rollup `LEFT JOIN`
  of two per-store aggregates over `store_sales` / `store_returns` — refused at tp4 on the CPU
  today; the swap to `Right` is read off q77's plan, not re-derived. Neither is in testdata.
- **Cells re-enabled / next wall:** `tpch/q16` × 3 cpu tp4 cells on (Change 2). `tpcds/q77` × 3:
  refusal gone under Change 1, then #189 — the top scatter hashes `__grouping_id:UInt8`
  (`tp4-single.plans.txt`, `== q77`, the `GpuEmitPartitions` under the final merge); so Change 1
  enables nothing today. Gpu cells stay: #152 (both), #183 (q16), #187 (q77). The injection hash
  dimension returns for `anti_join`, `q97`, `q93` and tpcds `q16`.
- **Overlaps and dependencies:** #173 (its B extends Change 2's climb and disputes the stopping
  rule; A/C independent); #152 (a kept empty on a probe lane is a second probe call — the spec's §5
  hazard, and why the guard reads the build child only); #189 (q77's next wall); #199 (adjacent,
  filed not fixed); `operator-cases.md` expects #175 `bug_` tests this deletes; `typed-nulls` (a
  padded Decimal128 column on a device — #173 F1); #185 (author q16's sections after its regen).
- **Complexity:** M — Rust ~70 lines under Change 2, ~60 more with ~60 deleted under Change 1, a
  ~15-line four-arm C++ guard, ~8 tests, additive goldens plus one result line; plus a spec-amendment
  round before dispatch and a shad-gpu run that proves less than promised.
- **Unticketed defects found:** the `filtered_join` zero-row launch in all four semi/anti arms on
  ≥25.10 (latent for any zero-row build or keys); four factual errors in `empty-build.md` and one in
  `-impl.md` (above); #175's ticket text (q21); `architecture.md:606-607`.

## #173 — the frozen surface cannot build a table out of nothing

- **Status after research:** live but blocks zero cells (no registry row carries `173`); the
  ticket's text overstates — three of its four sites emit nothing rather than refuse, one refuses,
  and a fourth site it does not name answers wrong on both engines.
- **Issue:** the sites: `gpu_backend/accumulate.rs:209-212`, `:247-250`, `:387-389` return
  `Ok(Vec::new())` (CPU twins `cpu_backend/accumulate.rs:138-141`, `:196-199`, `:270-272` the
  same); `gpu_backend/join.rs:209-238` `finish_without_keys` refuses Left/Full/LeftSemi/LeftMark and
  hands the raw build up for LeftAnti, ignoring `projection` (hacks-audit 11); the C++ throw at
  `node_session.cpp:273-282` is unreachable from Rust. Where the CPU answers and the device does
  not: (i) a finishing join lane with build rows and no probe batch (`cpu_backend/join.rs:253-266`
  answers all five). Also (ii) #175's residual; (iii) a mid-plan `GpuLimit` whose interval names no
  row emits nothing (`gpu_backend/accumulate.rs:420-424`, `cpu_backend/accumulate.rs:409-411`), so a
  join above meets (i) or (ii) at one lane; and (iv) review F4: the one-call finishing types —
  filtered LeftSemi/LeftAnti/LeftMark (`plan/join.rs:427-435`: `probe_streams: false, needs_finish:
  true`, so `answers_in_one_call`, `plan/mod.rs:772-774`) and nested-loop Left — publish no at-done
  call, and `finish_and_fetch` returns nothing at `gpu_backend/join.rs:186-188` /
  `cpu_backend/join.rs:255-257`: with build rows and no probe batch a filtered LeftAnti owes every
  build row and emits none — a silent wrong answer on both engines. None of (i)-(iv) is in the sf1
  corpus: no finishing join has an empty probe lane in any enabled golden (both sides measured).
- **Root cause:** the surface builds a table only by reading one; but every place a stream becomes
  nothing already holds a typed table and drops it — the scatter (`partitioned.rs:379-386`), the
  limit (`slice_handle(h, 0, 0)` is one existing call), the finish (the build handle is still held,
  `GpuProbingJoin::build` `:136`, and the at-done pad/narrow projects read exactly the build schema,
  `wire/join.rs:113-135`, `:261-322`).
- **Fix (as corrected by the review):** A, device, `gpu_backend/join.rs`: `finish_without_keys`
  seeds the at-done projects from the build handle — LeftAnti/Left/Full = the build, LeftSemi =
  `slice_handle(build, 0, 0)`, LeftMark refused (it needs a keys-schema table); thread `build:
  SchemaRef` from `executors_for` (`gpu_backend/backend.rs:128-136`) — two files, not one (F9c).
  **Split** (F1): land LeftAnti-narrow and LeftSemi now; Left/Full after `typed-nulls.md` or as
  `bug_` tests naming #198 — the pad project's bare numeric NULL is `is_ast_able`
  (`operators/project.cpp:45-49`, `expr.cpp:414`), `build_expr` builds it valid (`:157-255`) and a
  Decimal128 as FLOAT64 (`:212-225`), and the pad has never run on a device (Left/Full refuse at
  the first probe batch). C, both backends' `LimitStream`: keep one zero-row spare from the first
  out-of-interval batch, emit it at done iff nothing was emitted; costs one `slice_handle` per
  device run of a mid-plan limit with an offset (F11); incidentally hands #199's limit route a
  zero-row batch. B, driver, extends #175 Change 2: B1 the climb continues through a non-owing join
  on the side that passes emptiness and (F5) through `GpuMergePartitions`/`GpuUnion`/`GpuInterleave`,
  stopping at `GpuEmitPartitions`/`GpuMergeSortedPartitions`/`GpuUnload` — re-measured: the merge
  climb guards zero extra lanes in any enabled golden; B2 one spare per lane, delivered at the
  emitter's done (`partitioned.rs:348-353`), `release_in_flight` (`:640-649`) releases spares; B3 a
  non-owing join over a zero-row build probes once then `DropProbe` — recorded on the lane in
  `run`'s `SetBuild` arm (`single_partition.rs:301-317`), since `select` has no `site` (F6). B also
  needs the four-arm `filtered_join` guard (F7: for LeftSemi/LeftAnti/LeftMark the zero-row table is
  `right_keys`, `join.cpp:122-124`, `:143-145`) and the mock fix (F8). Python twin
  `exec_model/partitioned_driver.py:296-301` for B. Frozen surfaces: none. Goldens: none move on
  this ticket. Tests: `test_gpu_executors/join.rs:335-378` stays; new Left / LeftSemi /
  LeftAnti-with-projection / LeftMark-refused cases and CPU twins beside
  `cpu_backend/tests/join.rs:600-618`; the F4 case (filtered LeftAnti, `set_build` then
  `finish_and_fetch`, no probe → every build row) red today on both backends; limit-spare cases on
  both; `index/tests.rs` climbs; `driver/tests/flow.rs` spare delivery; cross-product sweep
  (`single_partition/tests.rs:328-380`). Docs: `architecture.md:476-479`, `:606-607`, the `GpuLimit`
  row and "streams and holds nothing", `:854-858`; `test_gpu_executors/accumulate.rs:296-329` doc
  drops "(#173)"; `cpu_backend/tests/accumulate.rs:66-72`.
- **Relation to the specs:** no approved spec owns #173; `empty-build.md` §4 says "cut #173 to its
  one real site, it blocks zero cells" — the proposal agrees on cells and disputes "one site" (two
  nodes turn a stream into nothing; F4 adds a fourth). Rejected `empty-answers.md` wanted
  `output_schema` on the wire and a C++ `make_empty_column` arm: A and C use handles the lane already
  holds and touch no wire, so they survive the rejection reason; B survives the rejection but
  contradicts the frozen `empty-build.md` Restriction ("a scatter feeding a probe side still drops
  its empties"; "code changes are limited to the conditional drop") — the human amends or B gets its
  own spec (F3). `refcounted-tables.md`: independent; B3 keeps a guarded lane off #152 until then.
- **Contested points:** (1) proposal vs the #175 sibling on climbing past a non-owing join — the
  review (and the code) support feasibility: DataFusion's `process_probe_batch` returns a batch
  whatever the count (`hash_join.rs:1567`), `single_node.rs:52-56` filters nothing, the C++ map arm
  returns one handle, so both engines emit one zero-row batch per probe call over a zero-row build
  and B3 bounds it to one; but B guards zero lanes in any enabled golden and costs q77 lane 2 about
  six calls where `OwedProbe` costs none — the human's trade. (2) F1 — **code supports the review**
  and contradicts the #152 review (cross-ticket 2). (3) F2 — the `LEFT JOIN` minimum query dies on
  #152's `copy_of` before any finish: **code supports the review** (`wire/join.rs:75-79`). (4) F5
  "#173 closes" — review right: a LeftMark finish over a merge whose every lane drained still
  refuses; narrow, or take the merge climb. (5) F9 — 30 finishing joins at tp4-single, not 38; 451
  permanently-empty lanes at 244 sites. (6) F12 — the CPU's LeftSemi finish over no keys is `[0]`
  (`process_unmatched_build_batch` `:1573-1618`), matching the device arm; drop the fallback.
- **Minimum corpus query:** shape (i) on a device: `SELECT count(*) FROM orders WHERE o_orderkey IN
  (SELECT CASE WHEN l_quantity > 100 THEN l_orderkey END FROM lineitem)` — tpch sf1, the tp4 modes;
  LeftSemi with a key-project-only probe, every probe key NULL lands in one lane, three lanes reach
  `finish_without_keys` and refuse "no rows … (#173)"; expected 0. LeftMark form: `SELECT count(*)
  FROM orders o WHERE o.o_orderkey IN (SELECT l_orderkey FROM lineitem WHERE l_orderkey = 1) OR
  o.o_custkey = 1`. Shape (iii), both backends, any mode: `SELECT count(*) FROM region r LEFT JOIN
  (SELECT n_regionkey FROM nation LIMIT 5 OFFSET 100) n ON r.r_regionkey = n.n_regionkey` — the
  limit emits nothing, the device refuses at the finish, the CPU answers 5 (if DataFusion swaps it
  to Right it is #175's residual). The proposal's `LEFT JOIN` form is the query the Left arm answers
  after #152 and typed-nulls. None is in testdata.
- **Cells re-enabled / next wall:** none on #173's account; `operator-cases.md:59`'s `bug_` row
  shrinks to LeftMark-with-no-probe.
- **Overlaps and dependencies:** #175 (B after its Change 2, same branch or next; A/C any time);
  #152 (B3; the Left/Full arms are unreachable from a planned tree until #152); #198/`typed-nulls`
  (A's Left/Full arms; a Decimal128 pad stays FLOAT64 by design → its own `bug_`); #199; #158 (a
  table of literals, a different capability — stays refused).
- **Complexity:** M for A+B+C; S for A(split)+C (three files, no spec conflict). The review's
  recommendation, supported by the code: land A(split)+C; put B to the human with the q77
  call-count trade and the merge climb, or narrow #173 to the LeftMark-through-a-merge shape.
- **Unticketed defects found:** F4's silent wrong answer (filtered semi/anti/mark and nested-loop
  Left; build rows, no probe batch; both engines); the pad project's typed-NULL / Decimal128 defect
  and the drift in #198 and `typed-nulls.md` ("a bare literal short-circuits to `build_scalar`" is
  false in a project for numeric literals); `finish_without_keys`'s LeftAnti ignoring `projection`;
  the `filtered_join` zero-row keys on ≥25.10.

## #190 — the CPU backend drops a nested-loop join's projection

- **Status after research:** live; one argument.
- **Issue:** `executor/cpu_backend/join.rs:140-146` builds `NestedLoopJoinExec::try_new(…, &join_type,
  None)`; the hash-join path passes `node.projection` (`:303-313`). `declared_as`
  (`cpu_backend/mod.rs:239-277`) refuses on the column count. Off: `tpch/q11` ×5 and `tpcds/q54` ×5
  cpu, and their gpu cells (registry 111, 55; `corpus_cases.inc:103-106,114`, `:212-213,219`).
  Behind #163 and unlisted: `tpch/q22` (row 122) and `tpcds/q24` (row 25) carry a projecting
  nested-loop join at every mode and hit this next.
- **Root cause:** DataFusion embeds the projection when the filter reads a column the output drops
  (`nested_loop_join.rs:566-598` → `try_embed_projection`, `projection.rs:381-433`); the translator
  keeps the sides (`nodes.rs:492-493`) and declares the projected schema (`:517-518`);
  `check_projection` validates (`plan/join.rs:66-75`); the wire (`wire/join.rs:342-386`) and the C++
  (`join.cpp:519-529`) apply it. Only the CPU ignores it.
- **Fix (as corrected by the review):** map `node.projection` to `Vec<usize>` as at `:303-306` and
  pass it (optionally a shared helper). Regression test in `cpu_backend/tests/join.rs` beside `:365`:
  `Some(vec![1, 3])` over `dim × FACT` with `k > fk` → `c|20`, `c|21` (red today: 4 vs 2 fields);
  optional `Some(vec![3, 1])` to pin the reorder (F6). Frozen surfaces: none. Goldens: none change;
  22 sections are authored — run all five modes of both queries under `PCK_UPDATE_SECTIONS=1`
  (F5: `mini.result.txt` is written only by the last declared mode). Registry: ten cpu cells on;
  gpu: q54 all five `152` — its `(i_item_sk, item_sk)` join probes a merge over a two-lane union
  (`tpcds.sf1/tp1-single.plans.txt:5097-5102`, recipe `build copy`), refused on the second batch at
  every mode as q2/q71 are; q11 four modes `152`, tp1-single off on whatever one run names (expect
  #185: the partsupp join emits ~800k rows → ~98 CPU chunks), fallback `152` plus a comment.
  `build-test.md:42`: N 447 → 457 and rewrite the stale "37 queries … thirteen on #163" (26 fully
  out today: 23 #163, 2 #190, q64 #192). Ticket to the archive with its anchor.
- **Contested points:** F1/F2 — proposal: "possibly a bonus device cell each"; review: q54 is #152
  by structure and q11's tp1-single cell cannot pass the byte-exact compare. **Code supports the
  review on both** (the plan golden; `corpus_gpu.rs:101-105`). F4 (tp4 modes plan merges) — right,
  immaterial. Nothing else.
- **Minimum corpus query:** `SELECT b.n_name FROM region a, nation b WHERE a.r_regionkey <
  b.n_regionkey` — tpch sf1, tp1-single, cpu. Plans with `projection=[n_name@1]` (the filter reads
  columns the output drops); refused at the first `probe_and_fetch` with "3 columns … 1 field"; after
  the fix answers. Not in testdata (`nested-loop-join.sql` is `SELECT *`).
- **Cells re-enabled / next wall:** ten cpu cells on; ten gpu cells attributed by reading (#152 ×9,
  q11 tp1-single → #185 or a device refusal on the decimal nested-loop path); q22/q24 cleared on
  this axis for #163's rollout.
- **Overlaps and dependencies:** #163 (q22/q24 next); #152/#185 (the gpu column);
  `operator-cases-impl.md:1184-1188` plans a `bug_` for it — deleted by whichever lands second;
  author the 22 sections after #185's regen. The cross-join sibling — `GpuCrossJoin.projection`
  (`plan/mod.rs:686-689`) is dropped by `CpuJoin::cross` and absent from `CudfCrossJoin`
  (`gpu_plan.fbs:440-443`) — is a latent both-backend gap no golden reaches; a ticket when
  `operator-cases` surfaces it.
- **Complexity:** S.
- **Unticketed defects found:** the cross-join projection gap (latent); `build-test.md:42` stale;
  the C++ treats an empty projection vector as none (`join.cpp:519`, unreachable).

## #47 — q77 GPU returns 40 rows vs CPU 45

- **Status after research:** stale — fixed by `e5d2c0e7` (grouping-set expansion) and `d3c92de5`
  (`null_policy::INCLUDE`), both 2026-06-11, neither an ancestor of the filing commit `2d07e908`
  (2026-06-10), both in HEAD (checked with `git merge-base --is-ancestor`); q77 was never re-run on
  a device. Closes on a device pin, not on the argument.
- **Issue:** registry row 78 carries `47`; it disables nothing on its own — q77's gpu cells refuse on
  #187 at tp1-single and #152 elsewhere, its cpu tp4 cells on #175 → #189. It is why q77 is absent
  from the exec-model corpus (`scripts/exec_model/tests/plans_tpcds.py:3-7`).
- **Root cause (at the filing commit):** `2d07e908:cpp/src/plan_executor.cpp` `execute_aggregate`
  never read `grouping_sets`/`null_exprs` — one set of three instead of the rollup → −4 rows (grand
  total + three channel subtotals); `groupby gb{keys_view}` took cuDF's default `EXCLUDE` → the NULL
  `cs_call_center_sk` group dropped in the cs CTE's own groupby (the ticket's "upstream aggregate
  branch" — review 6) → −1. Today's CPU answer is 44 (`tpcds.sf1/mini.result.txt`, `== q77`,
  counted; the DuckDB profile at `df99bf61` records `TOP_N` 44; the data was pinned at DuckDB v1.2.2
  since `12ad4999`, `2d07e908:testdata/generate_testdata.sh:43`), 44 − 5 = 39, and the ticket's
  45/40 is one more on each side. Today's `operators/aggregate.cpp:340-434` expands per set with
  `INCLUDE` (`:399`, `:441`); the Right arm is the filing commit's (`join.cpp:295-302` vs
  `2d07e908` `:1633-1641`) and matches every probe row on today's data (`tp1-single-mini.cpu.txt:3703-3704`,
  `:3795-3796`).
- **Fix (as corrected by the review):** no engine change. `test_gpu_recipe_walk.rs`: a
  `ROLLUP_OVER_LANES` const (nation ∪ region with a CASE-made NULL id, `GROUP BY ROLLUP(channel,
  id)`) and `a_rollup_over_a_union_folds_its_totals_across_lanes` at `ONE_LANE`, asserting 2 inits /
  3 merges / 1 finalize and DataFusion agreement (13 rows; 10 / 9 / 12 / 14 name the four loss
  modes); the coverage list at `:818-828`; one doc sentence naming the CASE as the one expression no
  device has evaluated through the recipe wire with a typed-NULL `then` arm (review 5 — the path is
  `build_column_case` → `build_column` → `build_scalar`, which honours `is_null`, so it reads
  correct). Registry row 78 drops `47` (and the archived `97`). `tickets.md` #47 → archive as Done
  with anchor + header + body (`cost-report/src/main.rs:270-296` `anchored_numbers`); Contents 17 →
  16. `build-test.md`: walk row N 10 → 11, Rust 1135 → 1136, total 1569 → 1570 (review 1). Frozen
  surfaces: none. Goldens: none.
- **Contested points:** review 2 ("unpinned DuckDB" is false; the profile says 44) — **evidence
  supports the review** (checked above). Review 3 (Right has run on the current executor: q93 at
  tp1-single completed in T19 and did not throw; its rows were never compared because the section
  failed first on the 36-batch chunking, `tp1-single-mini.cpu.txt:4953-4954`) — right; the residue
  is smaller than the proposal says. Review 4 (tp4-single plans 4+4 lanes, not 2) — **code supports
  the review** (`nodes.rs:386`). Review 8 (task 13 owns the same file) — coordination.
- **Minimum corpus query:** the `ROLLUP_OVER_LANES` SQL, tpch sf1: runs at the tp1 modes on both
  backends, refused at every tp4 mode on the CPU by #189 (`__grouping_id:UInt8` in the top scatter);
  the device run is the walk at `ONE_LANE`. Expected 13 rows. Not in testdata. The `Right` residue:
  `SELECT n_name, r_name FROM nation LEFT JOIN (SELECT r_regionkey, r_name FROM region WHERE
  r_regionkey < 3) r ON n_regionkey = r.r_regionkey` (25 rows, 10 NULL) — runnable in the walk only
  once task 13 admits `Right` (`driven()` refuses it, `:780-781`).
- **Cells re-enabled / next wall:** none; q77's gpu cells stay on #187/#152, its cpu tp4 cells on
  #175 → #189. When its tp1-single device cell runs after #187, expect #185's chunking first (the
  store_sales Inner join is 8 CPU batches `:3737-3738`, the cross join 5 `:3757-3758`) and the
  rollup init's 46 vs 70 `output_rows` — batch shape, not row loss.
- **Overlaps and dependencies:** task 13 `walk-drives-every-plan.md` rewrites `driven()`, `PROVEN`
  and the coverage list of the same file — whichever lands second rebases; #65 — the gid is INT32 on
  the device with bit `i` set (`aggregate.cpp:392`) and `UInt8` with bit `n−1−i` on the CPU
  (`aggregates/mod.rs:1273-1275`), invisible at tp1 and a hash-placement divergence at tp4 the day
  #189 lets the CPU hash `UInt8` (q77, q5, q80, `rollup_over_join`); #189; #175; #185/#187 (q77's
  real next walls); #46 (the same closing pattern).
- **Complexity:** S — about forty lines in one test file, one registry cell, one ticket moved, one
  wiki row and two numbers, one shad-gpu cycle.
- **Unticketed defects found:** #65's "unobservable while no enabled query projects `GROUPING()`" is
  wrong once #189 lands (above); nothing else.

## #60 — q78 GPU diverges in anti-join + top-N; possibly memory-borderline

- **Status after research:** misdiagnosed — real cause: cuDF's float `round(x, p > 0)` kernel is
  `modf` then `ip + round(fp·10^p)/10^p` (`third_party/cudf/cpp/src/round/round.cu:108-117`
  `half_up_positive`), DataFusion's is `(x·10^p).round()/10^p`
  (`datafusion-functions-45.0.0/src/math/round.rs:169-170`, the Array arm — review F4), and they
  differ by one ulp on about one value in twenty at or above 1.0. q78 has no anti join (three
  `Right` + `IS NULL` branches and a top `Left`, `tpcds.sf1/tp1-single.plans.txt:8413-8453`), its
  join and sort arms are the filing commit's byte for byte, and its 3-key sort prefix is unique
  over the answer. Live.
- **Issue:** `cpp/src/expr.cpp:707-722` calls `cudf::round(fcol, places, HALF_UP)` for any `places`
  — the arm the filing commit `57bed016` added. Re-emulated here over the committed answer
  (`mini.result.txt`, `== q78`, 100 rows, IEEE double): exactly five rows split — (25,9) 2.78,
  (76,56) 1.36, (38,15) 2.53, (18,11) 1.64, (42,22) 1.91 — which `ryu` renders as
  `2.7800000000000002` etc. under `golden_exact` (`tests/common/result_text.rs:39-44`). `q54`'s
  `round(x)` is `p = 0` → `half_up_zero` = plain `round` (`:90-98`), which is why "round is proven
  fine" proved nothing about `p > 0`. `q2` takes the same kernel seven times per row
  (`testdata/tpcds-queries/q2.sql`, seven `round(…, 2)`; `tp1-single.plans.txt:1664`) — review F1.
  Off on its own: nothing — q78's five gpu cells refuse on #152 first (the top Left's `BatchCopy`,
  `gpu_backend/join.rs:293-299`, at every mode), then #187 at the unload.
- **Root cause:** two roundings against one; `fl(0.78)` puts `2 + fl(0.78)` exactly on a midpoint
  of the result grid and ties-to-even takes the upper double; rows with ratio < 1 cannot split.
- **Fix (as corrected by the review):** A, `expr.cpp:722`: `places == 0` → `cudf::round(·, 0,
  HALF_UP)`; otherwise multiply by a FLOAT64 scalar `10^|p|` (exact for p ≤ 22; `1/10^|p|` for
  negative p), `cudf::round(·, 0, HALF_UP)`, divide — three IEEE operations, bit-identical to the
  oracle; the comment at `:703-706` says which kernel agrees (≤ 4 lines); `<cstdlib>` is already at
  `:31`. B, `test_gpu_recipe_walk.rs`: `ROUND_PLACES` (`SELECT n_nationkey, round((CAST(n_nationkey
  AS DOUBLE) * 25.0) / 9.0, 2) AS r FROM nation`) and `round_with_places_lands_on_the_oracles_double`
  at `ONE_LANE` — red today on k=1 (`2.7800000000000002`) and k=2 (`5.5600000000000005`), 23 rows
  equal; the coverage list. Plus (F2) `bug_a_desc_key_puts_its_nulls_last` in
  `test_gpu_executors/exec.rs` beside `:80-124` for the DESC-NULLS defect below, ticket number above
  it, deleted when the two sites are fixed. C: #60's body rewritten under the same anchor naming q78
  and q2; registry row 79 `60 97 152` → `152`; `build-test.md` walk row 10 → 11, executors row
  31 → 32, totals 1569 → 1571 / 1135 → 1137; `scripts/exec_model/operators/expressions.py:237`
  docstring (the Python already computes the three-step form). Frozen surfaces: none. Goldens: none.
  CPU: none (the CPU relays `round` to DataFusion, `cpu_backend/expr_physical.rs:110-121`).
- **Contested points:** F1 (q2) — **code supports the review**. F2 (a `bug_` test is owed by
  coding-style's "Building around a bug") — review right. F4 (which arm runs) — same formula;
  review right. F5 (the optional corpus row touches 16 goldens, not 11: five `.plans.txt` too) —
  review right. The mechanism itself is not disputed; my re-emulation agrees on the five rows.
- **Minimum corpus query:** the `ROUND_PLACES` SQL — tpch sf1, plans and runs at every mode on both
  backends, device via the walk at `ONE_LANE`; today two rows differ. Not in testdata. The ticket's
  named shape, isolated: `SELECT ws_item_sk, ws_order_number FROM web_sales LEFT JOIN web_returns ON
  wr_order_number = ws_order_number AND wr_item_sk = ws_item_sk WHERE wr_order_number IS NULL ORDER
  BY ws_item_sk DESC, ws_order_number DESC LIMIT 100` — `Right` + `IS NULL` + `GpuSort(fetch=100)`,
  Int64 only, one probe batch at tp1-single so it runs on a device today with nothing in the way
  (#152 at the other four modes); the first `Right` and the first `DESC LIMIT` on a device under a
  test. Optional as a custom corpus row `tpcds/anti-join-topn` (16 goldens).
- **Cells re-enabled / next wall:** none; q78 stays on #152 then #187. After both, its
  `gpu_tp1_single` under `golden_exact` must differ in exactly the five `ratio` cells before A and
  in nothing after.
- **Overlaps and dependencies:** #152/#187 (q78's walls); #56/#152 (q2's device cells — q2's
  `round` argument is `CAST(Decimal128 / Decimal128 AS Float64)`, and the fixed-point → double cast
  is the next unproven step); `typed-nulls` also edits `expr.cpp` (the `build_expr` literal arm,
  `:157-255`) — different lines, same file; task 13 (the walk file). **The DESC-NULLS defect**
  (new ticket, next free number 201): `operators/sort.cpp:45-46` and `node_session.cpp:314-315` map
  `nulls_first` to `null_order::BEFORE`/`AFTER` regardless of direction, and cuDF flips the
  comparison for a DESCENDING key (its own Python sends `asc ^ first`); arrow's `nulls_first` is
  positional and DataFusion defaults `DESC` to `nulls_first = true`
  (`datafusion-sql-45.0.0/src/expr/order_by.rs:107`), which the CPU relays (`cpu_backend/mod.rs:522-527`).
  A silent wrong answer for any `ORDER BY <nullable> DESC [LIMIT n]` with NULLs, at every mode; inert
  for q78 (no NULL keys reach its sort). Fix: `BEFORE` iff `nulls_first == asc` at both sites;
  `architecture.md:1044` (`cudf::null_order` row) moves with it. No enabled device cell sorts, which
  is why nothing is red.
- **Complexity:** S (twelve C++ lines, one walk case, one `bug_` test, counts and a docstring, one
  shad-gpu cycle); the DESC-NULLS fix is its own S.
- **Unticketed defects found:** DESC-NULLS (above); the archived `97` in a live registry cell.

## Cross-ticket observations

1. **The batch-shape wall, one statement.** The CPU join executor hands the driver DataFusion's
   output chunks and the device returns one table per call, and the device tier compares the
   per-batch lists byte for byte, so no device cell whose plan has a join (or a partial aggregate
   above one) emitting more than `batch_size` rows per call at that mode can pass — whatever #152,
   #183 and #187 do.
   - CPU side: `executor/cpu_backend/join.rs:236-250` `probe_and_fetch` and `:254-265`
     `finish_and_fetch` return `declared(...)`, one `CpuBatch` per chunk (`:269-273`);
     `cpu_backend/single_node.rs:51-56` collects every batch of the stream; DataFusion 45 bounds a
     hash join's matched pairs per chunk at `batch_size` = 8192 (`joins/hash_join.rs:1472-1479`;
     `lib.rs:25-36` keeps the default), `CrossJoinExec` emits one batch per build row, a semi/anti
     finish emits empties per probe range plus the answer. The driver queues every element
     (`driver/partitioned.rs:307-313`) and `render_run` writes the list (`plan_text/run_text.rs:81-87`),
     which the cpu tier authors into `<mode>-mini.cpu.txt` — e.g. `tpch.sf1/tp1-single-mini.cpu.txt:813-814`
     (`GpuHashJoin in_rows=[[1500000],[6001215]] batch_rows=[[8192,…]]` ×733 over a one-batch
     probe), q93's Right join at 36 chunks (`tpcds.sf1/tp1-single-mini.cpu.txt:4953-4954`), q77's
     store_sales join at 8 and cross join at 5 (`:3737-3738`, `:3757-3758`).
   - Device side: `executor/gpu_backend/join.rs:164-179` ends `out.extend(prior)` — at most one
     batch per probe call; `:185-198` one per finish; `execute_node` maps one call to one handle
     (`gpu_backend/mod.rs:217-236`).
   - The comparator: `tests/common/corpus_gpu.rs:101-105` → `corpus_golden.rs:111-119`
     `assert_section`, byte-equal, and it panics before `assert_result`. The six passing device
     cells (q6 ×5, q19 tp1-single) are the ones that never exceed the bound.
   - Named by: #152 review F2 ("no ticket names it"), #185 proposal §1-2 (as #185's real cause),
     #190 review F2, #47 proposal §7 and review 3.
   - Resolution: it **is** ticketed — it is #185 under a wrong title. The #185 proposal's CPU-side
     concat (`one_declared` in `cpu_backend/join.rs`) is the fix, the #185 review accepts it, and
     `architecture.md` already requires it ("no executor may return more than one batch per call per
     output lane"; "queues are self-bounding at one batch per lane"; "batch boundaries are a pure
     function of the plan"). The #152 review's other two options (relax the comparator; chunk on the
     device) are rejected in #185 §4 for reasons the code supports. Do not file a new ticket;
     retitle #185, and sequence it before #152's registry pass and before #190's and #175's new
     sections are authored.

2. **Two reviews contradict each other on a device's typed NULL in a pad project.** The #152 review
   (§3) says the Left test's `(b,5,NULL,NULL)` is producible today because "a bare typed-NULL literal
   takes `build_scalar`'s null path"; the #173 review (F1) says the numeric NULL is a typed zero and a
   Decimal128 NULL a FLOAT64 column. **The code supports #173**: `operators/project.cpp:45-49`
   asks `is_ast_able` before `build_column`, `expr.cpp:414` returns true for any non-string literal,
   and `build_expr`'s ten literal arms pass `valid = true` (`:157-255`, Decimal128 → double at
   `:212-225`). #198's and `typed-nulls.md`'s sentence "a bare literal short-circuits to
   `build_scalar` at `build_column:830`" describes `build_column`'s entry, which a project reaches
   only for string literals — drift in both. Consequences: #152's positive Left/Full tests and #173
   A's Left/Full arms depend on `typed-nulls` landing first (or land as `bug_` tests), and a
   Decimal128 pad column stays FLOAT64 after it by design ("One arm differs on purpose") — a type
   divergence at every Left/Full pad with a decimal probe column that wants its own `bug_` test and
   ticket.

3. **The approved `empty-build.md` is wrong in four places**, found by #175/#173 and verified: q77 is
   shape (b) and the spec's build-child climb never reaches it; the 244 / "low hundreds" golden
   delta is 0 existing sections (both proposals and both reviews measured it); `operators/join.cpp`
   needs the `filtered_join` zero-row guard on ≥25.10 (F1 of #175); `mini.result.txt` q16's mode
   line moves. Its impl plan's Task 2 test cannot go red under the mock (`driver/mock.rs:421-430`)
   and Task 4's expectation is inverted. Both #175 Change 1 and #173 B are outside its Restriction —
   the human amends the spec or splits them off; neither enables a corpus cell today.

4. **The `filtered_join` zero-row launch** (`third_party/cudf/cpp/src/join/filtered_join.cu:105-130`,
   `grid_size(build.num_rows())`) is latent in all four semi/anti arms of `join.cpp` on ≥25.10 —
   over the build for RightSemi/RightAnti, over the probe keys for LeftSemi/LeftAnti/LeftMark — named
   by #175 F1 and #173 F7; unprovable on shad-gpu (25.02 takes the guarded `#else` path); one guard,
   one ticket, one `bug_` test that only the 25.10 CI leg could ever turn red.

5. **File and spec collisions.** `refcounted-tables.md` is frozen and #152 is its strict prefix —
   carving it is the human's. Task 13 (`walk-drives-every-plan.md`) and #47 and #60 all append to
   `test_gpu_recipe_walk.rs`'s `PROVEN`/coverage list. `typed-nulls` and #60 both edit `expr.cpp`.
   `operator-cases.md` plans `bug_` tests on #152, #175 and #190 that these fixes delete. The layout
   chain (ENS-drop-mode-name) may not run beside the refcounted chain (`tasks.md`).

6. **Two legacy tickets close in opposite ways.** #47 is stale by history (two 2026-06-11 commits)
   and closes on a walk pin; #60 is a live defect under a wrong name and closes on a twelve-line
   kernel change. Both leave q77's and q78's cells where they are, and both use the recipe walk as
   the device instrument because the corpus device tier cannot pass any join query (observation 1)
   — the walk exports raw and compares names plus rows, so it side-steps #183, #187 and the chunking.

7. **#65's "unobservable" is false once #189 lands**: the gid is INT32 with bit `i` on the device
   (`aggregate.cpp:392`) and `UInt8` with bit `n−1−i` on the CPU (`aggregates/mod.rs:1273-1275`), so
   the top scatter of q77, q5, q80 and `rollup_over_join` places subtotal rows by different hashes
   at tp4 — #47 §7, review-verified. #189's fix surfaces it; say so in #189 and #65.

8. **Ticket text drift found in this group:** #175 names q21 (q16/q77); #173 names three sites that
   emit nothing rather than refuse, and misses F4's silent wrong answer; #185's title and mechanism;
   #60's mechanism; #47's 45/40 (39/44); #158's "cannot fire with a JOIN" (false for a cross join);
   #198's short-circuit sentence; `build-test.md:42` counts and the "thirteen on #163" list.

9. **Sequencing suggestion:** #185 (regen) → #190 (S; authors 22 sections) → #175 Change 2 with the
   four-arm guard → #173 A(split)+C → #152 (`retain`) → the 79-row registry pass. #47 and #60 any
   time (rebase against task 13). `typed-nulls` before #152's Left tests and #173's Left/Full arms.
   #175 Change 1 and #173 B together as one spec amendment if the human wants them; each buys no
   cell today.
