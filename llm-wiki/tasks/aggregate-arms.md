# The device aggregate reads every column by reference and names every column from the plan

Kind: production

**This task closes [#164](../tickets/corpus-coverage.md#t164)** (a column ordinal reaches cuDF
unchecked, and a bad one degrades rather than throws), **[#225](../tickets/corpus-coverage.md#t225)**
(the device names every Welford state column by the same alias) **and
[#280](../tickets/corpus-coverage.md#t280)** (a `stddev` or `var` under a grouping set answers one
column where the plan declares three). Third of chain L, after keyless-identity and before
welford-device, which builds the keyless Welford on the one request builder this task leaves.

## Why it happens

The wire's rule is that a node reads every column through a `ColumnRef { index, name }` and names
every column it emits from the plan's declared schema. `execute_aggregate`
(`cpp/src/operators/aggregate.cpp`) keeps it for an init's arguments and its group keys, and breaks
it everywhere else.

- **Inputs.** The grouped Merge arms read state with a running cursor, `in_off`, and recover each
  function's width from its name, from `mergeable_agg_state` (a stddev takes 1 or 3 columns) and
  from the residual column count (an avg takes 1 or 2) (`:491-532`, `:705-738`). The plan already
  puts a `ColumnRef` to each state column in every merge call's `args`
  (`translator/aggregate.rs:173-194`), and the writer sends them; the grouped arms ignore them,
  while the keyless merge reads them (`get_values_col`). Two rules for one input.
- **Outputs.** A function's columns are named from its `alias`. The init's three Welford calls are
  folded into one function node whose alias is the SQL output name, so the triple holds three
  columns under one name (#225). `ddof` is re-derived from that name (`stddev_ddof`), which the
  plan holds (`reports/hacks-audit.md` §5).
- **Three paths, three rules.** The grouped path builds the Welford triple when
  `mergeable_agg_state` is set; the grouping-set path (`:363-423`) does not and answers one
  finished `STD` or `VARIANCE` column (#280); the keyless path reduces by name (#216, welford-device).
- **Dead arms.** The writer sends `Partial` and `Merge` only (`aggregate_writer.rs`), never `avg`
  (decomposed into sum and count), and sets `mergeable_agg_state` whenever a Welford is present.
  So every Final and Single arm, every `avg` arm, the non-mergeable stddev path, `get_values_col`'s
  Final branch and the Final guards are unreachable — the 2026-10-08 column-indexing audit.
- **Unchecked ordinals (#164).** `TableResult` (`cpp/src/plan_executor.h`) holds a table and a
  name vector with nothing tying their lengths; the name sites use `operator[]` (`filter.cpp:42`,
  `join.cpp:260,338,343`, `aggregate.cpp:169`), so a short vector is undefined behaviour.
  `infer_expr_type` (`expr.cpp:330`) answers `type_id::EMPTY` for an out-of-range `ColumnRef`,
  surfacing later as a confusing type error. No C++ reader checks a `ColumnRef`'s name; the cpu
  does (`expr_physical.rs:45`).

## The work

1. **One request builder.** A function node and the node's phase give its cuDF requests and its
   output columns, in one place, used by the grouped path and once per set by the grouping-set
   path:

   | function | Init (`Partial`) | Merge |
   |---|---|---|
   | `sum`, `min`, `max` | that aggregation over `args[0]` | that aggregation over `args[0]` |
   | `count` | `COUNT` over `args[0]`, widened to `INT64` | never sent: the plan merges a count with `sum` |
   | `stddev`, `stddev_pop`, `var`, `var_pop` | `COUNT`, `MEAN`, `M2` over `args[0]` cast to `FLOAT64` | `MERGE_M2` over the struct of `args[0..3]` |

   Every input is the `ColumnRef` its `args` carry; a node's state is never located by counting.
   A function node with no `args`, or an `args` of the wrong length for its function, is refused
   naming the function. The grouping-set path, built on it, emits the Welford triple (#280). The
   keyless path keeps `cudf::reduce` for sum, min, max and count, its merge already reading `args`;
   its Welford arm stays as it is — a finished `stddev`, a refused `var` (#216, welford-device).
2. **Names from the plan.** `AggregateFuncNode` (`flatbuffers/gpu_plan.fbs`) appends
   `state_names: [string]` and `ddof: int8`. `state_funcs` (`plan/aggregate.rs`) fills
   `state_names` for every function from its calls' `outputs` — one name for a plain function,
   three for a folded Welford — and `ddof` from `AggStateColumns.ddof`. `execute_aggregate` names
   every state column from `state_names` and reads `func->ddof()`; `stddev_ddof` goes. `alias` and
   `mergeable_agg_state` are deprecated, no slot moved, as chain K deprecated `distinct`; the
   writer stops filling them.
3. **The dead arms go.** `agg_phase` maps `Partial` and `Merge` and refuses the rest, naming the
   mode. The Final, Single and `avg` arms, the non-mergeable stddev path, `make_agg`'s `is_final`,
   `get_values_col`'s Final branch, the width recovery and its check, `has_stddev_or_var_final` and
   its guard, and the keyless Final guards go. `is_avg_name` and `avg`'s `out_decimal_*` handling go
   with them; the two fields stay on the wire, unwritten, as today.
4. **#164.** Every `TableResult` is built through a constructor that refuses a table whose column
   count differs from its name count — chain J's refcounted-scatter adds one
   (`TableResult::owning`); the sites still assembling a `TableResult` by hand go through it. One
   helper, `column_at(table, names, ref)`, resolves a `ColumnRef`: an index past the width throws
   naming the index and the width; a missing name throws; a name other than `names[index]` throws
   naming both. Every `ColumnRef` reader in `cpp/src` resolves through it — `expr.cpp`'s
   `build_column`, `build_expr`'s AST arm and `infer_expr_type` (which throws rather than answering
   `EMPTY`), `project.cpp`, `sort.cpp`, the join's keys and residual (in `join_session.cpp` once
   join-backend has deleted `join.cpp`), the sort-preserving merge's keys and the repartition keys
   (`node_session.cpp`), `aggregate.cpp`'s keys and arguments. The expression evaluators
   (`build_column`, `evaluate_column`, `cudf_ast_can_evaluate`) take the input's names, which they
   do not today. Every name site indexed with `operator[]` uses `.at()`.

   The name check is safe to turn on: the device corpus's schema validator already holds every
   batch a node emits to its node's declared names on every enabled cell, and the one known
   exception, the Welford triple's (#225), is fixed in 2. A cell that the check refuses is a
   wrong reference found, ticketed as such.
5. **The scan's names, checked at plan time.** The device's scan reads parquet columns by name:
   the writer sends the declared output schema as `file_schema` and no `projection`
   (`wire/node_writer.rs`, `scan`), and `scan.cpp` asks cuDF's reader for those names. The cpu reads
   the plan's file ordinals (`cpu_backend/source.rs`). The two agree only while each declared name
   is the file's column name at its ordinal, which holds for every table registered from its own
   file and breaks for one whose declared schema renames or reorders the file's columns: the
   device then reads another column, or fails. `survivor_metadata`
   (`planner/translator/scan_mapping/parquet_meta.rs`), which already opens the file at plan time,
   refuses a scan where the file's root field name at a projection ordinal differs from the
   declared output field at that position, naming the ordinal and both names. No wire or C++
   change: the name-based read is then safe by construction.

## Corpus

A new tpch query, `testdata/tpch-queries/rollup-stddev.sql`, puts a Welford under a grouping set,
which neither benchmark has:

```sql
-- A stddev and a var under a rollup: the grouping-set path builds the Welford triple (#280).
select l_returnflag, l_linestatus, stddev_samp(l_quantity), var_pop(l_quantity)
from lineitem
group by rollup (l_returnflag, l_linestatus);
```

Its oracle and golden are `shuffle_stddev`'s (`data_fusion_approximate`, `golden_approx_std`); its
five plan goldens, cpu sections, DuckDB section, `corpus_cases.inc` line and registry row (features
`rollup stddev_var`) are written, schema validation enabled. Every cell enabled if it passes; a
failing one takes its ticket.

`tpch/shuffle-stddev` goes to `schema_validation_enabled`, and its `corpus_cases.inc` comment
naming #225 goes; its registry row carries no `225`. Every other
corpus cell keeps its state: nothing else this task touches changes an answer.

## Scope

| path | change |
|---|---|
| `flatbuffers/gpu_plan.fbs` | `state_names`, `ddof` appended; `alias`, `mergeable_agg_state` deprecated |
| `peacockdb-core/src/plan/aggregate.rs`, `wire/aggregate_writer.rs`, `wire/fb_text.rs` | the fields filled, written and printed |
| `cpp/src/operators/aggregate.cpp` | the builder, the names, the dead arms gone |
| `cpp/src/plan_executor.h`, `cpp/src/expr.cpp`, `cpp/src/operators/{project,sort,join,filter}.cpp`, `cpp/src/node_session.cpp`, and every `TableResult` construction | #164 |
| `cpp/tests/` | the builder per function and phase; #164's refusals |
| `peacockdb-core/src/planner/translator/scan_mapping/parquet_meta.rs`, its tests | the scan's name check |
| `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs`, `aggregate_schema_cases.rs` | the #225 pins flip; a rollup Welford case |
| `testdata/goldens/recipe-payloads.txt` | every aggregate payload |
| `testdata/tpch-queries/rollup-stddev.sql`, its goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the new query; shuffle-stddev |
| `llm-wiki/architecture.md` (the aggregate's wire fields, the C++ aggregate), `build-test.md`, `tickets/` | as in the work; counts; #164, #225 and #280 archived |

Component-level API: the wire format — two fields appended to `AggregateFuncNode`, two deprecated,
no slot moved. No facade, trait or ABI symbol change.

## Restriction

The aggregate's inputs, names and dead arms, #164's checks and the scan's name check. The keyless
Welford (#216), the `MERGE_M2` count type (#94) and all-NULL Welford groups are welford-device's.
Dead code outside the aggregate (`window.cpp`, `limit.cpp`, `union.cpp`), output names other nodes
copy from their input, and the device scan's read by name (checked at plan time instead, 5) do not
change. `__grouping_id` keeps its C++-built name, which is the plan's.

## Tests

- gtests, per function and phase of the builder's table: the requests made and the columns named
  from `state_names`, a `ColumnRef` that is not the next column in the input read where it points,
  and a function whose `args` have the wrong length refused.
- `bug_stddev_holds_three_identically_named_columns` and
  `bug_stddev_merge_holds_three_identically_named_columns` (`aggregate_schema_cases.rs`) assert
  the three declared names, under names without `bug_`.
- A rollup with a `stddev`, a `var` and a `sum` after them, init and merge: both backends agree, and
  the device holds the declared schema.
- #164: a `TableResult` of three columns and two names refused (it needs device memory, so it runs
  with the device tier); a `ColumnRef` past the width, one
  with no name, and one whose name is another column's, each refused with its own message, through
  `build_column` and through a sort key.
- `agg_phase` over `Final` and `Single` refused.
- The scan check: a table registered over a parquet file with a declared schema that renames one
  column, or swaps two of the same type, is refused at planning, naming the ordinal and both
  names; every corpus scan plans as before.
- Every existing aggregate case, walk test and corpus cell answers as before. The gtests that
  build a `ColumnRef` with no name, the Single-mode count tests with no args and the `avg` merge
  test are rewritten to the plan's shapes: they test arms this task removes or references it now
  checks.

## Verification bar

- rust-only: `--lib` (payloads), `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the
  registry tests both ways.
- ffi: `--lib -- ffi_tests::` and the payload test, through `scripts/cargo-cudf.sh`.
- C++: `ctest -L cpu` locally against cuDF 25.02; the 26.02 CI build leg green.
- device: every `peacock_*_tests` gtest binary, `gpu_tests::`, the walk tests, and the whole
  `test_gpu_corpus` (the name check touches every node), on the GPU host the chain header names.

## Device workflow

One GPU cycle per round on the chain header's host, the working tree synced as the header says.
