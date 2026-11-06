# dag-driver detail

Branch `ENS-dag-driver`, forked from `ENS-sf40-baseline` (`080b1bac`) on 2026-11-04.
Plan: `dag-driver-impl.md`, Tasks 1–11; each *Commit* step is a checkpoint the coordinator commits and pushes.

## Dispatches

- 2026-11-04: developer dispatched at Task 1 Step 1. Verda unreachable (no VERDA_CLIENT_ID); CPU tests run locally. Device steps on nebius-gpu (L40S).
- 2026-11-04: developer dispatched at Task 3, all steps, CPU only, local (verda unreachable).
- 2026-11-04: developer dispatched at Task 4, all steps, CPU only, local (verda unreachable).
- 2026-11-04: developer dispatched at Task 5, all steps, with Task 4's carry-over (the random-DAG runs), CPU only, local.
- 2026-11-05: developer dispatched at Tasks 6–11, stopping at each commit step.

## Task 1 notes

- `duplicate_handle` copies the original's `produced_by` entry instead of calling `producer_of`, as the plan's Step 3 put it. `producer_of` throws for a handle that is adopted or older than timing. A duplicate opens no region, so the refusal stays at the export of either handle, as before. `HandleDuplicate.IsChargedToTheOriginalsProducer` checks the duplicate opens no region: 6 regions, the export's at seq 1, call 1.
- An unknown handle resets the session, as `slice_handle` does (Interfaces). `AnUnknownHandleIsRefused` checks the next call sees "no plan loaded".
- The plan's file:line references are stale: `peacock_handle_release` is at `peacock_gpu.h:389`, not 299. The ABI count in architecture.md was already twenty-nine, which matches the header, and is now thirty.
- Counts after Task 1: `--lib` 987 (985 passed + 2 ignored), C++ CPU 64, `peacock_plan_tests` 135, gpu lib `gpu_tests::` 743. build-test.md's tier headers and rows are updated to match.
- The Step 4 run on the L40S: `ALL rc=0`. plan 135, join session 56, gpu 5, cpu 64, core gpu lib 743, test_gpu_corpus 989, benchmarks 9 (`--skip bench_`), node timing 1. The five new device cases are OK in `.gpu-test/` logs.

## Task 2 notes

