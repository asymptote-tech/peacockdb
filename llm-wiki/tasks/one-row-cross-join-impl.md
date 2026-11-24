# one-row-cross-join Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A cross join with a side of at most one row joins the inner-join cluster beside it, DPhyp
places the one-row relation, and disassembly rebuilds the `GpuCrossJoin` where it was placed.

**Architecture:** `multijoin.py` collects a one-row cross join's sides into the cluster and gives
each one-row relation an edge with no keys and no residual to every other relation.
`join_order.py` hands DPhyp those edges, estimates a cross product as `rows × rows`, and builds
the one-row side. `disassembly.py` rebuilds a join with only one-row edges across it as a
`GpuCrossJoin` on one lane, as the translator wires one. The corpus is made again at all five
modes.

**Tech Stack:** Python 3.13 (pandas 3.0.6, pyarrow 25.0.1) in the build image, the
`peacockdb-dphyp` library over its C ABI, the prototype's own test harness (`py <file> [name]`).

**Spec:** `llm-wiki/tasks/one-row-cross-join.md`

## Global Constraints

- Nothing in the engine changes: `peacockdb-core/`, `cpp/`, `peacockdb-dphyp/`, the wire format.
- CPU only; nothing measures the optimizer on the device.
- No test is weakened, and every answer equals DuckDB's.
- A plan that moves and costs more than before is a finding in the detail file, not a kept change,
  unless it is explained.
- `llm-wiki/coding-style.md` holds for every change: files under 1000 lines, functions under 150,
  comment limits, commit messages of at most 10 lines.
- Every run goes through the build image: `py` as `scripts/exec_model/README.md` ("DPhyp") defines
  it, `PEACOCK_DPHYP_LIB` set.

## Review Focus

1. tpcds q23 at tp4: the plan's cross join runs on one lane and the join below it on four. Once
   the one-row relation moves down, the top hash join must keep the four lanes its relations had,
   and the cluster's root must come back onto the plan root's one lane (tests in Task 4).
2. A partial aggregate with no group-by is a row per input batch, not one: four lanes at tp4,
   or one lane at tp1 below its final aggregate (test in Task 1).
3. A one-row cross join beside a non-inner join stays a wall, its plan unchanged (test in Task 2).
4. A replan whose one-row side has become a `GpuMemorySource`: the cross join is a wall again,
   since a memory source is not one row by its form, and the answer is unchanged (test in Task 4).
5. Two one-row relations crossed together above a cluster: one relation, never a cluster of their
   own; between two one-row sides DPhyp's left builds (tests in Tasks 2 and 3).

---

### Task 1: `at_most_one_row`

**Files:**
- Modify: `scripts/exec_model/optimizer/multijoin.py`
- Test: `scripts/exec_model/tests/optimizer/test_multijoin.py`

**Interfaces:**
- Produces: `multijoin.at_most_one_row(node: EngineNode) -> bool`; the property
  `Edge.one_row -> bool`, true where `keys == ()` and `residual is None`.

- [ ] **Step 1: Write the failing tests**, plans built with the file's `plan()` and `scan()`:
  - `test_a_final_aggregate_without_group_by_is_one_row`: `GpuAggregateBatches` with
    `group_by=[]` and a `final=[...]` field → True; `GpuAggregate` with `group_by=[]` at `lanes=1`
    and a `final=` field (q23 at tp1) → True.
  - `test_a_partial_aggregate_is_not_one_row`: `GpuAggregate`, `group_by=[]`, no `final`, at
    `lanes=4` and at `lanes=1` → False (a partial aggregate emits a row per input batch, tpch q22
    at tp1); `GpuMergePartitions` over the 4-lane one → False.
  - `test_fetch_and_empty_are_at_most_one_row`: `GpuLimit` `fetch=1` and `fetch=0` at `lanes=1,
    batches=single` → True, `fetch=2` → False; `GpuEmpty` → True.
  - `test_a_fetch_per_lane_or_per_batch_is_not_one_row`: `GpuSort` `fetch=1` at `lanes=4,
    batches=single` → False (a row per lane), at `lanes=1, batches=multiple` → False (a row per
    batch); a final aggregate with `group_by=[]` at `lanes=4` → False.
  - `test_what_passes_one_row_on_is_one_row`: a `GpuFilter`, a computing `GpuProject`,
    `GpuCoalesceAllBatches` and `GpuMergePartitions` over a final aggregate → True; a
    `GpuCrossJoin` of two final aggregates → True, of a final aggregate and a scan → False.
  - `test_scans_joins_and_grouped_aggregates_are_not_one_row`: a scan, a `GpuHashJoin`, a final
    aggregate with `group_by=[a@0]` → False.
  - `test_an_edge_without_keys_or_residual_is_one_row`: `Edge(1, 2).one_row` is True; with keys,
    or with a residual, False.
