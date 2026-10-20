# aggregate-arms — working record

## Standing facts

- Seventh of chain K (folded in from chain L), branch `ENS-aggregate-arms` off
  `ENS-keyless-identity`; its PR targets `ENS-keyless-identity`. Base master `ca4dedeb`; chain J's
  refcounted-scatter (`TableResult::owning`) and join-backend (`join.cpp` deleted, the join in
  `join_session.cpp`) are in it.
- Hosts, per chain K's board note: device work on nebius-gpu, tree `~/peacockdb-K`
  (`gpu-host-sync.sh --dest peacockdb-K`), the L40S list only; cpu runs local (verda not
  reachable). The final device run is the dispatched `GPU Tests (nebius-gpu)` CI job.
- Pull a recorded `gpu-result.txt` home through `gpu-host.sh rsh` before the next sync.
- Adding `rollup-stddev.sql` moves `testdata/test_duckdb_result.py`'s tpch pin (42 → 43). Run
  every cost-report CI step locally before reporting.
- #225's schema-hook masks beyond the spec's list: empty-sorts left `tpcds/q17` at
  `schema_validation_disabled(tp1_single | tp1_rowgroup)` on #225, and distinct-functions carries
  `disabled(tp4_single | tp4_rowgroup | tp4_sized)` on #225. With #225 closed, every such mask
  goes, and the hook runs at those cells.
- keyless-identity's done call is priced at zero bytes; not this task's.

## Dispatch log

### 2026-10-20 — `building`, developer dispatched

## Working notes — 2026-10-20 developer pass (all eleven plan tasks, uncommitted)