- `executor/driver/tests/plans.rs` moved, unchanged, to `src/tests/plans.rs`. `plan`'s tests cannot reach `executor::driver`, which is private to `executor`. The plan says the dag tests build from it, and Tasks 4 and 7's validate tests need it too. The 14 driver test files import `crate::tests::plans`. rustfmt reordered those imports. It also reflowed one `assert_eq!` in `counts.rs`, which was not rustfmt-clean at HEAD.
- The plan's readers for `unload(filter(S))`, `unload(project(S))` are wrong. Pre-order is unload 0, filter 1, S 2, unload 3, project 4, so S's readers are `[(1,0),(4,0)]`, not `[(1,0),(3,0)]`.
- `structural`'s old check that a child is a sink ("whose output has already crossed to the host") is gone. A structure pass, `sinks_are_roots`, runs before the walk and refuses with the plan's message, naming the sink. The old test `a_sink_below_the_root_is_refused` became `a_sink_below_a_root_is_refused`, over one root and over two.
- `validate::validate` is gone. `plan::validate(root)` is `validate_roots(&[root])`, and `check_canonical_form(root)` is `check_canonical_form_of_roots(&[root])`. Order in `validate_roots`: every root a sink, then `sinks_are_roots`, then the limit rule, then the walk. For one root this is the old order. The limit rule now runs over `PlanDag` in pre-order: a limit is refused when every reader is a sink. On a tree that is the old one-parent test.
- The walk-order helper is `validate::walk_order`. `walk` checks nodes in that order, so the test reads what validation does.
- `share_sources` leaves a load that appears only once unshared and rebuilt. The count is the number of keys seen at least twice.
- Three tests the plan does not list: `every_root_is_a_sink`, `only_a_shared_source_has_a_sharing_key`, and `equal_loads_are_shared_and_others_are_not` (the fixture's own test).
- `#[allow(dead_code)]` with a reason on `SharedSource::new`, `region_of_node` and `region_of_edge`. Nothing in production builds a DAG or asks for a region until Task 3 (the index) and query-fuser. `PlanDag.roots` has a production reader, validation's root check, so it needs no allow.
- architecture.md "Traits" gains "Only a source is shared". The plan lists no doc for Task 2, but the commit must keep code and wiki in agreement. build-test.md: `--lib` 997, cpu 3456, and a new row, `plan::dag::tests` 8. The `plan::validate::tests` row said 43, but HEAD had 35 cases (drift). It now says 37, the count `--list` gives.
- Mutation check: `all`→`any` in `decide` and dropping the `max` in `longest_paths` turn 3 dag tests red.

## Task 3 notes

- Dispatched 2026-11-04, CPU only, local. Task 3 has no device step.
- The plan's line references in `executor/mod.rs` are stale. `JoinShape`…`PlanIndex` sit at :1086–1156, not :929–997.
- `index::build` takes `&[&dyn GpuNode]` and reads every number off one `PlanDag`. That covers pre-order, children, readers, post-order and heights. Lanes are set in `in_post_order()`, so a sink takes its input's lanes. `regions[n]` is `region_of_node(n)`, and a join's `probe` is `region_of_edge(join, PROBE_CHILD)`. The old `walk` and `subtree_ranges` are gone. Building every region costs O(n² log n) per index; the corpus plans are small.
- Two old index tests were renamed because their fields went. `nodes_are_numbered_pre_order_and_each_knows_its_parent` became `…_its_readers`. `a_subtree_is_a_contiguous_range_of_the_numbering` became `a_region_on_a_tree_is_a_contiguous_range_of_the_numbering`. Their assertions are the same numbers, written as readers and as region lists.
- `a_tree_indexes_exactly_as_before` compares against a copy of the old recursive walk, over `every_kind()` plus the index's own `plan()`. It checks heights, lanes, post-order, `slot_base`, regions, probe regions, and `child_readers` all 0.
- The scheduler tests' `shape(readers, lanes, joins)` works regions out from their definition: the nodes no root reaches once the node or the edge is cut. Heights come from a memoized longest path. None of this goes through `dag.rs`'s `decide`, so the oracle stands on its own. `tree(parents, …)` turns the old parent lists into readers. `joins` now lists join nodes, and the probe is always slot 1.
- `random_shape` grows one or two trees in pre-order. Each leaf then has a 1-in-3 chance of an extra reader edge from a node that already has children. A leaf gains no child, so no edge makes a cycle. A node may read one leaf twice. Joins are drawn among the nodes left with exactly two children. `assert_regions_are_dominated` checks three things: every reader is strictly lower, every region member other than its own node is read only from inside, and a probe member is read only from inside or through the held edge.
- One test the plan does not list: `a_satisfied_root_holds_only_its_own_region`. `satisfy` holds `regions[node]`, so a second root's reader of a shared source stays runnable. Task 6 adds reader retirement on top.
- Mutation check: making `satisfy` hold the span `regions[n][0]..=last` (a contiguous range) turns `the_incremental_schedule_picks_what_a_full_rescan_would` red. Trees alone would not catch it.
- `step` refreshes the parent of each reader of the node that ran, not one `parent`. No test isolates this. A tree has one reader per node, so the 168 old driver tests cover it unchanged, and DAG runs arrive in Task 5.
- `#[allow(dead_code)]` sits only on `IndexedNode.child_readers`, which Task 5's cursors read. `PlanShape.readers` and `roots` need none, because the derived `PartialEq` reads them; building without the allows showed only `child_readers`. The allows on `region_of_node` and `region_of_edge` went, since the index calls them now.
- architecture.md "The scheduling rule" now says: height is the longest path; order is `PlanDag`'s pre-order; holds are regions (`PlanShape.regions`, `JoinShape.probe`); a step re-checks the parent of each reader. "It cannot deadlock — plans are trees" became "The driver runs one tree, and there it cannot deadlock". Task 5 replaces that with the argument.
- Counts after Task 3: `--lib` 1003 (1001 passed + 2 ignored), `executor::driver::` 174 (168 before), Driver internals 49 (index 4→8, scheduler 15→17). build-test.md: cpu 3462, `--lib` 1003, Driver internals 49.

## Task 4 notes

- **The property tests' simulation half waits for Task 5.** The driver runs one root, with one queue per node, until Task 5. So no random DAG with several roots can run yet. `no_valid_dag_is_refused` landed with its validation half only: 500 seeded DAGs (2–4 roots, 1–3 shared sources, 0–3 joins, buffers on random edges) go through `buffer_unbuffered_holds`, and `validate_dag` must then accept each. It lives in `executor/driver/tests/random_dags.rs`, so Task 5 can add the run without moving it. `every_dag_validate_dag_accepts_finishes`, and the run in `no_valid_dag_is_refused`, are Task 5's to add once `run_roots` exists.
- The 3b rule is `holds_are_buffered`, the last step of `validate_roots` (after the node walk). `validate_dag` is `validate_roots`, then the wait-for graph. `unbuffered_entry` is private to `validate.rs`, since Task 7's `validate_cap` lives in the same file. `validate_dag(_, Some(cap))` returns `PlanError::Unsupported` until Task 7.
- `PlanError::HoldCycle` carries `what: String` beside `holds`, as `UnbufferedHold` does: Display needs the names, and a `PlanError` holds no DAG.
- The constructor is the free `plan::new_hold_buffer` (it delegates to `hold_buffer::over`). There is no `GpuHoldBuffer::new`; query-fuser's plan names `new_hold_buffer`.
- Wait-for graph: "a join waits on its build region" is read as every node its build child reads, through buffers too. It is not `region_of_edge(J, 0)`, which would miss a shared source feeding the build. The edge is J → H′ when one of those nodes is in H′'s probe region (which stops at buffers). A join's own probe region never holds a node its build reads, so there is no self-loop. Mutation check: making a join wait on its own region turns `no_valid_dag_is_refused` red with a false `HoldCycle`.
- A build hold never masks a buffer; a limit hold still does. `region_of_node` does not stop at buffers (a limit's hold never lifts), so a satisfied limit takes a buffer below it out of the schedule. Task 6 then releases its store.
- `Scheduler::lane_left_build` now returns `true` when the hold lifts. The driver then re-checks every buffer in that join's probe region (`hold_lifted`). This is needed because a buffer's readiness reads the hold, and no step touches the buffer when the hold lifts. Without it the four-lane, order and accounting tests stall.
- New `CallKind::Keep`, for a held buffer taking a batch. An unheld emission is a `Forward` and the lane's end a `ForwardDone`, as at a forwarder. The output hook is not called at a buffer, as at a forwarder: the batch is not its own output.
- A kept batch is counted as consumed by the buffer at intake. So a store released at the end of a run adds nothing to `abandoned`, and `assert_conserved` holds edge by edge. Task 6 says a satisfied region's buffer "adds those rows to abandoned". That would break conservation on the buffer's input edge unless intake stops counting as consumption. Task 6 must decide which.
- Driver test shape: the plan's `unload(join(B, buffer(filter(P))))` puts B lower than P. B then drains completely before P runs, so nothing runs during the build. The tests use `join(coalesce_all(project(filter(b))), buffer(filter(p)))` instead: the build path is deeper, so p is the lower node and runs first. Every build side goes through a coalesce, because a join's build must be one batch per lane. "Failing at the join's build" is `FailAt::MarkDone`, the coalesce's done call that completes the build. The buffer keeps all three batches by then.
- `a_held_buffer_with_nothing_offered_is_not_ready` is a driver test, not a scheduler one, because readiness is the driver's. The scheduler got `a_held_buffer_stays_runnable_for_intake`, plus two tests the plan does not list: `a_limit_still_holds_a_buffer` and `a_lifted_hold_says_so_once`. Two dag tests the plan does not list either: `a_node_also_read_around_a_buffer_is_still_held`, and `a_join_waits_on_the_hold_its_build_side_sits_in` in validate. `buffering_each_refused_edge_makes_a_plan_valid` is the fixture's own test.
- Validation fixtures: `plans::join` declares one column where an unprojected inner join emits two, and the plan's `join(S, S)` and `join(S1, S2)` put a source straight onto a build side. Full validation refuses both before the buffer rule runs; the driver only checks the canonical form, so the driver tests never saw it. I added `plans::projected_join` (projection `[0]`) and put every build side under `coalesce_all`. The indices in the plan's expectations shift. Shared source in a hold: `{ hold: 1, reader: 4, slot: 0, producer: 5 }`. `join(coalesce(S), S)`: the edge `(1, 1)` → 3. The crossed builds: `(1, 1)` → 4.
- `buffer_unbuffered_holds` rebuilds the whole DAG for each refused edge. A shared source's key maps to one new `SharedSource`, built from its first rebuild, so the sharing survives the rebuild. `rebuild` gained the buffer's arm.
- Estimator: `held_by_accumulator(buffer) = bytes(child)`. Two changes beyond that: `amplification` walks through a buffer, and `batch_bytes` passes the child's batch through. A buffer is not where a batch ends, and treating it as an accumulator there would size its output as one batch per lane.
- Twenty node kinds: `plan/mod.rs`'s head, `layout_injection`'s count, `rebuild`'s arm and two fixtures, `cpu_backend/tests/backend.rs` (`NODE_KINDS` 20 and a buffer in its list), `translator/tests.rs`'s `name_of`, and `gpu_tests/coverage.rs`'s `EXCLUDED`, which gains `GpuHoldBuffer` (no executor). `wire/gpu_tests/mod.rs`'s walk passes a buffer's lanes through unchanged. The two rewrap matches in `wire/writer.rs` and `wire/join.rs` pass the new variants through.
- `partitioned.rs`: `answer` and `report` moved verbatim to `partitioned/answers.rs` (only `pub(crate)` added), then the buffer arm grew: 999 → 966 lines. The arm itself is `partitioned/hold_buffer.rs`.
- rustfmt was run on the touched leaves, except three files whose diffs at HEAD are not on my lines: `single_partition.rs` (three `.made(...)` chains, through its `tests` child), `gpu_backend/backend.rs` and `nullability.rs`. My edits there are formatted.
- Mutation checks, each turning tests red. Regions never stopping at a buffer: 5 red. Build holds masking a buffer: 4 red (the scheduler's differential test stayed green; the targeted tests caught it). No re-check after a lift: 5 red. No buffer exemption in the rule: 3 red, the property test among them.
- The plan's line references are stale again (`plan/mod.rs`, `executor/mod.rs`).
- Counts after Task 4: `--lib` 1028 (1026 passed + 2 ignored), +25: `executor::driver::tests` 133 (+8), Driver internals 52 (+3), `plan::validate::tests` 46 (+9), `plan::dag::tests` 10 (+2), memory estimation 15, wire 32, plan text 16. `test_corpus_goldens` 53, `plan_goldens` 25, `test_cpu_corpus` 2403, `testdata/` untouched, `test_source_formatting` 32. The `gpu` test build compiles locally against rapids-cuda-12.2 with no warning; no device run, since Task 4 has no device step. build-test.md: cpu 3487, `--lib` 1028, and the rows above.

## Task 5 notes

- **`run` does not go through `validate_dag`.** The plan has `run`/`run_with_hook` delegate to `run_roots`, which validates. They delegate to the same driver path (`Driver::new` is `for_roots(&[root])`, and `run` is `run_fused` with `answers[0]` moved into `batches`), but keep today's canonical-form check only. Three reasons: `plan/mod.rs` documents that the driver asks only for the canonical form; the hook tests run `merge_sorted(merge(...))` through `executor::run_with_hook`, which full validation refuses; and a tree never needs a buffer. `run_roots` alone calls `validate_dag`. `run_roots_of_one_root_matches_run` shows the two give the same trace.
- `Driver::for_roots` takes no `cap`. `run_roots` takes one and hands it to `validate_dag`, which refuses `Some` until Task 7, so a cap is never silently ignored. Task 7 adds it to `for_roots`.
- `Charge` is the plan's `Rc<()>` holder count. To keep a failed call from needing its input's charge, every lane call now releases its input right after the call returns, success or failure, as the emitter already did; `failed` and `hold_all` release nothing. On a tree the order of holds and releases is unchanged: the input is released before the outputs are held, as before.
- `OutQueue` keeps one holder per entry, the queue's. A non-last reader's share is a second holder (a clone of the charge); the last reader takes the original and the queue's holder. Readers take in order, so the last taker of an entry always takes it from the front. `retire` is implemented and tested, `#[allow(dead_code)]` until Task 6's driver calls it. `drain` gives each entry once with how many live readers never took it; `release_in_flight` adds `rows × untaken` to `abandoned`.
- `assert_conserved` now checks per node over its readers: Σ consumed + abandoned == readers × emitted. On a tree that is the old per-edge check.
- **The property tests.** A reader's cursor never blocks its producer, so an unbuffered hold does not deadlock: the queue in front of it grows. The plan's "stuck ⇒ `validate_dag` refused" arm therefore cannot fire on this driver. Both tests also assert what rule 3b actually buys: every DAG `validate_dag` accepts (and every DAG once buffered) finishes, reconciles, and never queues more than one batch per lane (`peak_queued[n] ≤ lanes_of[n]`). `every_dag_validate_dag_accepts_finishes` also requires more than 100 accepted DAGs, and more than 50 refused DAGs whose queues grew, so neither arm is vacuous. Mutation check: validation accepting every unbuffered hold turns both red ("a queue grew under a valid plan", seed 0).
- Plan tests whose shape or number changed:
  - `a_shared_batch_is_held_once_and_released_once`: the plan's `holds == 9` is one root's count (three loads, three filters, three unloads). Fused, it is 9 + 9 − 3 = 15, which the test asserts beside that.
  - `a_four_lane_shared_source_reaches_every_reader_on_every_lane`: the second reader is a `merge`, not `emit(…, 4)`. An emitter reads only its input's lane 0, so it cannot read every lane.
  - `a_failed_call_with_a_shared_batch_queued_still_reconciles`: A = `unload(coalesce(coalesce(S)))`, so B's filter is the lower reader. It takes a share and fails with the original still queued for A.
  - `a_buffered_share_keeps_its_charge_after_the_last_reader_takes`: the build path is deeper (`coalesce(project(filter(b)))`), so S runs during the build, as in Task 4's tests.
  - `an_unbuffered_reader_inside_a_hold_is_refused_by_run_roots` uses `projected_join` and a coalesced build, since `run_roots` validates fully. "No step taken" is checked as no executor built.
  - `a_batch_is_dropped_after_its_last_reader` is in `tests/dag.rs`, not `partitioned/tests.rs`, which holds the test helpers' `impl`.
- **Deferred to Task 6:** `a_limit_retiring_a_reader_keeps_the_charge_a_buffer_holds`. It needs the driver to retire a satisfied root's reader, which is Task 6's.
- `check_canonical_form(root)` lost its last production caller (the driver checks over roots) and is gone. Its callers in `plan/validate/tests.rs` now use `check_canonical_form_of_roots(&[root])`.
- `partitioned.rs` went over 1000 lines (1043). Its queue section moved to `partitioned/queues.rs` (only `pub(crate)` added), so the file is now 920. rustfmt over `single_partition.rs` also reflowed its two `.made(...)` chains, which were not rustfmt-clean at HEAD.
- `RunReport.abandoned`'s doc now says it sums over readers; `batches`' doc says a `FusedRun`'s report leaves it empty. architecture.md "Queues need no cap" (one cursor per reader, a hold answered by a buffer, never a growing queue), the no-deadlock argument as a paragraph of its own, `run_roots`, and Memory accounting (one charge per batch, refcounted over its holders).
- Mutation checks, each turning tests red. A charge released at every holder: 10 red. Readers taking the original and removing it, so they steal from one another: 12 red. Validation accepting every unbuffered hold: both property tests red.
- Counts after Task 5: `--lib` 1048 (1046 passed + 2 ignored), +22: `executor::driver::tests` 150 (dag 16, random_dags +1), Driver internals 57 (queue 4, accounting +1). `test_corpus_goldens` 53, `test_cost_model` 3, `test_cpu_corpus` 2451, `plan_goldens` 25, `test_source_formatting` 32, `testdata/` untouched. build-test.md: cpu 3555, `--lib` 1048, and the two rows.
- The `--features gpu --no-run` build against rapids-cuda-12.2 compiles with no warning. No device run; Task 5 has no device step. Task 4's carry-over (the random-DAG runs) is done.

## Task 6 notes

- Dispatched 2026-11-05, CPU only, local. Task 6 has no device step.
- **Decision (Task 4/5 carry-over): a buffer's kept batches are not abandoned rows.** A buffer's intake is counted as consumed, so its input edge already balances. Its store, released at a satisfied limit or at the end, is like rows a filter dropped. Counting it again as abandoned would break `Σ consumed + abandoned == readers × emitted` on the edge into the buffer. `RunReport.abandoned`'s doc says so. The plan's "adds those rows to abandoned" is not done.
- `Scheduler::satisfy` returns `Vec<(producer, reader ordinal)>`. It works off a pending list: holding each newly satisfied node's region, retiring each reader inside that region of a producer outside it, and queueing a producer once every reader of it has retired. The scheduler keeps `readers` and a `retired` flag per reader. Only a source is shared, so in practice a retired producer is a source. The scheduler's differential test's `Naive` now computes the same closure from its definition (`satisfied_in_turn`): a node is satisfied in turn when every reader sits in a satisfied region that leaves the node outside. Mutation check: dropping the propagation turns three red, the differential test among them.
- **`OutQueue` now owns the abandoned count** (`abandoned()`), and the driver's `abandoned` field is gone; the report reads it off the queues. A retire adds the rows the reader had yet to take. A push adds `rows × retired readers`. A drain adds `rows × live readers that never took it`. `drain` returns the entries only. Mutation check: not counting a push after a retire turns five red, the queue test among them.
- `settle_limit` moved to `partitioned/early_exit.rs` (partitioned.rs 920 → 883). On satisfaction it retires each returned reader on every lane of its producer and releases what `retire` hands back. Every node satisfied in this call (the node and any producer satisfied in turn) is marked done, and each buffer in its region gives back its store and is marked done. No re-check is needed: everything touched is held by the limit.
- Test helpers `shared`, `read`, `buffer`, `refs` moved into `driver/tests/mod.rs` from `dag.rs`, `hold_buffer.rs` and `random_dags.rs`, since `limit.rs` needs them too. `partitioned/tests.rs` gained `is_satisfied(node)`.
- Plan tests whose shape changed:
  - `a_retired_reader_releases_what_it_left_queued`: A = `unload(filter(limit(merge(S))))`, B = `unload(S)`, S at 2 lanes. A merge forwards one batch per visit, so the limit is satisfied by lane 0's batch with lane 1's still queued for the merge alone (B took a share before). An unload takes every lane in one step, so an unload-limited root never leaves anything queued.
  - `a_satisfied_limit_releases_what_a_buffer_below_it_keeps` uses Task 4's `join(coalesce_all(project(filter(b))), buffer(filter(p)))` under `unload_limited(.., 0, Some(1))`. p drains into the buffer before b runs, so "P stops" cannot happen in this shape: p has already finished. The test checks the store is empty right after the satisfying step, and the buffer emitted one batch.
  - `a_limit_retiring_a_reader_keeps_the_charge_a_buffer_holds` (moved from Task 5): B = `unload_limited(filter(S), 0, Some(1))` is satisfied before the buffer has taken S's first batch. It also checks S's queue never held more than one, which fails without retirement (2).
  - The tests that need it check `peak_queued[S] <= 1`. Before retirement, a retired root's cursor kept every later entry queued.
- `RunReport.satisfied` now lists a source satisfied in turn (`the_source_stops_once_every_reader_is_satisfied`: `[0, 1, 2]`). A single tree never satisfies one, so the goldens' `early_exit=` is unchanged.
- architecture.md "Early exit at a limit": the hold is the region, through buffers; retirement; a source stops when every reader has retired.
- Counts after Task 6: `--lib` 1067 (1065 passed + 2 ignored), +9: `executor::driver::tests` 156 (limit +5, dag +1), Driver internals 60 (scheduler +2, queue +1). build-test.md: cpu 3582, `--lib` 1067, grand total 7962 (Rust 6132).

## Task 7 notes

- Dispatched 2026-11-05, CPU only, local. Task 7 has no device step.
- `PlanShape.cap_regions` is per root ordinal: `held_region_of_node(root)`, which lost its `dead_code` allow. `Scheduler::with_cap(shape, cap)` holds every root's cap region with a third counter, `cap_holds`, then admits roots in order while fewer than `cap` admitted roots are unfinished. `root_finished(root)` is idempotent and returns the nodes whose cap hold lifted. A buffer masked only by a limit stays runnable under the cap's hold, as under a join's, so it takes what a shared source offers.
- `Scheduler::new` had no production caller left (`for_roots` builds `with_cap`), so it moved to an inherent `impl` in `scheduler/tests.rs`, where every caller is.
- The driver calls `report_root(node)` after `settle_limit` in `step` and in `seed`: a root whose lanes are all done, or that was satisfied (which marks it done), reports once. The buffers among the lifted nodes are re-checked, as `hold_lifted` does for a join (both go through `recheck_buffers`). Without the re-check the random-DAG tests stall (mutation check).
- `validate_cap(roots, cap)` is the plan's interface (query-fuser calls it). `validate_dag` reaches the same rule, `capped_roots_are_buffered`, on its own `PlanDag`. The refusal is `UnbufferedHold { hold: root node, .. }`, its message naming the root. `PlanError::Unsupported` for a cap is gone.
- Wait-for graph: a hold vertex is a node index, a join's or, for the cap, the waiting root's (an unload, so never a join's index). A waiting root r waits on each hold whose region holds a node reached from a root of lower ordinal. A join inside r's cap region gets no edge to r's cap. In practice a join only ever reaches r's region from inside r, since a root's region is reached through that root alone, so the exclusion is the only join→cap edge there would be. `wait_for_edges` takes `cap`.
- `plan/validate/tests.rs` reached 999 lines with the three cap tests. Its several-roots section ("holds and buffers over several roots", unchanged but for `super::` paths becoming `crate::plan::validate::`) moved to `plan/validate/tests/holds.rs` (284 lines), which is where the cap tests sit. tests.rs is 720.
- `cap.rs`'s "no step of root 1's region precedes root 0's last" excludes `Keep` events: a waiting root's buffer takes batches while held, which is the point. `a_capped_root_with_a_join_is_accepted_and_runs` uses `projected_join` (full validation) and compares with each root alone as a tree without the buffer.
- The property tests draw `cap = seed % roots` (none when 0). Both still clear their thresholds.
- Mutation checks, each turning tests red. A cap that never binds: 5. Roots never reported finished: 7, both property tests among them. No buffer re-check on admission: both property tests. A join waiting on its own root's cap: `a_capped_root_never_waits_on_its_own_region`. The cap's buffer rule off: 3.
- The stale `#[allow(dead_code)]` on `IndexedNode.child_readers` ("until then every one is 0") went: Task 5's queues read it.
- Counts after Task 7: `--lib` 1079 (1077 passed + 2 ignored), +12: `executor::driver::tests` 162 (cap +6), Driver internals 63 (scheduler +3), Plan types 57 (`plan::validate::tests` 49). build-test.md: cpu 3594, `--lib` 1079, grand total 7974 (Rust 6144).

## Task 8 notes

- Dispatched 2026-11-05, CPU only, local. Task 8 has no device step.
- `Tree::of(roots)` is built from `PlanDag`: nodes in its post-order, children mapped to post-order, and `parents` (every reader) in place of the single `parent`. On a tree `PlanDag`'s post-order is the old recursive one, so every `--- memory ---` section is unchanged. `estimate_roots(roots, budget)` lives in `memory_estimation.rs`; `planner::estimate(root)` and `planner::estimate_roots` delegate to it. The tests import `crate::planner::estimate`.
- `amplification` walks every reader's path from the source up to the accumulator that ends it, taking the maximum. It was one parent chain. One test the plan does not list covers it: `a_shared_source_is_amplified_by_its_widest_reader` (`filter(S)` beside `merge(emit(S, 4))`: 4.0, where the first reader alone gives 1.0). Mutation check: following only a source's first reader turns it red. The plan's three tests do not reach amplification.
- `plans::shared(table, lanes)` is now in `src/tests/plans.rs`, and `driver/tests/mod.rs`'s copy (Task 6) is gone. `plan/dag/tests.rs` and `plan/validate/tests/holds.rs` keep their own one-lane `shared`/`shared_source`, untouched.
- Counts after Task 8: `--lib` 1083 (1081 passed + 2 ignored), +4: Memory estimation 19. build-test.md: cpu 3598, `--lib` 1083, grand total 7978 (Rust 6148).

## Task 9 notes

- Dispatched 2026-11-05 (continued after Task 8's commit), CPU only, local. Task 9 has no device step.
- `plan_text/dag_walk.rs::render(dag, first)` is the one walk: depth first over the roots in order, headers and `@k` back-references written there, each first visit's lines by the renderer's closure, handed the node's `PlanDag` index, its indent and its `@k ` prefix. A continuation line (`in_rows=`, `time_us=`) is indented from the indent, not from the prefix.
- All four renderers go through it, and each one-root form delegates with `&[root]`: `render_plan`/`render_plans`, `render_plan_memory`/`render_plans_memory`, `render_run`/`render_runs`, `render_timings`/`render_timings_of_roots`. The old recursive walks are gone. `run_text` and `bench_text` walk `PlanDag` rather than `PlanIndex`: the driver's index is numbered by the same `PlanDag::of`, so the report's indices are the walk's. `timed_load_us` still builds a `PlanIndex` over one root, unchanged. The memory renderer reads `post_order` off the walk's nodes, which is what the model is indexed by since Task 8.
- One test the plan does not list: `the_memory_section_prints_a_shared_node_once`. `a_run_renders_a_shared_node_once` lives in `driver/tests/render.rs`, beside the run renderer's other tests, since it needs the mock.
- I wrote these tests before reporting Task 8's checkpoint, set them aside as a patch, and reapplied them after Task 8's commit. They failed to compile (no `render_plans`, `render_plans_memory` or `render_runs`) before the implementation. Mutation check: never labelling a shared node turns all four red.
- Counts after Task 9: `--lib` 1088 (1086 passed + 2 ignored), +5: Plan text 20, `executor::driver::tests` 163. build-test.md: cpu 3603, `--lib` 1088, grand total 7983 (Rust 6153).

## Task 10 notes

- Dispatched 2026-11-05, CPU and compile only, local. Task 10 has no device step; Task 11 has the L40S run.
- `gpu_plan.fbs`: `GpuPlan.roots: [PlanNode]` appended after `root`. One root writes `root` alone, so every single-query buffer is byte-identical (`one_root_writes_no_roots_field` compares `attach_recipes` with `attach_recipes_for_roots(&[root])`; `recipe-payloads.txt` and every plan golden unchanged).
- Writer: `reuse(offset)` pushes a written node back for the next parent to take, spending no seq; `last()` is what a node left after `reduce`. **Deviation:** the plan's `finish(self, roots)` became `end_root()` plus `finish(self)`. A finished root leaves the pool at once, so a later root's node can never take it as a child when `take` would otherwise stub a slot. `finish` ends the root still in the pool, so the existing tests that write one node and call `finish` are unchanged.
- `attach_recipes_for_roots` walks `PlanDag` (`attach.rs`'s `Walk`): roots in order, children in order, a node with two or more readers written at its first visit and its offset kept (`written`). `attach_recipes(root)` delegates. Recipes are pushed in first-visit post-order, which is `PlanDag`'s post-order.
- Reader: `read::indexed(plan)` lists the nodes in seq order, visiting a node once by its buffer location (`_tab.loc()`), roots in order; `node_at` reads it, `check_seq_kinds` builds it once, `depth` takes the deepest root. A plan setting both `root` and `roots` is refused on both sides with `"GpuPlan sets both root and roots"`.
- `recipes.rs`'s node count is `PlanDag::of(&[root]).nodes.len()`. Its renderer stays one-root; nothing asked for a fused recipes section.
- C++: `NodeSession::Impl::indexed` (an `unordered_set`) makes `index_post_order` skip a node already indexed; the constructor indexes `roots()` in order when set. The three CPU tests build buffers by hand. `IndexesSeveralRootsInOrder` reads the order off `execute_scan_rowgroups`'s refusal of a non-scan, which names the kind at a seq before touching the device: roots `[filter(scan), project(scan)]` put a filter at 1 and a project at 3.
- `test_support::Session::open_roots(roots, join_scratch, what)`; `open` delegates. No caller yet: Task 11's device cases.
- The FFI tier (`--lib -- ffi_tests::`, default features) passes, 5 of 5, run as the handoff below says.
- The new Rust tests are in `wire/tests/roots.rs`, since `wire/tests.rs` is 992 lines. The schema change alone broke the lib's build (`GpuPlanArgs` gained a field), so the Rust tests were red as a compile failure. Three mutation checks each turned tests red: a shared node rewritten at every visit (2), the reader revisiting a node (3), one root written as `roots` (1). The three C++ tests failed on behaviour before the C++ change (count 5 for 4, "no root node", no refusal).
- Counts after Task 10: `--lib` 1093 (1091 passed + 2 ignored), +5: Recipes per join type 37. `peacock_cpu_tests` 67 (+3). build-test.md: cpu 3608, `--lib` 1093, C++ CPU/FFI 67, grand total 7991 (Rust 6158, C++ 286).

## Handoff to Task 11 (2026-11-05)

Tasks 6–10 were done by one developer, whose context ran long; Task 11 goes to a fresh one.

- **What Task 11 consumes is in place:** `share_sources`, `new_hold_buffer`, `buffer_unbuffered_holds(roots, cap)` (`src/tests/share.rs`), `run_roots(roots, ctx, budget, cap)`, `estimate_roots`, `render_plans`/`render_runs`, `attach_recipes_for_roots`, and `Session::open_roots` (`test_support/gpu_session.rs`, no caller yet). `plans::shared(table, lanes)` in `src/tests/plans.rs` builds a shared source from the mock fixtures.
- **Running the FFI and gpu binaries locally:** `scripts/cargo-cudf.sh test …` builds them, but running one fails with exit 127 (`libcudf.so` not found) unless `LD_LIBRARY_PATH` holds `~/data/miniforge3/envs/rapids-cuda-12.2/lib` and the newest `target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-*/out/lib` (where `libpeacock_gpu.so` is). Run the test binary directly with that path. The C++ build: `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build`, then `ctest --test-dir cpp/build -L cpu`.
- **Counts at the end of Task 10:** `--lib` 1093 (1091 + 2 ignored), `test_corpus_goldens` 61, `test_cost_model` 3, `test_cpu_corpus` 2451, `plan_goldens` 25, `peacock_cpu_tests` 67, `peacock_plan_tests` 136, gpu `gpu_tests::` 743, `ffi_tests::` 5. build-test.md's rows were checked against `--list` at each task, and the cpu rows sum to the header.
- **Device step:** nebius-gpu's disk had 4.9 GB free on 2026-11-05. The coordinator's rule: check `df -h /` first; under 10 GB, stop and report, unless deleting this worktree's own `~/peacockdb-peacockdb-beta/target-cudf-*` on the VM frees enough, then rebuild cold. The C++ side changed in Tasks 1 and 10 (`node_session.cpp`, `gpu_plan.fbs`), so the device build is not incremental from Task 1's.
- q95's shape: the plan describes it from `testdata/goldens/tpcds.sf1/tp1-single-mini.cpu.txt` (`== q95`); check it still reads `web_sales` four times before relying on "1 shared, 4 buffers". If the count differs, record what `buffer_unbuffered_holds` reports and the edges.

## Task 11 notes

- Dispatched 2026-11-05 (fresh developer, after the handoff), all steps, the L40S run included.
- No production code changed. The tests are `tests/end_to_end/dag.rs` (8 CPU cases) and `tests/gpu_tests/dag_cases.rs` (5 device cases). Each mode is its own case, named `…_at_tp1_single` / `…_at_tp4_sized`.
- The four tpch queries and `planned(dataset, queries, mode)` live in `tests/share.rs`, beside `share_sources`, since both tiers use them. Every query is planned in one session, because the cpu backend runs a fused plan against one task context.
- **Deviation: R2 is `SELECT l_quantity, count(l_orderkey) FROM lineitem GROUP BY l_quantity`**, not the plan's `l_orderkey % 7 … GROUP BY k`. The device refuses a computed group key ([#264](../tickets/complete-coverage.md#t264), `CudfAggregate: only ColumnRef group exprs supported`), so the plan's R2 failed on the device *alone*, before any fusing. That was the first L40S run: `peacockdb_core_gpu_lib` 746 passed, 2 failed, both two-root cases. The new R2 reads the same lineitem columns, so it still shares with R1.
- **`Session` is private to `test_support`**, so `tests::gpu_tests` cannot call `Session::open_roots` directly. Two `pub(crate)` delegates were added to `test_support/mod.rs`, gated like `schema_at`: `answer_on_the_device(tree, join_scratch, what)` (`Session::open`, `run`) and `answers_of_roots_on_the_device(roots, join_scratch, what)` (`Session::open_roots`, `run_roots`). Their bodies are in `gpu_session.rs`, and each asserts `holds == releases` and `in_flight_bytes == 0`. The names avoid the private module `device_answer`.
- Answers are compared sorted (`batches_to_sorted_str`), as the plan says. The device side compares each root's fused answer with its query's alone, each alone run in its own session.
- **Buffers the fixture inserts**: R1+R2 0 at both modes; R1+R3+R4 1 at both modes (lineitem on R3's probe edge, which R1 reads from outside the hold). Orders feeds R3's build, which is not held, so R4's read needs none. q95: 1 shared source and 4 buffers, as the plan predicted from the golden. The shape still reads `web_sales` with `{ws_warehouse_sk, ws_order_number}` four times. The tests pin these counts.
- `a_fused_run_holds_each_shared_batch_once`: fused `holds` == Σ alone `holds` − Σ over shared sources (readers − 1) × the batches it emitted (read off `report.emitted` at the `PlanDag` index).
- Mutation checks (production, then restored). `CpuBatch::share` returning the first half of the rows turned 6 of the 8 red. q95 stayed green under that, because its 1999 date window sits in the first half of `web_sales`. With the second half instead, `q95_fused_with_buffers_answers_as_q95_alone` was red (order count 0 against 68).
- Counts: `--lib` 1101 (1099 passed + 2 ignored), +8: End to end 52. gpu `gpu_tests::` 748, +5: new row "Fused plans on a device" 5. build-test.md: cpu 3616, gpu 2417, grand total 8004 (Rust 6171).
- **L40S run (Step 4), second run after the R2 change: `ALL rc=0`.** `peacock_cpu_tests` 67, `peacock_gpu_tests` 5, `peacock_join_session_tests` 56, `peacock_plan_tests` 136, `peacockdb_core_gpu_lib` 748 (the 5 `dag_cases` OK), `test_gpu_corpus` 1009, `peacock_gpu_benchmarks` 9 (`--skip bench_`), `test_node_timing` 1. These equal build-test.md's rows. Against Task 1's run: plan +1 and cpu +3 (Tasks 1 and 10, after rebase 2), corpus +20 (rebase 2), gpu lib +5 (this task). Disk before the build: 22 GB free on `/`, so nothing was deleted. The C++ rebuilt (`node_session.cpp`, `libpeacock_gpu.so`), so the device ran Task 10's numbering.

## Review round 1 (2026-11-05)

- **Important, `finish` and `seed`.** Two regression tests went red first.
  - `cap::a_limit_satisfied_at_seed_still_lets_the_next_root_in` is the reviewer's case. It answered root 1 with `[0]` (one empty batch) and `Ok`.
  - `limit::a_satisfied_limit_does_not_excuse_a_root_that_never_ended` calls `finish` right after `seed`. It was `Ok` with root 1 never stepped. With `seed` fixed, the driver gives no way to build a real stall beside a satisfied limit, so this one drives `finish` directly.
  - Fix in `finish`: `assert_roots_ended`, split out of `assert_drained` in `partitioned/queues.rs`, now runs on both branches. Only the stranded-queue check is skipped on an early exit.
  - Fix in `seed`: it settles every limit first, then refreshes and reports every node.
  - The `finish` fix alone also turned the existing `limit::a_mid_plan_limit_of_no_rows_is_satisfied_before_any_pull_whatever_its_skip` red. That is the same stale-`seed` bug on a single tree. Its root never ended, and the old `finish` let it through, since the empty answer happens to be right for `LIMIT 0`. With the `seed` fix it is green.
- **(a) A budget trip before a call.** New accountant methods `begin_call_with(slot, executor, Option<Held<T>>)` and `begin_call_taking(.., Held<T>)` release the taken input when the pre-check trips. All seven sites that take an input use them: five in `single_partition.rs` (Exec, Unload, Accumulate, SetBuild, Probe), the emitter and the cross-lane accumulator. The three no-input sites keep `begin_call`. `budget::a_call_refused_before_it_runs_releases_its_input` was red first (1 held, 0 released). It trips the filter's call with `modelling(1)` and a budget of 1.5 batches. `run_fused`'s comment, "held and released stay equal on every path out of here", is now true.
- **(b)** `driver/mod.rs`'s `run_roots_with_hook` is one expression. The empty-roots refusal and `validate_dag` moved into `partitioned::run_roots_with_hook`.
- **(c)** `OutQueue::high_water()` is an inherent `impl` in `queue/tests.rs`. The field keeps `#[cfg_attr(not(test), allow(dead_code))]` with the reason at the site: only tests read it.
- **(d)** "rule 3b" now names the rule, "the only unbounded storage is a node you can see", in `architecture.md` and `validate.rs`.
- **(e)** The two over-long comments, `partitioned.rs` `refresh` and `executor/mod.rs` `satisfied`, are reflowed.
- **(f)** `architecture.md` changes:
  - "the old argument" became "the argument is shorter".
  - "a single query's text is unchanged" became "prints as a plain tree".
  - The ABI lead now says three symbols carry per-call facts and the fourth gives a batch a second owner.
  - The `GpuHoldBuffer` row names the cap's hold as well as a join's.
  - In `coverage.rs`, the doc names both exclusions: the forwarders and the hold buffer.
- **(g)** `validate_roots` refuses a root given twice, as `PlanError::Invalid` "`<name>: root <i> is root <j> given again — each root is its own query`" (`roots_are_distinct`, the first structural check). `validate::tests::the_same_root_given_twice_is_refused` was red first. `plan/mod.rs`'s doc and `architecture.md` "Traits" say so.
- **(h)** The four `HandleDuplicate` gtests moved, with their three helpers, to `cpp/tests/gpu/test_handle_duplicate.cpp`, which is wired into `peacock_plan_tests` in `cpp/CMakeLists.txt`. The sibling files in that binary keep their own fixtures in an anonymous namespace, with no shared header. So the new file carries its own small copies of the scan and scatter plan builders, `CApiPlan`, `TimingOn` and `allocated_by`. The file is clang-formatted. It builds locally, and `peacock_plan_tests --gtest_list_tests` lists 136 with the four `HandleDuplicate` cases. This host has no GPU, so they were not run.
- Counts after round 1: `--lib` 1105 (1103 passed + 2 ignored), +4: Drivers over a mock backend 166 (+3), Plan types 58 (+1). build-test.md: cpu 3620, grand total 8008 (Rust 6175). `test_corpus_goldens` 61, `test_cost_model` 3, `test_cpu_corpus` 2451, `plan_goldens` 25, `test_source_formatting` 32, `testdata/` untouched, gpu `gpu_tests::` 748 (`--no-run`, no warning), `ctest -L cpu` 1/1 (67), `peacock_plan_tests` lists 136. The L40S list was not rerun after the driver change.
- rustfmt follows `mod` declarations. Formatting `partitioned.rs`, `queue.rs`, `validate.rs` and the two `mod.rs` files reformatted seven files I had not touched. Those were restored from HEAD by plain file writes, and the `compressed_bytes` hunk in `plan/mod.rs` was put back.

## Completeness (2026-11-06)

- **The cap and state below a buffer (spec 5).** The cap holds a waiting root only down to its buffers: `held_region_of_node(root)` stops at a buffer. So any node between that buffer and the shared source it buffers runs while the root waits. In Task 7's terms, the cap region ends at the buffer; the nodes below it are in `region_of_node(root)`, which goes through buffers, and not in the cap region. A stateless node there is fine: a batch in, a batch out, kept by the buffer above. An accumulator would keep its state alive outside the cap, which spec 5 exists to bound.
  - Fix: `capped_roots_keep_their_state` in `validate.rs`. For each waiting root, it takes `region_of_node(root)` less its cap region. The first accumulator there is refused as the new `PlanError::UncappedAccumulator { root, node, what }`, whose message names both nodes. Both `validate_cap` and `validate_dag` run it, after `capped_roots_are_buffered`.
  - "Accumulator" is `holds_state`: the memory estimator's accumulators (`held_by_accumulator`) less the buffer, which is the cap's own answer. That is coalesce-all, merge-sorted, accumulate-and-sort, aggregate-batches, hash join, nested-loop join and cross join. The list is written twice, once in each place. If one changes, the other must follow.
  - `wire/join.rs` and `wire/writer.rs` pass the new variant through, as they pass the other hold refusals. `plan/error.rs` displays it as "invalid plan: …".
  - Tests in `plan/validate/tests/holds.rs`:
    - `an_accumulator_between_a_waiting_roots_buffer_and_its_source_is_refused` covers `unload(buffer(coalesce_all(S)))` through `validate_cap`, and `unload(buffer(join(coalesce(b), buffer(filter(S)))))` through `validate_dag`. It was red first, with the variant added and no rule.
    - `stateless_nodes_between_a_waiting_roots_buffer_and_its_source_are_accepted` covers `unload(buffer(project(filter(S))))`. It passed before and after the change: it guards against over-refusal.
  - Mutation check: with the rule off, both the refusal test and `random_dags::no_valid_dag_is_refused` go red.
  - **The property test changed.** The generator draws buffers above joins and coalesces. In a waiting root such a DAG is now refused, rightly, and no buffer answers it. `no_valid_dag_is_refused` flags those DAGs with an oracle written from the regions' definitions (`keeps_state_below_a_waiting_buffer`, matching the generator's two accumulator kinds by name). A flagged DAG must panic in `buffer_unbuffered_holds` with the `UncappedAccumulator` phrase. Every other DAG keeps the old property. At least one DAG must be flagged, so the arm cannot be empty.
- **Spec 1's "in the order that root alone would produce them" is narrowed, not met.** A shared source's height and index come from the whole DAG. So a root that merges streams, such as `unload(union(T, S))` with S shared with root 0, drains S before T fused, and T before S alone. The `FusedRun` doc in `executor/mod.rs` now says "the same rows; the same order where the root reads one source per stream". The module doc of `driver/tests/dag.rs` now says "the rows". `dag::a_fused_root_answers_its_rows_alone_though_not_always_in_their_order` asserts the sorted rows are equal and pins both orders: fused `[10, 20, 30, 1, 2, 3]`, alone `[1, 2, 3, 10, 20, 30]`. It documents the scheduler as it is, so it passed on first run. The scheduler is unchanged. The spec file is frozen, so this note stands for its signoff.
- `architecture.md` is not touched here (the coordinator was editing it).
- Counts: `--lib` 1108 (1106 passed + 2 ignored), +3: Drivers over a mock backend 167 (+1), Plan types 60 (+2). build-test.md: cpu 3623, grand total 8011 (Rust 6178).

## Golden baseline (Task 1 Step 1, parent state `f52c11b6`, local)

- `--lib`: 984 passed, 2 ignored (986 cases).
- `--test test_corpus_goldens`: **53 passed** (master's 28 plus the chain's additions). Later golden checks compare with 53 plus each task's additions.
- `--lib -- planner::tests::plan_goldens`: 25 passed.
- `git status --short testdata/`: empty.

## VM time

| VM | start | stop | hours |
|---|---|---|---|
| nebius-gpu (Task 1 Step 4) | 2026-11-04 14:24 UTC | 2026-11-04 14:57 UTC | 0.55 |
| nebius-gpu (Task 11 Step 4) | 2026-11-05 21:56 UTC | 2026-11-05 22:34 UTC | 0.63 |

Task total, nebius-gpu: 1.18 h.

## Carry-over (coordinator)

- Task 5 must finish `executor/driver/tests/random_dags.rs`: the driver-running halves of
  `no_valid_dag_is_refused` and `every_dag_validate_dag_accepts_finishes` (Task 4 landed only the
  validation half, since the driver had one root).
- Task 6 must decide how a buffer's kept batches at a run's end are counted (`abandoned` vs the row
  conservation check); see Task 4 notes. Decided in Task 6: not abandoned (Task 6 notes).

## Rebase (2026-11-04)

Control file said `rebase`; master holds #165 (ENS-optimizer) and the human's note in chain L's
board preamble. dag-driver stopped at its Task 4 checkpoint (`10824c69`); resumes at Task 5 after
sf40-baseline, once its own branch is rebased and re-proven.

Re-proof on the rebased branch (`3394b553`, parent `0aa67e93`), local, CPU and compile only:

- Master's merge reaches four files under `peacockdb-core/src` (`cpu_backend/backend.rs`, `test_support/corpus*`, `test_support/mod.rs`) and adds no node kind and no walk over kinds, so Tasks 1–4 needed no new arm.
- **New baseline, parent `0aa67e93`:** `test_corpus_goldens` 53; `--lib` 1001 cases (master dropped two `test_support::corpus` tests, so dag-driver's 25 sit on top: 1026 = 1024 passed + 2 ignored); `test_cpu_corpus` 2451; `plan_goldens` 25.
- **One failure, mine from Task 4:** `test_cost_model::every_node_kind_is_in_exactly_one_cost_category` refused `GpuHoldBuffer`. The test predates the rebase: it was red at the Task 4 checkpoint, which ran `--lib`, the goldens and the CPU corpus but not `test_cost_model`. The fix bins `GpuHoldBuffer` into `cuda_forward_bytes` (multiplier 0.0) in `testdata/cost_model.conf`: a buffer hands whole batches on without a copy, as a forwarder does. **Every `.cost.txt` prints each category's node list**, so the 15 cost goldens change on 1017 lines. The change is that comment and nothing else: with comments stripped, every file is byte-identical to before. This is a deviation from the Global Constraint "every golden byte-identical", in comment text only; there is no category a node kind can join without it. `scripts/ticket_widget.py` leaves `testdata/report.html` unchanged.
- **Green after the fix:** `cargo test --features rust-only -p peacockdb-core`, every target: `--lib` 1024 + 2 ignored, `test_corpus_goldens` 53, `test_cost_model` 3, `test_cpu_corpus` 2451, `test_crate_privacy` 1, `test_golden_format` 41. `plan_goldens` 25. By name: `executor::driver::` + `plan::` 328, `hold_buffer` 7, `random_dags` 1, `plan::dag::tests` 10. `cost-report --test test_source_formatting` 32. `ctest -L cpu` 1/1, which is `peacock_cpu_tests` with 64 cases, `Lifecycle.DuplicateWithNoPlanIsRefused` among them. The `--features gpu --no-run` build against rapids-cuda-12.2 compiles with no warning; its `--list` has 743 `gpu_tests::` cases.
- **build-test.md:** HEAD's page plus the three hunks the replay lost. The C++ CPU/FFI row is dag-driver's (64, the duplicate refusal named) in HEAD's row order. The cpu header is 3533 (`--lib` 1026 + `test_cpu_corpus` 2451 + `test_corpus_goldens` 53 + `test_cost_model` 3). The gpu header is 2412 (`gpu_tests::` 743 + HEAD's 1009, 659 and 1). Every dag-driver row outside the hunks had merged and matches `--list` (driver tests 133, Driver internals 52, validate 46, dag 10, estimation 15, wire 32, plan text 16).

Coordinator decision (2026-11-04): `GpuHoldBuffer` joins `cuda_forward_bytes` in `cost_model.conf`, so
15 `-mini.cost.txt` goldens change in their category comment line only (numbers identical). This
departs from the spec's "every existing golden byte-identical"; the alternative, exempting the buffer
from the taxonomy test, would hide a node kind from the cost model. Name it in the signoff.

## Rebase 2 (2026-11-05)

Rebased onto `ENS-sf40-baseline` `606335f4`, the finished chain (containers-rebase, new-hosts,
pinned-loads, bench-rows, sf40-baseline). Re-proof at `d3076c44`, local, CPU and compile only.

- **New baseline, parent `606335f4`** (its tree exported and run on its own): `--lib` 994 (992 passed + 2 ignored); `test_corpus_goldens` **61**; `plan_goldens` 25. Later golden checks compare with 61 plus each task's additions.
- pinned-loads' `RowGroupMeta.compressed_bytes` and its scan-path change needed nothing more: the replay had already given every dag-driver fixture `compressed_bytes: 0` (`tests/rebuild.rs`, `plan/validate/tests.rs`, `wire/tests.rs`). Every target compiles with no warning.
- **Green:** `cargo test --features rust-only -p peacockdb-core`, every target: `--lib` 1056 + 2 ignored (994 + dag-driver's 64), `test_corpus_goldens` 61, `test_cost_model` 3, `test_cpu_corpus` 2451, `test_crate_privacy` 1, `test_golden_format` 41. `plan_goldens` 25. `cargo test -p cost-report`: 49, and `test_source_formatting` 32. `ctest -L cpu` 1/1: `peacock_cpu_tests`, 64 cases, `Lifecycle.DuplicateWithNoPlanIsRefused` among them. `peacock_plan_tests` lists 136, the four `HandleDuplicate` cases among them. The `--features gpu --no-run` build against rapids-cuda-12.2 compiles with no warning; its `--list` has 743 `gpu_tests::` (the duplicate's `abi.rs` case among them) and 5 `ffi_tests::`.
- `git status --short testdata/` is empty. Against the parent, `testdata/` differs only in `cost_model.conf` and the 15 `-mini.cost.txt` category comment lines (`, GpuHoldBuffer`): the coordinator's decision of 2026-11-04.
- **build-test.md:** HEAD's page plus the three hunks the replay lost (`/tmp/Lsurg/d2/build-test.md.merged`). C++ CPU/FFI row: the duplicate refusal, 63 → 64. Plan-executor row: the four duplicate cases, HEAD's 132 → 136. cpu header 3573 (`--lib` 1058 + 2451 + 61 + 3).
- **Drift repaired, from Task 2:** Task 2 read the "Plan types" row's 43 as drift against `plan::validate::tests`' 35. It was not: the row also counts `plan::layout::tests` (4) and `plan::aggregate::tests` (4), as its text says ("layout canonical forms and equality; whether a hash survives a regrouping"), and 35 + 8 = 43. The row now links the three modules and says 54 (46 + 4 + 4). With that, the cpu rows sum to the header, 3573, and every `--lib` test maps to a row.
- **Grand total** was never moved by dag-driver. Recounted by the page's rule (the N columns of both tables): 7953 — Rust 6123 (parent's 6058 + `--lib` 64 + `gpu_tests::` 1), C++ 283 (278 + 1 + 4), Python 1547.
- 2026-11-05: green (Task 11); PR #199 (base ENS-sf40-baseline); reviewer round 1.
- review round 2: important closed; doc-comment nit fixed by the coordinator (comment-only); the two unused arrow includes in test_plan_executor.cpp left (nit). completing.
- completeness approved 2026-11-06; dispatch 42137166270 (00:29:13Z; first device run since review round 1), PR run 42137167965.
- done 2026-11-06: dispatch 42137166270 and PR run 42137167965 green (GPU Tests (nebius-gpu) included).
