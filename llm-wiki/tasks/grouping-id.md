# The device's grouping-set id is DataFusion's; q66's quotient and the DISTINCT lowering run on a device

Kind: production

**This task closes [#65](../tickets/corpus-coverage.md#t65)** (the device's grouping-set id is not
DataFusion's value or width) **[#55](../tickets/corpus-coverage.md#t55)** (q66: two-phase
decimal aggregate ignores the partial-phase divisor cast) **and
[#262](../tickets/corpus-coverage.md#t262)** (the DISTINCT lowering's device cells have never
run). First of chain L, whose base is master once chains J and K have both merged: J closes #152,
#220 and #189, which hold most of the cells this task turns on, and K adds the DISTINCT lowering
and `tpch/rollup-distinct`, whose device cells are off on #65.

## Why it happens

**#65.** `execute_aggregate`'s grouping-set arm (`cpp/src/operators/aggregate.cpp`) sets bit `i`
for masked key `i` and builds the id from a `cudf::numeric_scalar<int32_t>`. DataFusion 45 folds
`(acc << 1) | is_null` over the keys in order, the first key highest (`group_id_array`,
`datafusion-physical-plan` `aggregates/mod.rs`), into a `u64`, and holds it at the width
`Aggregate::grouping_id_type` picks: `UInt8` up to 8 keys, `UInt16` to 16, `UInt32` to 32,
`UInt64` beyond; past 64 keys it refuses. A two-key rollup is 0, 1, 3 there and 0, 2, 3 on the
device. The merge only needs one id per set, so it is right; `GROUPING()` over the full key list
reads the device's bits, and the schema validator refuses the width.

**#55.** Most likely stale. DataFusion casts `sum(decimal / int)`'s divisor in the Partial phase
only; the old executor's Final re-evaluated the argument over state. The wire has had no Final
since: `Init` is written as `Partial` and `Merge` as `Merge`, the division runs once in `Partial`,
and a merge reads state columns by reference. What is missing is a device run.