- **Plan vs code.** keyless-identity landed `done_call = was_handed_nothing(in) && Partial` (a merge
  handed nothing still refuses through `take_input`), not the plan's unconditional form; kept, with
  `agg_phase` above it. `sort.cpp` still had a `ColumnRef` arm (the plan said exit-copies removed
  it); it now resolves through `column_at`. The join's probe keys resolve at the first probe batch
  against its own names (no probe table exists at construction); the filter-schema names come from
  one helper, `join::filter_names` (`join_residual.cpp`), over the build's and the probe's names
  (`probe_schema`'s before a probe batch).
- **Private default `TableResult()`** forced: `run_op` split into `run_kind` (dispatch.cpp), the
  scan-rowgroups and repartition sites in node_session.cpp, `JoinSession::State` built with its
  build side, and three gtests (an emptied handle for the registry refusal, `std::optional` out
  params). `register_handle` keeps its check: the fields are still public.
- **Fixtures the name check found** (wrong references in test code, fixed, never the check):
  `gpu_backend/gpu_tests/contract.rs` `summed` and `accumulate.rs`
  `a_merge_folds_the_partials_its_init_produced` merged by `v@1` over a state whose column 1 is
  `sum(v)`; `aggregate_cases.rs` `a_count_merges_by_sum` uploaded its arrivals under `sum(i64)`
  where the merge declares `count(i32)`. The cursor-based merge never read the args, so all three
  were green before. No corpus cell was refused: no ticket.
- **#225 masks removed** with the fix: q17 tp1, distinct-functions tp4, shuffle-stddev; q39's
  registry row loses `225` (it stays off on #57). All validated cells green on the device.
- **Red evidence.** One red build: the final tests over `cpp/src` as it stood after Task 5 (names
  from `state_names`, dead arms gone, cursor merges, no reader checking names), with
  `test_executor.cpp` and two `test_plan_executor.cpp` call sites at their Task-5 signatures
  (`/tmp/aa-red.patch`, verified to compile locally first). #225's flips are green in that tree;
  their red is HEAD's `bug_stddev_*_identically_named_columns`, green on keyless-identity's
  dispatched CI.
- **Device runs, nebius-gpu, `~/peacockdb-K`, cuDF 25.02, `build-test-shadgpu.sh --build`.**
  - Red build rc 0, no warnings. `peacock_plan_tests --gtest_filter='AggregateBuilder.*:ColumnRefs.*:TableResultInvariant.*'`:
    6 pass, 8 fail — the six builder cases and the two sort-key cases, each for its plan reason
    (cursor reads the wrong column or MERGE_M2 rejects m2 as a count; count merged by SUM; one STD
    column; no arity check; `vector::_M_range_check` / not refused for bad references).
    `peacock_join_session_tests --gtest_filter='ColumnRefs.*'`: the misnamed residual ran (1 fail).
    `peacockdb_core_gpu_lib` filtered: 6 pass, 3 fail — both rollup cases (5 columns held where 9
    declared) and the #216 finalize pin (old `ColumnRef index 2 out of range (cols=1)` message).
  - Green build rc 0, no warnings. `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus rollup_stddev`: 5/5,
    `gpu-result.txt` +5 sections, nothing else moved; rsynced home before the second sync.
  - `gpu-host-test.sh run` (the L40S list): `peacock_cpu_tests` 21/21, `peacock_gpu_tests` 2/2,
    `peacock_join_session_tests` 54/54, `peacock_plan_tests` 108/108, `peacock_gpu_benchmarks`
    8 passed (3 `bench_` skipped), `test_node_timing` 1/1, `test_gpu_corpus` 894/894 (892 cells,
    every enabled cell, the name check at every node), `peacockdb_core_gpu_lib gpu_tests::` 725/728
    — the three fixtures above.
  - Second sync (only the three fixture files differed; gpu-result.txt confirmed identical first),
    rebuild rc 0, `peacockdb_core_gpu_lib gpu_tests::` 728/728. The other binaries do not compile
    those files. VM soft-stopped, rc 0.
- **Local.** rust-only `--lib` 843 + 2 ignored, `test_cpu_corpus` 2202, `test_corpus_goldens` 28,
  `test_cost_model` 3, `test_ci_coverage` 11, `test_module_layout` 18, `test_golden_format` 43;
  ffi `--lib -- ffi_tests:: …the_payload_golden` 6, `test_ffi` 3 (needs the cuDF lib on
  `LD_LIBRARY_PATH`); `ctest -L cpu` 21. Every cost-report step locally (python from a venv with
  pandas, matplotlib and shellcheck-py): all green; `--cost-diff --base ENS-keyless-identity`
  rc 0, "no cost change vs base", so no regression to name.
- **DuckDB venv.** The system python has no `ensurepip`; `/tmp/duckdb-1.5.4` is built from
  `~/data/miniforge3/bin/python3`. `duckdb_result.py --only` rewrites the whole file with the one
  section: regenerate the whole dataset instead and read the diff.
- **Slip, undone.** `git add -N` was run once on the three new files to preview clang-format; it
  was reverted at once with `git reset -- <files>`, leaving the index as HEAD's.
- **Deferred** (chain K's note): the sf40 binaries, `--run-benchmarks`, Nsight, H200 timing; the
  26.02 build leg is CI's.

### 2026-10-20 — green on both engines; PR opened; `reviewing`

Coordinator applied the developer's wiki list: `build-test.md` counts (5437 / 4832 / 207; cpu
3078; gpu 1634; corpus 2201 / 893; 892 cells; the hook-mask parenthetical gone), C++ unit and
plan-executor rows; `architecture.md` (aggregate sequence's names, `CudfAggregate` fields and
cell, the scan's name check, union branch projects, the Welford merge's copies, `TableResult`'s
five constructors, executor layout, column indexing and what guards it, the flat-buffer rows for
`state_names`, `ddof`, the deprecated pair, node display); #164, #225, #280 archived, `tickets.md`
94 open (corpus-coverage 19); #57, #216, #94, #264 rewritten. Cost: no change vs base.
Not fixed, noted for the human: `architecture.md` "Every cast is explicit" lists the Welford
operand casts as plan-node casts, but the FLOAT64 cast is the C++'s (predates this task);
welford-device's frozen spec quotes the old `ColumnRef index 2 out of range` message.
Review round 1 dispatched.
- PR #183 → ENS-keyless-identity.

### 2026-10-20 — review round 1: 0 blocking, 1 important, 4 nits

1. Important: `test_column_refs.cpp:74-76` — the column path's own check (`expr.cpp:943`) cannot go
   red: `CAST(ref AS FLOAT64)` is AST-able, so `cudf_ast_can_evaluate` throws first. Developer: an
   `Int32` cast, shown red with line 943 reverted.
Nits: `aggregate.cpp:226-227` an extra copy of the INT32 count child per Welford merge (developer);
`architecture.md` the "hand their columns over" paragraph's broken join and the `MERGE_M2`
sentence naming a deleted arm, `build-test.md`'s scan-mapping row prose (coordinator, fixed).
Reviewer checked the three fixture changes: fixture bugs, not an over-strict check.

## Review round 1 — developer, 2026-10-20

- **Important, fixed.** `ColumnRefs.AComputedKeyRefusesEachBadReferenceInsideIt` used
  `CAST(ref AS FLOAT64)`, which the AST takes, so `cudf_ast_can_evaluate`'s `ColumnRef` arm threw
  first and `evaluate_column`'s check (`expr.cpp:943`) could not go red. The key is now
  `CAST(ref AS Int32)`: the AST casts only to INT64 and FLOAT64, so the routing answers false
  without reading the reference and the Cast arm's `evaluate_column` resolves it.
- **Nit, fixed.** `MergedChild` casts an INT32 count straight from the struct's child view
  (`as_int64_count(column_view)`) and copies only the two moments, as before the builder.
- **Device, nebius-gpu `~/peacockdb-K`, synced from the working tree over f9f522a7.** Red build
  (only `expr.cpp:943` back to the raw index, `/tmp/aa-red2.patch`) rc 0, 0 warnings:
  `peacock_plan_tests --gtest_filter='ColumnRefs.*'` 2 pass, 1 fail —
  `AComputedKeyRefusesEachBadReferenceInsideIt` at all three bad references (cuDF's
  `_M_range_check` for the index past the width; the unnamed and misnamed keys not refused).
  Green build rc 0, 0 warnings, VM C++ tree md5-identical to local:
  `ColumnRefs.*:AggregateBuilder.*:AggregateMerge.*` 16/16; the L40S list ALL rc=0 —
  `peacock_cpu_tests` 21, `peacock_gpu_tests` 2, `peacock_join_session_tests` 54,
  `peacock_plan_tests` 108, `peacockdb_core_gpu_lib gpu_tests::` 728, `test_gpu_corpus` 894,
  `test_node_timing` 1, `peacock_gpu_benchmarks` 8 (`bench_` skipped). VM soft-stopped.
- **Local.** `scripts/build.sh … --build` clean; `ctest -L cpu` 21/21. No test count moves.

### 2026-10-20 — round 1 closed; `completing`

The important and the copy nit fixed; the computed-key gtest shown red with `expr.cpp:943`
reverted; L40S list green again. Completeness pass dispatched.

## Completeness finding — the AST route's `ColumnRef` check, developer, 2026-10-20

- **Finding.** `cudf_ast_can_evaluate`'s `ColumnRef` arm (`expr.cpp:437`) could not go red. It
  returns `true` like `default:`, and `TheAstRouteResolvesAReferenceUnderAnOperator` uses a binary
  expression, where `infer_expr_type` throws first.
- **Fix, tests only.** Three cpu-tier gtests in `cpp/tests/cpu/test_executor.cpp`, none through
  `infer_expr_type`: `ColumnRefs.TheAstRouteResolvesABareReference`, `…AReferenceUnderACast`
  (to Int64, which the AST takes) and `…AReferenceUnderAUnaryOperator` (Negative). Each refuses a
  wrong name, no name and an index past the width against one INT32 column `x`, each message
  matched exactly, then answers true for `x@0`.
- **Red** (only the arm's `column_ordinal` line deleted): the three new cases fail at all nine bad
  references with "resolved: …"; `UnderAnOperator` stays green. **Green** (restored, `expr.cpp`
  byte-identical to HEAD): `scripts/build.sh … --build` rc 0, 0 warnings; `peacock_cpu_tests`
  24/24; `ctest -L cpu` 1/1.
- **Counts.** C++ CPU/FFI unit 21 → 24, so C++ 207 → 210 and the grand total 5437 → 5440.
  `build-test.md` not yet updated.

### 2026-10-20 — completeness pass closed; `completeness approved`

Reviewer: 2 important — the AST arm's `ColumnRef` check unguarded (three cpu-tier gtests added by a
developer, red with the check deleted; C++ unit 24, total 5440), and stale text: the decimal-scale
row and `aggregates.py`'s `stddev_ddof` docstring (coordinator). Analyst: none blocking or
important; `architecture.md` corrected for the four sentences it named (decimal-scale row; the
flat-buffer table's last rows now five; `column_ordinal` in the column-indexing row; "Six stay in
C++", the Welford casts) and `run_kind`. Signoff written. GPU host hard-stopped.