- [ ] **Step 2: Run** `py scripts/exec_model/tests/optimizer/test_multijoin.py one_row` — FAIL,
  `at_most_one_row` not defined.
- [ ] **Step 3: Implement** in `multijoin.py`. An aggregate counts when its `group_by` is `[]`
  and it carries `final`, on one lane in one batch. A node counts when its `fetch` is 0 or 1 and
  it runs on one lane and emits one batch (`lanes=1`, `batches=single`): a `GpuSort` with a `fetch` keeps it
  per batch and per lane. `GpuEmpty`
  counts. A `GpuFilter`, a `GpuProject` and `_WIRING` count when their child does. A `GpuCrossJoin`
  counts when both children do. Nothing else counts.
- [ ] **Step 4: Run** the file whole — PASS.
- [ ] **Step 5: Commit** `multijoin.py` and the test.

### Task 2: one-row cross joins in the cluster

**Files:**
- Modify: `scripts/exec_model/optimizer/multijoin.py`, `scripts/exec_model/optimizer/disassembly.py`
  (`baseline`, `planned_lanes`)
- Test: `scripts/exec_model/tests/optimizer/test_multijoin.py`,
  `scripts/exec_model/tests/optimizer/test_disassembly.py`

**Interfaces:**
- Consumes: `at_most_one_row`, `Edge.one_row` (Task 1).
- Produces: `MultiJoin.one_row: int = 0`, the mask of its one-row relations; one `Edge(1 << r,
  1 << o)` per one-row relation `r` and every other relation `o` (`o > r` where both are one-row,
  so a pair has one edge); `GpuCrossJoin`s of the cluster in `MultiJoin.joins`, carrying
  `mj` from `named`; `disassembly.planned_lanes(cluster) -> dict[int, int]` keyed by the hash
  joins' masks with `cluster.one_row` removed, a cross join recording none.

- [ ] **Step 1: Write the failing tests.** A fixture `Q23` in `test_multijoin.py`: a
  `GpuCrossJoin` whose build is `GpuProject(GpuAggregate(group_by=[], final=..., lanes=1))` over a
  scan `ms`, and whose probe is a picking `GpuProject` over `GpuHashJoin(Inner, (c@0, sc@0))` of
  `GpuCoalesceAllBatches(scan cust(c))` and `scan ss(sc, q)`, as tpcds q23's `best_ss_customer`
  has it at tp1.
  - `test_a_one_row_cross_join_joins_the_cluster_below_it`: `clusters(plan(Q23))` is one cluster
    rooted at the cross join; relations `[ms-root, cust, ss]` in collection order; `one_row ==
    0b001`; edges are the key edge `cust–ss` and `Edge(0b001, 0b010)`, `Edge(0b001, 0b100)`;
    `output` lifts the cross join's columns through it (`ms` first, then the probe's).
  - `test_a_one_row_cross_join_below_a_hash_join_is_in_its_cluster`: `HashJoin(scan a, Cross(agg,
    scan b))` on `a = b` → one cluster, three relations, `one_row` the aggregate's bit.
  - `test_cross_joins_of_one_row_relations_alone_form_no_cluster`: `Cross(agg, Cross(agg, agg))`
    over scans (q88's shape) → `clusters` returns `[]`.
  - `test_a_one_row_cross_join_beside_a_non_inner_join_stays_a_wall`: `Q23` with the hash join
    `LeftSemi` → the cluster is the semi join alone, rooted below the cross join, `one_row == 0`.
  - `test_named_gives_the_cross_join_its_cluster_id`: `named(plan(Q23))` → the cross join and the
    hash join both carry `mj="MJ1"`.
  - In `test_disassembly.py`, `test_the_baseline_of_a_one_row_cluster_is_the_plan_order`:
    `baseline(cluster)` of `Q23` is `(0, (1, 2))`, and `reassembled(plan, 1)` renders the plan's
    own text (`plan_text`), the cross join rebuilt by Task 4 — mark this test `bug_` until Task 4
    and rename it there.
  - `test_planned_lanes_leave_one_row_relations_out`: `Q23` at tp4's layout (hash join `lanes=4`,
    cross join `lanes=1`) → `planned_lanes` is `{0b110: 4}`.
