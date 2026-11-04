# dag-driver detail

Branch `ENS-dag-driver`, forked from `ENS-sf40-baseline` (`080b1bac`) on 2026-11-04.
Plan: `dag-driver-impl.md`, Tasks 1–11; each *Commit* step is a checkpoint the coordinator commits and pushes.

## Dispatches

- 2026-11-04: developer dispatched at Task 1 Step 1. Verda unreachable (no VERDA_CLIENT_ID); CPU tests run locally. Device steps on nebius-gpu (L40S).
- 2026-11-04: developer dispatched at Task 3, all steps, CPU only, local (verda unreachable).
- 2026-11-04: developer dispatched at Task 4, all steps, CPU only, local (verda unreachable).

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
  conservation check); see Task 4 notes.

## Rebase (2026-11-04)

Control file said `rebase`; master holds #165 (ENS-optimizer) and the human's note in chain L's
board preamble. dag-driver stopped at its Task 4 checkpoint (`10824c69`); resumes at Task 5 after
sf40-baseline, once its own branch is rebased and re-proven.
