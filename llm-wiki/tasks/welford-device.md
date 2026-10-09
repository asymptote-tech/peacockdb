# The device's Welford state: keyless, NULL-free and version-gated

Kind: production

**This task closes [#216](../tickets/corpus-coverage.md#t216)** (the device's global aggregate has
no Welford arm) **and [#94](../tickets/corpus-coverage.md#t94)** (MERGE_M2 count-child type is
cuDF-version-specific). Fourth and last of chain L: after keyless-identity, whose done call is this
task's only zero-row keyless call, and after aggregate-arms, whose one request builder the keyless
Welford runs through and which has already named the triple (#225) and taken the `ddof` from the
wire.

## Why it happens

**#216.** `execute_aggregate`'s keyless path (`key_cols.empty()`) reduces a stddev name with
`make_std_aggregation` whatever the phase: at the init the finished sample stddev of the argument,
at the merge the stddev of the state's first column, the count. The finalize project above then
fails (`ColumnRef index 2 out of range (cols=1)`), and a `var` name is refused by
`make_reduce_agg`. So `SELECT stddev(x) FROM t` and `SELECT var(x) FROM t` are refused on the device
in every shape. cuDF has `M2` and `MERGE_M2` for groupby only.

**#94.** The Merge arm's `MERGE_M2` casts the count child to `INT32`, which cuDF 25.02's
`group_merge_m2` requires; 25.06 and later (cuDF PR #18546) accept only `INT64` or `FLOAT64`. The 26.02 CI leg is
build-only, so nothing fails until a GPU runs a later cuDF.

**All-NULL groups.** DataFusion's Welford state over a group whose values are all NULL is
`(0, 0.0, 0.0)`, never NULL (`VarianceGroupsAccumulator::state` builds its arrays with no null
buffer). cuDF's `MEAN` and `M2` over no valid values are not pinned and may answer NULL. The answer
is right either way, since the finalize answers NULL wherever `count − ddof ≤ 0`. The state is what
differs, and a NULL child in a `MERGE_M2` input beside other lanes' real entries is cuDF's to skip
or to poison.

## The work

1. **#216, the keyless Welford.** A keyless node holding any stddev or var runs through
   aggregate-arms' grouped request builder on one constant `INT32` key column of the input's row
   count, dropped before return. The rest of the node's aggregates (a `sum` or `count` beside it)
   ride along, as in a grouped node. Every other keyless node keeps the `cudf::reduce` path, the
   cheaper pass over raw rows. The `is_std` reduce arm goes.
2. **#216, zero rows.** After keyless-identity, the only keyless call over zero rows is the done
   identity call, and a groupby over zero rows has no groups. So a routed node over zero rows
   builds its one row: `count` 0 (`INT64`), each Welford triple `(0, 0.0, 0.0)`, every other column
   the typed NULL of the type the groupby answers for it over zero rows (the wire's
   `CudfAggregate` carries no output schema; a decimal keeps its scale), named from `state_names`. This is `empty_state`'s table
   (`plan/aggregates.rs`), written once in C++.
3. **All-NULL groups.** The Welford init, grouped, grouping-set and keyless alike (the builder is
   one), applies `cudf::replace_nulls(…, 0.0)` to `$mean` and `$m2`, always. So does every Merge:
   cuDF's `MERGE_M2` itself answers NULL moments for a group whose merged counts are all 0
   (`group_merge_m2.cu`, `is_valid = count > 0`, in 25.02 and 25.10 alike), so a per-lane merge
   would hand NULL children to the cross-lane one. The state is then DataFusion's at every step,
   and no `MERGE_M2` meets a NULL child. The zero-row row of 2 already holds `(0, 0.0, 0.0)`.
4. **#94.** `aggregate.cpp` picks one `constexpr` count type from cuDF's own
   `CUDF_VERSION_MAJOR` and `CUDF_VERSION_MINOR` (`<cudf/version_config.hpp>`, shipped in 25.02 and
   26.02 alike; no CMake define, which would be a second source of the version), `INT32` before
   25.06 and `INT64` from 25.06, and the one `MERGE_M2` site aggregate-arms leaves uses it. Chain
   J's verify-26.02 runs `shuffle-stddev` on 26.02 and may already have gated this site the same
   way; if so, this item only confirms it covers the site aggregate-arms leaves, and #94 is
   archived with verify-26.02 rather than here. The widening back to `INT64` after the merge
   stays, a no-op from 25.06. No runtime probe: each cuDF version is its own build. The gate goes
   when 25.02 does.

## Corpus

A new tpch query, `testdata/tpch-queries/global-stddev.sql`, puts a keyless stddev and var in the
corpus, which no query has:

```sql
-- A keyless stddev and var: the device's Welford over no keys (#216).
SELECT stddev_samp(l_quantity) AS sd, var_samp(l_quantity) AS v FROM lineitem;
```

It reads lineitem whole, so the tp4 modes merge four lanes' states. Its oracle and golden are
`shuffle_stddev`'s (`data_fusion_approximate`, `golden_approx_std`); its five plan goldens, cpu
sections, DuckDB section, `corpus_cases.inc` line and registry row (features `stddev_var`) are
written. Every cell enabled if it passes.

pbench's `empty-dispersion-aggregates` (keyless-identity) has its gpu cells turned on and `216`
struck. Any other row tagged `216` or `94` is run at its off modes and enabled if it passes; a
failing cell takes its ticket.

## Scope

| path | change |
|---|---|
| `cpp/src/operators/aggregate.cpp`, `cpp/tests/` | the keyless route, the zero-row row, the NULL replacement, the count type |
| `peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs`, `aggregate_schema_cases.rs`, a new `welford_cases.rs` (`aggregate_cases.rs` is near the 1000-line cap) | the #216 pins flip; the zero-row and all-NULL cases |
| `testdata/tpch-queries/global-stddev.sql`, its goldens and sections, `corpus_cases.inc`, `testdata/cost-registry.csv` | the new query; the rows above |
| `llm-wiki/architecture.md` (Welford), `build-test.md`, `tickets/` | as in the work; counts; #216 and #94 archived; #261's "after #216" line |

Component-level API: none. No facade, trait, ABI or wire change.

## Restriction

The keyless route, the zero-row row, the NULL replacement and the count type. The keyless
`cudf::reduce` path for every other aggregate is unchanged. A Welford companion beside a DISTINCT
stays refused ([#261](../tickets/complete-coverage.md#t261)). No 26.02 device run: that is
verify-26.02's and [#260](../tickets/system-hardening.md#t260)'s.

## Tests

- The #216 pins in `aggregate_dimension_cases.rs` —
  `bug_a_global_welford_init_answers_a_finished_stddev_on_the_device`,
  `bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device`,
  `bug_a_global_stddev_finalize_is_refused_on_the_device`,
  `bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device` — and
  `bug_a_global_stddev_holds_one_finished_float64_where_the_plan_declares_the_welford_state`
  (`aggregate_schema_cases.rs`) become cpu-vs-device agreement cases, `holds_as_declared` where
  they read the handle.
- keyless-identity's identity cases for `Stddev` and `Var` gain their device half: no batch and
  zero-row batches, init and shortcut, both backends agree with `empty_state`.
- A keyless `stddev` beside a `sum` and a `count`, init and merge, both backends.
- All-NULL groups, grouped and keyless, both backends: the init's state is `(0, 0.0, 0.0)` with
  no NULL, on the device as on the cpu. Across lanes: one lane holds a group's values and another
  only NULLs for it; the merged and finalized answer matches the cpu's. A group all NULL on every
  lane finalizes to NULL.
- gtest: the count type is `INT32` under 25.02's `version_config.hpp` (the 26.02 CI leg compiles the other
  branch).

## Verification bar

- rust-only: `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`; the registry
  tests both ways.
- C++: `ctest -L cpu` locally against cuDF 25.02; the 26.02 CI build leg green.
- device: `gpu_tests::`, the walk tests, and `test_gpu_corpus` over `shuffle-stddev`,
  `rollup-stddev`, `global-stddev`, `empty-dispersion-aggregates` and the rows above, on the GPU
  host the chain header names.

## Device workflow

As the chain header says: one sync per task to its host, with as many back-to-back builds as its
red/green pairs need, the red build first.