- [ ] **Step 2: Run** both files — the new tests FAIL.
- [ ] **Step 3: Implement.**
  - `is_reorderable` admits a `GpuCrossJoin` with a child for which `at_most_one_row` holds.
  - `_Assembly._collect` on such a cross join: each child for which `at_most_one_row` holds
    becomes a relation whole and joins `one_row`; the other child is collected as a hash join's
    side is, with no dropped keys. No `Operator` is added for it.
  - `_Assembly.multijoin` adds the one-row edges.
  - `lift` reads a `GpuCrossJoin` as an inner join's output, through its `projection` if any.
  - `assemble` returns a cluster only where `joins` holds a `GpuHashJoin`. Where the cluster holds
    a non-inner join and a cross join, it assembles again with cross joins as walls. `clusters`
    walks on into the children of a node that gives no cluster, as for any other node.
  - `baseline` and `planned_lanes` walk the cluster's joins by identity
    (`{id(j) for j in cluster.joins}`) instead of `is_reorderable`.
- [ ] **Step 4: Run** `test_multijoin.py`, `test_disassembly.py`, `test_names.py` — PASS but the
  `bug_` test.
- [ ] **Step 5: Commit.**

### Task 3: the estimate, DPhyp's edges and orientation

**Files:**
- Modify: `scripts/exec_model/optimizer/join_order.py`
- Test: `scripts/exec_model/tests/optimizer/test_join_order.py`

**Interfaces:**
- Consumes: `MultiJoin.one_row`, `Edge.one_row`, `planned_lanes` (Task 2).
- Produces: `SetEstimates.rows(mask)` with a set joined by one-row edges alone estimated as the
  product of its two parts' rows; `ordered` passing DPhyp every edge with keys or `one_row`.

- [ ] **Step 1: Write the failing tests**, the cluster from Task 2's `Q23` fixture, its relations
  estimated as the file's existing tests build theirs: `ms` 1 row, `cust` 100,000, `ss` 2,750,000,
  the key edge's estimate 2,750,000.
  - `test_a_one_row_relation_multiplies_the_rows_of_the_set_it_joins`: `rows(0b011) == 100_000`,
    `rows(0b111) == rows(0b110)`, and `cost(0b011) == 100_000 × width(needed(0b011))`.
  - `test_dphyp_crosses_the_one_row_relation_onto_the_smaller_side`: `ordered` on `Q23` gives the
    tree `((0, 1), 2)` up to side order, and its C_out is below the plan order's.
  - `test_the_one_row_side_builds`: `orient` puts `0` first in its join; for a tree joining two
    one-row relations, DPhyp's left side first.
  - `test_lanes_of_a_join_ignore_its_one_row_relations`: at tp4's layout `orient`'s join over
    `0b111` runs on 4 lanes (`planned.get(mask & ~one_row)`).
  - `test_one_row_edges_never_cross_two_larger_relations`: a star `a–b`, `c–b` on keys and a
    one-row `r`, `c` and `a` small → `cost` of `{r, a, c}` is `inf`, and `ordered`'s tree joins
    no two sets that are both above one row across one-row edges alone.