**#262.** Chain K ran without a GPU, so distinct-companions is proven on the cpu only. Its plans
reach the wire as no device has run them: a two-stage aggregate whose outer init runs merge
aggregators over state, a `__distinct_arg` key, a grouping id narrowed by a project. Its C++ edits
(the `distinct` guard gone from `aggregate.cpp`, `test_plan_executor.cpp`'s arguments) were built
and not run. A device answer could differ from the cpu's, and nothing says so: the cells are off.

## The work

1. **#65, the id.** A static `grouping_id_column(mask, nkeys, rows)` in `aggregate.cpp` folds
   `gid = (gid << 1) | masked` over the mask in key order into a `uint64_t`, and builds the column
   from a `numeric_scalar` of `uint8_t`, `uint16_t`, `uint32_t` or `uint64_t` for `nkeys` ≤ 8,
   ≤ 16, ≤ 32, beyond. Past 64 keys it throws, naming the count, as DataFusion refuses. The
   `int32_t gid` block and its comment go. No wire change and no payload change: the payloads
   print masks, never an id.
2. **#65, the exec model.** `grouping_set_id` (`scripts/exec_model/operators/aggregates.py`)
   folds the same way, `rollup_masks`' docstring says 0, 1, 3, and the pins in
   `tests/test_operators.py` and `tests/test_end_to_end.py` go from `{0, 2, 3}` to `{0, 1, 3}`, and
   `test_operators.py`'s single-set pin from `2` to `1`.
3. **#55, the proof.** A walk test beside
   `each_lane_merges_its_own_state_before_the_cross_lane_merge_folds_them` in
   `wire/gpu_tests/mod.rs`: `SUM_OF_QUOTIENTS`, a grouped `sum(l_extendedprice / l_linenumber)`
   under an outer `sum`, through `assert_walk_matches_datafusion` at `TWO_LANES`, with the
   `PARTIAL`, `MERGE` and `FINALIZE` counts pinned from the trail as the neighbouring tests do.
   Green closes #55 as stale. Red means the defect is live: the throwing call names its phase,
   and the merge's argument is fixed here.
4. **#262, the proof.** A walk test of a two-stage DISTINCT at `TWO_LANES` through
   `assert_walk_matches_datafusion`: `l_returnflag, count(DISTINCT l_suppkey), avg(l_quantity),
   count(*)` over lineitem, grouped by `l_returnflag` — shapes DataFusion 45 answers right (not
   `avg` or `stddev` DISTINCT, which it answers wrong or refuses; distinct-companions' corpus
   note). Plus the full device gtest run, which runs K's `test_plan_executor.cpp`.
5. **The cells.** The corpus rows below are run at every device mode still off when this task
   builds, and each cell is enabled if it passes, compared with its golden and with DuckDB. A cell
   that fails gets the ticket it fails on (an open one, or a new one filed with the query and mode)
   and stays off under it. `65`, `55` and `262` are struck from the rows' tags in
   `testdata/cost-registry.csv` once no off cell is left without another ticket.

## Corpus

#65 reaches the corpus two ways.

**A wrong answer: `tpch/rollup-grouping` alone** (below), the one query reading the id's value.

**A schema refusal only: every corpus plan with a grouping-set id.** `tpch/rollup-over-join` and
tpcds q5, q14, q18, q22, q77, q80 (the plan goldens' `__grouping_id`), and chain K's
`tpch/rollup-distinct`. None reads the id: the merge needs one distinct id per set, which the
device gives, and the final project drops it, so their answers are right. But each row is
`schema_validation_enabled`, and the device corpus holds every batch a node emits to its declared
schema (`test_support/schema_validation.rs`): the init's `Int32` id against the declared `UInt8`
refuses the cell there, the divergence
`bug_grouping_sets_hold_an_int32_grouping_id_where_the_plan_declares_uint8` pins. Predicted, not
seen: on master today no device cell of theirs runs past #152, #189 or #220, which chain J
closes. Their registry tags as of 2026-10-08:

| row | tags |
|---|---|
| tpch rollup-over-join | `152 189 220` (lacks `65`; join-backend adds it if a cell then fails on it) |
| tpcds q5 | `65 152 189` |
| tpcds q14 | `65 220` |
| tpcds q18, q22 | `65 189 220` |
| tpcds q77 | `65 152 212 220` |
| tpcds q80 | `65 152 189 220` |
| tpch rollup-distinct (chain K) | `65 189` |
| pbench rollup-small-keys (chain J, repartition-keys) | `65` |

Any other row carrying `65` when this task builds is run the same way: the registry, not this
table, is the list. The id's value shows only through `GROUPING()`. The corpus's users are q70 and q86 (and q67's
rollup), window queries that never run (`32 143`); they stay off.

**#262's rows:** `tpch/distinct-functions` (off at all five device modes on `262` alone; its
oracle is DuckDB's, DataFusion's being `data_fusion_disabled`), tpcds q28 (`262` and `152`) and
`tpch/rollup-distinct` (`262` once chain K has run, with `65 189`). `262` is struck when no off
cell is left without another ticket. `distinct-functions` carries a `stddev(DISTINCT …)`, whose
float digits differ across lanes as `shuffle-stddev`'s do, so its gpu oracle goes from
`golden_exact` to `golden_approx_std`; `rollup-distinct` and q28 stay exact.

**#55's row:** tpcds q66, tags `55 152 220`. Its five device cells are off; once chain J closes
#152 and #220, `55` alone holds them.

**A new tpch query,** `testdata/tpch-queries/rollup-grouping.sql`: the ticket's simplest, from
[`reports/corpus-fixes.md`, fix 14](../reports/corpus-fixes.md#fix14).

```sql
-- GROUPING() over the full key list reads the grouping-set id itself, so its value and width
-- are the answer, not just a tag that keeps the sets apart (#65).
select n_regionkey, n_nationkey, grouping(n_regionkey, n_nationkey) as g, count(*)
from nation
group by rollup (n_regionkey, n_nationkey);
```

31 rows with `g` in {0, 1, 3} on the cpu; before the fix the device answers the five subtotal rows
with `g = 2`. Its five plan goldens, its cpu sections, its `duckdb-result.txt` section from the
pinned `testdata/duckdb_result.py`, a `corpus_cases.inc` line and a registry row (features
`rollup`) are written. Every cell enabled if it passes; a failing one takes its ticket.

## Scope

| path | change |
|---|---|
| `cpp/src/operators/aggregate.cpp`, a new `cpp/tests/gpu/` gtest file (`test_plan_executor.cpp` is past the 1000-line cap) | the id's fold and width, through hand-built plans in `peacock_plan_tests` (`grouping_id_column` is file-static, so the gtests need the device) |
| `peacockdb-core/src/tests/executor_cases.rs`, `executor/cpu_backend/tests/contract.rs`, `executor/gpu_backend/gpu_tests/contract.rs` | the rollup contract case |
| `scripts/exec_model/operators/aggregates.py`, `scripts/exec_model/tests/test_operators.py`, `test_end_to_end.py` | the model's id |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs`, `peacockdb-core/src/wire/gpu_tests/mod.rs` | the four `bug_` pins flip, `grouping_sets_as_exported` goes; the rollup cases; `SUM_OF_QUOTIENTS` |
| `testdata/tpch-queries/rollup-grouping.sql`, `testdata/goldens/tpch.sf1/*.plans.txt`, the cpu tier's and DuckDB's sections for it | the new query |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | the two-stage DISTINCT walk |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | device modes and tags of the rows above |
| `llm-wiki/architecture.md` (grouping sets), `build-test.md`, `tickets/` | the id described; counts; #65, #55 and #262 archived |

Component-level API: none. No facade, trait, ABI or wire change; `recipe-payloads.txt` unchanged.

## Restriction

The id and the two proofs. `GROUPING()` over a subset or a reordering of the keys stays refused on the
device (#230); DataFusion 55's duplicate ordinal stays #228. A cell that fails on anything but the
id, the quotient or the DISTINCT lowering's device path is a ticket, not a fix here.

## Tests

- gtests, on the device: the id over 1, 2, 8, 9, 17, 33 and 64 keys, value and type each (`UInt8`
  at 8, `UInt16` at 9, `UInt32` at 17, `UInt64` at 33 and 64), and 65 keys refused.
- `bug_grouping_sets_carry_the_devices_own_id_and_type`,
  `bug_grouping_sets_over_zero_rows_are_zero_rows_under_the_devices_id_type`
  (`gpu_tests/aggregate_cases.rs`),
  `bug_grouping_sets_hold_an_int32_grouping_id_where_the_plan_declares_uint8`
  (`aggregate_schema_cases.rs`) and
  `bug_a_rollup_partial_holds_an_int32_grouping_id_where_the_plan_says_uint8`
  (`wire/gpu_tests/mod.rs`) become agreement cases under names without `bug_`;
  `grouping_sets_as_exported`, which swapped the bits and the type, goes.
- A rollup contract case, two keys, on both engines: the device's id equals the cpu's, value and
  type. A walk test of `rollup-grouping.sql`'s query at `TWO_LANES` against DataFusion.
- `SUM_OF_QUOTIENTS` and the two-stage DISTINCT walk, as in the work.
- The exec model's suites green with the new pins.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: the build against cuDF 25.02 locally, `ctest -L cpu` green; the id's gtests run with the
  device tier.
- exec model: every `scripts/exec_model/tests/test_*.py`.
- device: every `peacock_*_tests` gtest binary (K's `test_plan_executor.cpp` included),
  `gpu_tests::` and the walk tests, and `test_gpu_corpus` over the rows above, on the GPU host the
  chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.
The corpus run is filtered to the rows above and `rollup-grouping`.
