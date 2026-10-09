# A keyless aggregate answers one row whatever arrived

Kind: production

**This task closes [#199](../tickets/corpus-coverage.md#t199)** (a global aggregate over no
arrival drops its identity row) **and [#282](../tickets/corpus-coverage.md#t282)** (a scan with no
surviving row groups is refused at planning). Second of chain L, after grouping-id and before
aggregate-arms and welford-device; the last builds its keyless Welford on this task's done call.

## Why it happens

A keyless aggregate owes one row over any input: `SELECT count(*) FROM t WHERE false` is `0`, and
`SELECT sum(x), min(x) FROM t WHERE false` is one row of NULLs. Two sites break it.

- **The init over no batch.** A `GpuAggregate` is `Exec`, so a lane that receives no batch makes
  no call and answers nothing. The merge above then folds fewer rows than lanes, and over none
  at all it answers nothing on the device (`gpu_backend/accumulate.rs`) and, on the cpu, a merged
  `count` of NULL (`cpu_backend/accumulate.rs`, `mark_done_and_fetch`'s `!self.grouped`
  clause), which a non-nullable declaration refuses. That is q96, q88 and q90 at the three tp4
  modes: an empty lane comes from tp4-single cutting a one-row-group table into four, or from an
  Inner join with no build rows in a lane.
- **The single-node shortcut** (`translator/aggregate.rs`): a keyless `GpuAggregate` over a
  single-batch input finalizes itself, and when no batch arrives it is never called. Symmetric,
  so no cpu-vs-device comparison sees it.

A third break is the init's own: called once per batch, it answers one row per call, a zero-row
batch included. Adding a zero-row batch adds a row to its output, which `architecture.md`'s
"Zero-row batches change no answer" forbids. The merge absorbs the extra row, so no answer is
wrong; the node's rows are.

## The work

1. **What each aggregator owes over no rows.** `empty_state(agg, input) -> Result<ScalarValue,
   PlanError>` in `plan/aggregates.rs`, beside `state_type` and shaped like it, with a
   `PlanAgg::empty_state` method beside `PlanAgg::state_type`. No wildcard:

   | `PlanAgg` | empty state |
   |---|---|
   | `Count` | `Int64(Some(0))` |
   | `Mean`, `M2` | `Float64(Some(0.0))` |
   | `Sum`, `Min`, `Max` | the typed NULL of `state_type(agg, input)`, `ScalarValue::try_from(&DataType)` |
   | `MergeM2` | unreachable: it merges the Welford triple and declares no state |

   Its doc says why `Mean` is `0.0` and not NULL: `Mean` exists only inside the Welford triple,
   and DataFusion 45's `VarianceAccumulator::state` answers `(0, 0.0, 0.0)` over no rows. Neither
   backend builds a row from it: each engine computes its own identity, and the tests hold both to
   this table.
2. **The category.** `category_of` (`plan/mod.rs`) makes every `GpuAggregate` a
   `BatchAccumulator`, as `GpuLimit`, which also streams and holds nothing, already is. One node,
   one category: the mapping stays by node, not by body. The driver already makes `MarkDone` on
   every `BatchAccumulator` lane, one that saw no batch included, so it does not change.
3. **The recipe** (`wire/attach.rs`, `aggregate()`). One new `CallPattern`,
   `AtDoneIfNothingOut` ("at done, if nothing went out"). Not `AtDone`: that reads as always,
   once, at the end, and the walk tests run every `AtDone` call after the batches, so a keyless
   walk would add an identity row after its real one. Every init's recipe:

   ```
   Aggregate(init)    [batch]         per batch
   Project(finalize)  [prior output]  per batch                     (shortcut only)
   Aggregate(init)    []              at done, if nothing went out
   Project(finalize)  [prior output]  at done, if nothing went out  (shortcut only)
   ```

   The per-batch calls stay `PerBatch`: the recipe says which seq a batch goes to, and whether a
   given batch needs the call is the executor's (next item), as a limit's slice stays
   `PerStraddlingBatch` though a batch wholly inside its interval makes none. The done call reuses
   the init's seq; no second wire node. Only the aggregate's executors accept the new pattern;
   `wire/recipes.rs`, which renders recipes, prints a call with no input and a reused seq cleanly.
   Grouped and keyless share the recipe: what the done call answers is the engine's (next item),
   not the planner's.