- [ ] **Step 2: Run** `py scripts/exec_model/tests/optimizer/test_join_order.py one_row` — FAIL.
- [ ] **Step 3: Implement.** `SetEstimates._join` and `_applied` estimate one-row edges alone as
  `equi_join` with no pairs: the product of rows. `SetEstimates.cost(mask)` is `inf` where the
  relations of `mask` outside `cluster.one_row` are more than one and no key edges inside `mask`
  connect them. `ordered`: edges
  `if edge.keys or edge.one_row`. `_orient`: a join with only one-row edges across builds the
  side inside `cluster.one_row`, else DPhyp's left, and runs on one lane, its probe merged, as
  Task 4 rebuilds it. Every other `planned.get(mask, …)` reads `mask & ~cluster.one_row`.
  `_join` takes a one-row relation off first where the rest stays connected, so `rows(S ∪ {r})
  == rows(S) × rows(r)` for every key-connected `S`.
- [ ] **Step 4: Run** `test_join_order.py`, `test_dphyp.py`, `test_optimize.py` — PASS.
- [ ] **Step 5: Commit.**

### Task 4: the cross join rebuilt

**Files:**
- Modify: `scripts/exec_model/optimizer/disassembly.py`
- Test: `scripts/exec_model/tests/optimizer/test_disassembly.py`, `test_replan.py`

