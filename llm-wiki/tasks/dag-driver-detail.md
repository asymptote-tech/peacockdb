# dag-driver detail

Branch `ENS-dag-driver`, forked from `ENS-sf40-baseline` (`080b1bac`) on 2026-11-04.
Plan: `dag-driver-impl.md`, Tasks 1–11; each *Commit* step is a checkpoint the coordinator commits and pushes.

## Dispatches

- 2026-11-04: developer dispatched at Task 1 Step 1. Verda unreachable (no VERDA_CLIENT_ID); CPU tests run locally. Device steps on nebius-gpu (L40S).
- 2026-11-04: developer dispatched at Task 3, all steps, CPU only, local (verda unreachable).

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

## Golden baseline (Task 1 Step 1, parent state `f52c11b6`, local)

- `--lib`: 984 passed, 2 ignored (986 cases).
- `--test test_corpus_goldens`: **53 passed** (master's 28 plus the chain's additions). Later golden checks compare with 53 plus each task's additions.
- `--lib -- planner::tests::plan_goldens`: 25 passed.
- `git status --short testdata/`: empty.

## VM time

| VM | start | stop | hours |
|---|---|---|---|
| nebius-gpu (Task 1 Step 4) | 2026-11-04 14:24 UTC | 2026-11-04 14:57 UTC | 0.55 |