4. **The executors.** Each backend's init is a batch accumulator holding one flag, `emitted`.
   `executors_for(…, lane)` builds one per node per lane, so the flag is one lane's, and nothing
   else keeps it: not the driver, the recipe or C++, whose node stays stateless.
   `accumulate_and_fetch` checks the row count first: a zero-row batch is dropped, which releases
   its handle, and answers nothing with no ABI call (`no_abi_calls()`), as the limit's executor
   releases a batch outside its interval (`gpu_backend/accumulate.rs`). The driver still records
   the `Accumulate`, with an empty call list; the trace and the cost registry record calls made, so
   the skip shows as no row. Otherwise it makes the init call (and the finalize call) and answers
   its output, setting `emitted` when that has rows: the device reads the row count the call
   already returns (`produced`), the cpu `RecordBatch::num_rows()`.
   `mark_done_and_fetch` makes the done call(s) when `emitted` is false and answers their output
   if it has rows, nothing otherwise. Over zero rows a keyless init answers its identity row and a
   grouped one no groups, so a grouped lane that saw no rows emits nothing — each engine's own
   answer (cuDF's groupby, DataFusion's `AggregateExec`), with no keyless case in either executor.
   A grouped init stops emitting a zero-row batch per zero-row input. The cpu (`CpuAccumulator`,
   built in `cpu_backend/backend.rs`) runs the init over
   `RecordBatch::new_empty(input schema)`; the device's done call hands the seq no input; the
   driver's mock (`executor/driver/tests/mock.rs`) scripts the new category. The walk tests
   (`wire/gpu_tests/mod.rs`), which drive recipe calls with no executor, keep the same flag per
   lane: `phases` puts `AtDoneIfNothingOut` on the done side, and the walk runs it for a lane only
   where that lane's per-batch calls returned no rows.
5. **No input on the device.** `execute_aggregate` (`cpp/src/operators/aggregate.cpp`), handed no
   input, builds a zero-row table from the node's `aggr_input_schema` (on the wire today, read by
   nothing) through `fb_to_type_id` (`cpp/src/peacock/expr.h`, already exposed), and runs as it does over any
   input: the keyless `cudf::reduce` path answers one row, the groupby no groups. Any other node
   handed no input still refuses.
6. **The merge's clause goes.** A keyless merge now always meets at least one row per lane, so
   the cpu's `!self.grouped` clause, and its `grouped` field, go; the device's merge is unchanged.
7. **#282, a scan of nothing.** `source()` (`planner/translator/nodes.rs`) maps a scan with no
   surviving row groups — every one pruned, or a file with none — to one lane that holds no batch
   (`partition_groups = [[]]`), rather than calling `partition()`, which keeps its refusal of an
   empty survivor list for every other caller. The lane's source executors make no call: the cpu's
   `CpuSource` and the device's source step one call per mapped batch, so none means the lane ends
   with nothing, the arrival 1–6 make every node answer right. The validator needs no change: it
   already accepts a lane with no batch (`plan/source.rs`, "four lanes over two row groups leave
   two of them empty") and refuses only a scan with no lane, a batch reading no row group, or one
   reading a pruned group. The memory estimate prices the lane at zero. The wire never sees an
   empty mapping: no call is made to carry one.
8. **`architecture.md`.** "Zero-row batches change no answer" loses its first break; the
   categories list the aggregate's init as a batch accumulator; the call patterns list the new one;
   "a lane that receives nothing is never runnable" and "an empty lane emits no batch at all" are
   corrected: such a lane gets its `MarkDone`, and a keyless init's emits its identity row. An empty
   grouped answer reaches the sink as nothing, and chain K's empty-sorts (in this chain's base)
   answers it with one zero-row batch under the sink's schema, so `empty-grouped-count` compares
   with DuckDB exactly.

## Corpus