**Interfaces:**
- Consumes: Tasks 2 and 3.
- Produces: `_Disassembly.part` emitting a `GpuCrossJoin` (`mj` the cluster's) for a join with
  only one-row edges across.

- [ ] **Step 1: Write the failing tests.**
  - Rename Task 2's `bug_` test to `test_…`; it now passes.
  - `test_a_cross_join_is_rebuilt_on_one_lane`: `disassemble(Q23 cluster, ((0, 1), 2), 1)`: the
    root is the hash join, its build `GpuCoalesceAllBatches` over `GpuCrossJoin` whose build is
    the one-row relation (`batches=single`, `lanes=1`) and probe `cust`; no `hashed_on` on the
    cross join; the root's schema equals the plan root's.
  - `test_at_four_lanes_the_cross_join_merges_and_the_root_comes_back_to_one_lane`: the tp4
    layout, the same tree at `lanes=4`: the cross join at `lanes=1` with each side merged onto one
    lane, emitted on `c` onto 4 lanes for the hash join, the hash join at `lanes=4`, and the root
    a `GpuMergePartitions` onto `lanes=1`, as the plan root had.
  - `test_a_key_edge_makes_a_hash_join_whatever_one_row_edges_cross`: tree `(1, (0, 2))`: the
    join of `{1}` with `{0, 2}` is a hash join on `c = sc`, and the join inside `{0, 2}` a cross
    join.
  - In `test_replan.py`, `test_a_replanned_one_row_side_is_a_wall_again`: the reassembled `Q23`
    with the one-row relation replaced by a `GpuMemorySource` assembles with the cross join as a
    relation and runs to the same answer as before the replan.
  - In `test_multijoin.py`, `test_a_residual_reading_a_one_row_relation_keeps_the_wall`: `Q23`
    with `filter=c@build:0 > m@probe:0` on the hash join → the cluster is the hash join alone,
    `one_row == 0`, and `ordered` returns the plan unchanged (`assemble` walls it, Step 3).
- [ ] **Step 2: Run** both files — FAIL.
- [ ] **Step 3: Implement.** In `part`: where `_predicates` gives no keys and every edge across is
  one-row, build `_cross(build, probe, need)` instead of raising: the build side merged onto one
  lane and coalesced to one batch, the probe merged onto one lane (`_wired` at `lanes=1`), the
  node `GpuCrossJoin` with a `projection` where `need` is not the whole output, `lanes=1`,
  `batches=multiple`, no hash. In `disassemble`: where the plan root runs on one lane and the
  part on more, wrap the part in `GpuMergePartitions` onto one lane.
- [ ] **Step 4: Run** `test_disassembly.py`, `test_replan.py`, `test_disassembly_corpus.py`,
  `test_engine_reassembled.py tpcds q23` — PASS.
- [ ] **Step 5: Commit.**

### Task 5: the reports name a one-row edge and a cross join

**Files:**
- Modify: `scripts/exec_model/optimizer/report_text.py`, `report_md.py` where a tree is in words
- Test: `scripts/exec_model/tests/optimizer/test_report_text.py`, `test_report_md.py`

**Interfaces:**
- Consumes: `EdgeReport` with no keys and no residual (from `join_order._edge`).

- [ ] **Step 1: Write the failing tests.**
  - `test_a_one_row_edge_reads_as_a_cross_join`: `_edge` of `EdgeReport(0b001, 0b010, (), (),
    INNER)` is `"  edge r0-r1: one row crossed in, keeps every row"`.
  - `test_a_cross_join_in_a_tree_reads_cross_join`: `.optimizer.md`'s tree for `((0, 1), 2)` over
    `ms`, `cust`, `ss` reads `[[ms] cross join cust] join ss`.
- [ ] **Step 2: Run** — FAIL. **Step 3: Implement** the two cases. **Step 4: Run** — PASS.
- [ ] **Step 5: Commit.**

### Task 6: the corpus made again

**Files:**
- Modify: `scripts/exec_model/testdata/goldens/**`, `llm-wiki/tasks/one-row-cross-join-detail.md`

- [ ] **Step 1:** `py scripts/exec_model/tests/optimizer/test_ordered_corpus.py` and
  `test_cardinality_corpus.py` — PASS; `UPDATE_CANONICAL=1` only where the cardinality golden
  moves for q23, the move recorded in the detail file.
- [ ] **Step 2:** `py scripts/exec_model/run.py` (both benches, all five modes; tpcds `--jobs 1`,
  tpch `--jobs 2`), then `py scripts/exec_model/cost_report.py`. Wall time in the detail file.
- [ ] **Step 3:** `git diff --stat scripts/exec_model/testdata/goldens` and the `.plandiff.txt`
  files: every query other than q23 whose plan moved is explained in the detail file. q23's
  optimized cost and final ratio per mode are stated beside the estimate (about 60 MB less per
  copy, final ratio about 2.55), with any gap explained. q23's dynamic filters in
  `.optimizer.txt`, before and after: each one whose build and key survive still applies.
- [ ] **Step 4:** the corpus tier, every file whole: `test_engine_answers.py`,
  `test_engine_dynamic_filters.py`, `test_engine_reassembled.py`, `test_engine_optimized.py` —
  PASS, every answer equal to DuckDB's.
- [ ] **Step 5: Commit** the goldens and the detail file.

### Task 7: documents and tickets

**Files:**
- Modify: `scripts/exec_model/optimizer.md` ("Clusters", "The cost DPhyp is given",
  "Orientation", "Disassembly", "Constants"), `scripts/exec_model/design.md` (the clusters and
  disassembly sections), `llm-wiki/tickets/optimizer.md`, `llm-wiki/tickets.md`,
  `llm-wiki/archive/archived-tickets.md`

- [ ] **Step 1:** `optimizer.md`: the one-row rule as Task 1 has it, the wall's exception, the
  one-row edge and its estimate and cost, the one-row side building, the cross join rebuilt on one
  lane and the root brought back onto the plan root's lanes. Within 300 lines.
- [ ] **Step 2:** `design.md`: the same where it describes clusters and disassembly.
- [ ] **Step 3:** #101's q23 paragraph carries Task 6's figures; #320 moves to
  `archived-tickets.md` (Done), the index and the contents block updated;
  `python3 scripts/test_ticket_widget.py` — OK.
- [ ] **Step 4:** the cheap tier whole, as `scripts/exec_model/README.md` ("Cheap") runs it — PASS.
- [ ] **Step 5: Commit.**
