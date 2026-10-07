# Joins run on the session on both engines, with no recipe and no refusal left

Kind: production

**This task closes** [#155](../tickets/joins.md#t155) (umbrella: join execution through a wider C
and FlatBuffers API), [#152](../tickets/joins.md#t152) (the build handle does not survive a
streamed probe), [#173](../tickets/joins.md#t173) (a finish whose probe produced no keys refuses
what it could answer from the build side), [#212](../tickets/joins.md#t212) (a build side that
emits no batch at all still refuses Right, Full and RightAnti), [#159](../tickets/joins.md#t159)
(RightSemi/RightAnti with a residual filter has no cuDF path), [#59](../tickets/joins.md#t59)
(nullable-key semantics for semi/anti/mark joins), [#80](../tickets/joins.md#t80) (anti-join
`NOT IN` three-valued logic), [#137](../tickets/joins.md#t137) (the planner does not drop null
join keys before the shuffle), [#220](../tickets/joins.md#t220) (the cpu's joins answer several
batches per call), [#190](../tickets/joins.md#t190) (the cpu drops a nested-loop join's
projection), [#207](../tickets/joins.md#t207) (both backends drop a cross join's projection),
[#208](../tickets/joins.md#t208) (the cpu's cross join answers nothing over a zero-row build), and
the tickets join-session-cpp built the C++ half of: [#136](../tickets/joins.md#t136),
[#153](../tickets/joins.md#t153), [#160](../tickets/joins.md#t160), [#215](../tickets/joins.md#t215),
[#63](../tickets/joins.md#t63), and #154's `join.cpp` sites (so #154 closes whole). Eighth of the
join-rewrite chain.

The design is [`join-rewrite-design.md`](join-rewrite-design.md): §1.3 (validation), §3.5 (the
`NOT IN` rewrite), §4 (planner, wire, executors, driver, exec_model), §5.7 (tests). This spec does
not repeat it.

## The work

1. **Planner** (§4.1): the capability matrix as two facts; the refusals lifted (#153, #159, #160,
   the nulls.rs anti/mark refusal), `can_be_null` kept as `planner/nullability.rs`; the two
   unreachable refusals kept; the `NOT IN` rule with data-driven nullability (§3.5); #137's
   `IS NOT NULL` filters; the predicate-free nested loop as cross for Inner only, over `true`
   otherwise; the explicit `__rowcount__` placeholder.
2. **Wire** (§4.2): `CudfJoin` written for all three plan nodes; the join recipes, roles and
   inputs removed; the old three fbs tables and `execute_*_join` removed with them.
3. **GPU executor** (§4.3): the session typestate, `set_build(Option)`, `Option` returns,
   `owes_nothing`, `Drop` releasing; `copy_of`, `build_copy`, `finish_without_keys` go; pricing as
   §4.3 says.
4. **CPU executor** (§4.4): one DataFusion join stream per lane, `None` per probe for the
   build-side semi family, one batch per call otherwise.
5. **Driver** (§4.5) and **exec_model** (§4.6).
6. **Tests** (§5.7): the 56 join pins flip; the new harness cases, including every pbench join
   shape at node level and each `key_types` column as a join key; the planner, wire, executor,
   driver and exec_model tests named there.
7. **Corpus and pbench cells**: every cell whose last ticket closes here is run at its mode on
   shad-gpu and compared with DuckDB, and enabled if it passes; one that meets another issue gets
   that ticket on its row (or a new one).
8. **Cleanup — nothing of the old join path survives.** Removed, each checked gone by a grep the
   PR quotes and by a build with no dead-code warning on either side:
   - wire: `wire/join.rs`'s recipe writers (all but `CudfJoin`'s), `attach.rs`'s `cross_join`;
     the join-only vocabulary — `Input::{BuildSide, BuildSideCopy, BatchCopy, AccumulatedKeys}`,
     `ProjectRole::{ProbeKeys, NullPad, Narrow}`, `FbKind::{HashJoin, CrossJoin, NestedLoopJoin}`
     and their `recipes.rs` renderings. `CallPattern::AtDone` and `Input::PriorOutput` stay: the
     accumulators use them (`gpu_backend/accumulate.rs`, `attach.rs`).
   - fbs and C++: `CudfHashJoin`, `CudfCrossJoin`, `CudfNestedLoopJoin`; `execute_hash_join`,
     `execute_cross_join`, `execute_nested_loop_join` and their `dispatch.cpp` arms; the map arm's
     "multi-partition joins are not implemented yet" refusal text (`node_session.cpp:584-596`),
     which describes a path no join takes any more.
   - Rust executors: `GpuJoin`'s `per_probe`/`at_done` lists, `make`, `copy_of`, `build_copy`,
     `finish_without_keys`; the cpu's `Calls` (per-call join, key project, finish join, pad
     project, `empty_build_answers_nothing`); `JoinExecutor::without_build`.
   - planner and plan: `planner/nulls.rs`'s refusal (the analysis moves), `answers_in_one_call`,
     `per_call_join_type`, `finish_join_type`, `empty_build_answers_nothing`, the probe-side coalesce.
   - driver: `feeds_owing_build`, the scatter-drop exception, the mock's `empty_build_owes_its_probe`.
   - exec_model: `operators/recipe_join.py`, `recipe.py`'s `copy_handle`, the `cudf_calls.py`
     entries only the recipe join called.
   - wiki: `architecture.md`'s join recipes and capability matrix rewritten from the design;
     `hacks-audit.md`'s join items (P1, P4, P8, P9, P10, P13; findings 5, 11, 12) marked resolved;
     `build-test.md`'s rows; `join-rewrite-design.md` archived with the specs.
9. **The estimate.** [`reports/join-rewrite-cell-estimate.md`](../reports/join-rewrite-cell-estimate.md),
   committed with the chain's specs, predicts which cells flip and which meet another issue. The
   completeness record compares it with what turned on: each miss, either way, gets a line with its
   cause.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/planner/` (translator `nodes.rs`, `common.rs`, `nulls.rs` → `nullability.rs`, the new `NOT IN` rule, `pipeline.rs`, `mod.rs`), `lib.rs` | §4.1, §3.5 |
| `peacockdb-core/src/plan/join.rs`, `plan/mod.rs`, `plan/validate*` | the matrix; validation; no zero-column schema |
| `peacockdb-core/src/wire/` (`join.rs`, `attach.rs`, `mod.rs`, `node_writer.rs`) | §4.2 |
| `peacockdb-core/src/executor/gpu_backend/`, `cpu_backend/`, `driver/`, `executor/mod.rs` | §4.3–§4.5 |
| `peacockdb-core/src/planner/memory_estimation.rs` | the session's pricing |
| `cpp/src/operators/join.cpp`, `project.cpp`, `dispatch.cpp`, `flatbuffers/gpu_plan.fbs` | the old join paths and tables removed; an empty projection refused |
| `peacockdb-core/src/tests/**`, `planner/tests/**`, `plan/tests/**`, `executor/**/tests*`, `wire/tests.rs`, `wire/gpu_tests/` | §5.7 |
| `cpp/tests/gpu/test_plan_executor.cpp` | the four `CudfHashJoin` tests onto the session |
| `scripts/exec_model/` | §4.6 |
| `testdata/goldens/**`, `recipe-payloads.txt`, `cost-registry.csv`, `corpus_cases.inc` | the session's recipe line; #137's tpcds filters; the 19 `__rowcount__` nodes; cells |
| `llm-wiki/architecture.md` (Joins), `build-test.md`, `tickets/`, `reports/` | the lasting parts of the design; counts; the closed tickets archived |

Component-level API: the `JoinExecutor`/`ProbingJoin` traits (`set_build(Option)`, `Option`
returns, `owes_nothing`; `without_build` removed); the wire's join vocabulary; three fbs tables
removed. The C ABI's old join paths go with them.

## Restriction

Joins, and the planner pieces the design names. No change to non-join nodes' recipes. No
broadcast (#140): the session is its foundation only. No float key normalization (#243).

## Registry

The estimate (`reports/join-rewrite-cell-estimate.md`) expects this task to flip 23 cpu cells
(#190 ×20, #212 ×3) and up to about 428 gpu cells, about 306 of them weighted by confidence; and
three groups to meet an out-of-chain issue instead: tpch rollup_over_join ×5 on #65 (its row lacks
`65` today), tpcds q78 ×5 on #60, tpcds q32 at tp4 on #199. Those rows gain their tickets. The
stale `183` rows' 25 join cells (tpch nested-loop-join, q4, cross-join, nested-loop-left-join,
semi-join, anti-join, q15) are run here, since #183 is closed and #152 is their real blocker.
pbench's join rows turn on with the rest. Each cell enabled with its DuckDB case green.

## Verification bar

- rust-only: the full cpu tier, the planner and wire tests, `test_cpu_corpus` with every newly
  enabled cpu cell and its DuckDB case; the golden regenerations reviewed as one diff per kind
  (recipe line, #137 filters, `__rowcount__`).
- device: the full gpu tier; `test_join_session.cpp`; every newly enabled gpu cell on shad-gpu at
  its mode; `gpu-result.txt` against DuckDB.

## Device workflow

`build-test-shadgpu.sh`: a cycle for the harness, then the corpus in batches by dataset and mode
(about five queries per round where cells are new).