Six pbench queries, in `testdata/pbench-queries/`, reaching the three ways an init meets nothing.
The first four filter `tiny` with `t_id + t_v < 0`, which holds for no row and which no row-group
statistic can prune: the scan reads `tiny`'s one row group and the filter hands the init a zero-row
batch, on the one lane at tp1, and at tp4-single on one lane with three left no batch (`tiny` is
under the small-table size, so tp4-rowgroup and tp4-sized plan it one lane and reach the zero-row
batch only). The last two scan
nothing at all (#282): pbench's `empty` table has no row group, and `t_v < 0` prunes `tiny`'s one.
Every answer checked with DuckDB 1.5 on 2026-10-08 (the first four over `empty`, which answers as
the filter does):

| name | SQL | DuckDB | off on |
|---|---|---|---|
| empty-all-null-aggregates | `SELECT sum(t_v) AS s, min(t_v) AS lo, max(t_v) AS hi FROM tiny WHERE t_id + t_v < 0` | one row: NULL, NULL, NULL | none |
| empty-count-aggregates | `SELECT count(*) AS n, count(t_v) AS nv, sum(t_v) AS s, avg(t_v) AS a FROM tiny WHERE t_id + t_v < 0` | one row: 0, 0, NULL, NULL | none |
| empty-dispersion-aggregates | `SELECT stddev_samp(t_v) AS sd, var_pop(t_v) AS vp FROM tiny WHERE t_id + t_v < 0` | one row: NULL, NULL | gpu: #216, turned on by welford-device |
| empty-grouped-count | `SELECT t_k, count(*) AS n FROM tiny WHERE t_id + t_v < 0 GROUP BY t_k` | no rows | none: a grouped init owes nothing at done |
| empty-table-count | `SELECT count(*) AS n, sum(t_v) AS s FROM empty` | one row: 0, NULL | none: a file with no row group (#282) |
| empty-pruned-count | `SELECT count(*) AS n, min(t_v) AS lo FROM tiny WHERE t_v < 0` | one row: 0, NULL | none: `t_v` is `i * 5`, never negative, so the statistics prune `tiny`'s one row group (#282) |

Each gets its plan goldens, cpu sections, DuckDB section, `corpus_cases.inc` line and registry row,
as pbench's queries do. pbench's `cross-empty-build` and `outer-on-true-empty`, refused at planning
on #282 until now, then meet the joins they test; each cell runs and takes the ticket it fails on. `max(t_pat)` is left out: a string min or max on the device is #195's.

The rows tagged `199` — q96, q88 and q90 at the tp4 modes, q32 at tp4, and any other carrying it
when this task builds — are run at those modes and enabled if they pass; a failing cell takes its
ticket. `199` is struck from their tags, and the `corpus_cases.inc` comments naming #199 are
reworded.

## Scope

| path | change |
|---|---|
| `peacockdb-core/src/plan/aggregates.rs`, `plan/mod.rs` | `empty_state`; `category_of` |
| `peacockdb-core/src/wire/mod.rs`, `wire/attach.rs`, `wire/recipes.rs` | the new pattern; the init's recipe; rendering a call with no input |
| `peacockdb-core/src/wire/gpu_tests/mod.rs` | `phases` runs the new pattern per lane, only where nothing went out |
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs`, `cpu_backend/backend.rs` | the init accumulator; the merge's clause goes |
| `peacockdb-core/src/executor/gpu_backend/accumulate.rs`, `gpu_backend/backend.rs` | the init accumulator |
| `peacockdb-core/src/executor/driver/tests/` | the mock and the lane cases |
| `cpp/src/operators/aggregate.cpp`, `cpp/tests/` | no input → zero-row table |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_dimension_cases.rs` | the identity cases |
| `testdata/goldens/*/*.plans.txt`, `testdata/goldens/recipe-payloads.txt` | every init's recipe and category |
| `peacockdb-core/src/planner/translator/nodes.rs`, the source executors, the memory estimate | #282's scan of nothing |
| `testdata/pbench-queries/empty-*.sql`, their goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the six queries; the `199` and `282` rows |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/` | as in the work; counts; #199 and #282 archived |

Component-level API: `ExecutorCategory` of `GpuAggregate`; one `CallPattern` variant, which
prints in the plan text. No facade, trait, ABI or wire change: the done call is an existing
seq handed no input.

## Restriction

The init, its recipe and the merge's clause. The merge's device code,
the limit (#214) and the cpu's sort and merge over zero-row batches (#205) do not change: below the
init, a source of nothing needs no fix of its own for this beyond #282's scan. The keyless Welford on the device stays as it is: a
finished `stddev`, a refused `var` (#216, welford-device).

## Tests

- #282: `select count(*) from nation where n_nationkey < 0;` (tpch) plans one lane with no batch at
  every mode and answers `0` on the cpu; a scan with survivors maps as before.
- `empty_state`: for every init `PlanAgg` over `Int32`, `Int64`, `Float64`, `Decimal128(15, 2)`
  and `Date32` where `state_type` accepts it, the value's `data_type()` is `state_type`'s.
- `category_of`: `GpuAggregate` → `BatchAccumulator`. The init's recipe: four calls and their
  patterns, grouped and keyless alike.
- Driver (mock): a keyless lane with no batch makes one done call and answers one row; one with
  only zero-row batches makes no init call and one done call; one with batches with rows makes no
  done call; zero-row batches interleaved among them change no row. A grouped lane with no batch,
  and one with only zero-row batches, makes one done call and emits nothing.
- Both backends, per `AggFunc`, through `decomposition()`: a keyless init over no batch and over
  two zero-row batches answers the row `empty_state` gives, and the shortcut answers it finalized
  (count 0, the rest NULL). For `Stddev` and `Var` the cpu half is here and the device half is
  welford-device's.
- `bug_a_global_merge_over_no_arrival_answers_nothing_on_the_device` becomes
  `a_global_merge_over_no_arrival_answers_nothing_on_both`: no merge meets no arrival now, and
  neither backend invents a row for one.
- A walk test with a lane that receives no batch: a keyless aggregate answers its identity row
  there and its one row elsewhere, matching DataFusion; a keyless walk over lanes with rows makes
  no done call.
- gtest, in a new `cpp/tests/gpu/` file (`test_plan_executor.cpp` is past the 1000-line cap):
  `execute_aggregate` handed no input answers one row, typed as declared, for a keyless
  node, and zero rows for a grouped one.

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: `ctest -L cpu` locally against cuDF 25.02.
- device: `gpu_tests::`, the walk tests, and `test_gpu_corpus` over the `199` and `282` rows and the six
  pbench queries, on the GPU host the chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.
