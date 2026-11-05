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

## Golden baseline (Task 1 Step 1, parent state `f52c11b6`, local)

- `--lib`: 984 passed, 2 ignored (986 cases).
- `--test test_corpus_goldens`: **53 passed** (master's 28 plus the chain's additions). Later golden checks compare with 53 plus each task's additions.
- `--lib -- planner::tests::plan_goldens`: 25 passed.
- `git status --short testdata/`: empty.

## VM time

| VM | start | stop | hours |
|---|---|---|---|
| nebius-gpu (Task 1 Step 4) | 2026-11-04 14:24 UTC | 2026-11-04 14:57 UTC | 0.55 |

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
