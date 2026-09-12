# typed-nulls — run record

Spec: [`typed-nulls.md`](typed-nulls.md). Plan: [`typed-nulls-impl.md`](typed-nulls-impl.md).
Branch `ENS-typed-nulls` off `ENS-declared-schemas` at `b86805b3`; PR against it when reviewing.

### 2026-09-12 — building: plan task 1 dispatched

The gtest literal helpers first, since every later test builds a literal through them. The
developer also settles whether `PCK_TEST_FILTER` reaches the gtest binaries and records it here.
#198's amendment stands over the spec: the second pin of task 9 shows a bare typed null in a
select list is a column of zeros, so the spec's "a bare literal short-circuits to null" is false
and plan task 8's test asserts what the device does after the fix, not what the spec assumed.

### 2026-09-12 — plan tasks 1–3 done: helpers repaired, two red tests on the device

**Task 1, the helpers.** `make_int64_literal` and `make_float64_literal`
(`cpp/tests/gpu/test_plan_executor.cpp`) now go through `fb::ScalarValueBuilder` and add the
field they name; `make_null_literal(fbb, type)` sits beside them and sets `is_null` with no
value, which is what `serialize.rs:92` writes for a non-decimal null (a null decimal also carries
precision and scale — plan task 5's helper, if it needs one). The plan's assumption held: the
generated `CreateScalarValue` is `(fbb, type, is_null, bool_val, int_val, uint_val, float_val,
…)`, so the old calls put `val` in `bool_val` (int) and in `uint_val` (float).

**Moved assertions, one line per helper call site** (there are three, in two tests):
- `PlanExecutor.FilterNation` — the spec and plan call it `FilterNationByRegion`; no test of
  that name exists, the `> 2` claim is `FilterNation`'s. It ran `n_regionkey > 0` (20 rows) and
  now runs `> 2` (10 rows). Its assertion `0 < rows < 25` held on both, so nothing failed on the
  device either way — which is the spec's finding: a range a wrong literal also satisfies. It now
  asserts `== 10` (regions 3 and 4, five nations each, checked against `nation.parquet` with
  duckdb locally and green on the run below). A tightening, not an adjustment to pass; the
  coordinator can drop it if it is judged beyond the task.
- `PlanExecutor.ProjectSqrtThroughTheColumnPath` — `make_int64_literal(fbb, 0)` and
  `make_float64_literal(fbb, 0.0)` built 0 and 0.0 before and after, so nothing moves; the
  `otherwise` arm is never taken on region anyway.

**Tasks 2 and 3, the red tests.** `Literals.ATypedNullInsideAnAstExpressionIsNullAndNotZero`
(project of `CAST(n_nationkey AS Int64) + NULL::Int64`, asserts `null_count == size`, size
pinned at 25) and `Literals.AComparisonAgainstATypedNullKeepsNoRows` (filter
`CAST(n_nationkey AS Int64) = NULL::Int64`, asserts zero rows). Both reuse the file's
`nation_scan_node`. They are in `test_plan_executor.cpp` (now 1470 lines, +93) as the plan's
tasks 2–8 all name that file; the ~150-line sibling threshold is for whoever adds task 5–8's
tests to decide, and a sibling would need the static helpers and `WholePlan` hoisted into a
header plus a CMake target, which no task has asked for yet.

Run `20260912T114700-371238` (`--build` rc 0 with 0 warnings, `--push-binaries --patch` rc 0,
`--run-detached`, gate exit 1 by design; every rmm pool built, 103 GiB free):
- `peacock_cpu_tests` 12 passed; `peacock_gpu_tests` 6 passed; `peacock_tpch_tests` 4 passed;
  `peacock_tpchv_tests` 4 passed.
- `peacock_plan_tests` 29 ran, 27 passed, 2 failed — the two new ones, exactly as the plan
  predicted: `view.null_count()` `Which is: 0` against `view.size()` `Which is: 25` at
  `test_plan_executor.cpp:1421`; `result.table->num_rows()` `Which is: 1` against `0` at `:1444`
  (the nation whose key is 0, so the `== 0` rather than `< 25` matters). `FilterNation` OK.
- rust, `PCK_TEST_FILTER=wire::gpu_tests::declared`: `peacockdb_core_gpu_lib` 15 passed, 1
  ignored, 837 filtered out; `test_gpu_corpus` 0 run, 8 filtered out.

**`PCK_TEST_FILTER` does not reach the gtest binaries.** `build-test-shadgpu.sh:353` runs each
`peacock_*_tests` with no arguments; the filter is forwarded only in the rust loop (`:374`
onward, through `rung_args` at `:383`). So the C++ suites run whole every cycle (about 50 s of device time, the sf40 pair
most of it), and the rust half is kept short with a real narrow filter such as
`wire::gpu_tests::declared`. `PCK_RUN_CPP=0` would skip the gtests, so it is not set on this
task. A filter matching no rust test trips the zero-test guard.

`build-test.md`: Plan-executor row 27 → 29 with the literal tests named in its text and a new
example link; C++ 67 → 69 and the grand total 1778 → 1780 moved with it (the header is the sum
of the N columns). No other wiki page counts these tests. `cpp/src/expr.cpp` untouched.
`peacock_plan_tests` stays red on this branch until plan task 4 lands the fix.
