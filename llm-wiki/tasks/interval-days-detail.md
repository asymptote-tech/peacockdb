# interval-days — run record

## Run log

- 2026-10-21: branched `ENS-interval-days` off `ENS-oracle-cleanup`. Developer dispatched.
  verda: no address, CPU tests local. GPU: nebius-gpu, tree `~/peacockdb-K`. oracle-cleanup's
  grammar is in place: `duckdb(sort|no_sort, …, date("<column>"))`; `date` is not in `FACETS`
  yet (deliberately, for this task). An ORDER BY query must declare `no_sort` (order guard).

## Developer log

- **Disk.** Local `/` had 2.9 GB free; removed this worktree's own `target/debug/incremental`
  (6.8 GB, rebuildable) and build with `CARGO_INCREMENTAL=0`.
- **Baseline** `--lib` (rust-only): 869 passed, 2 ignored.
- **Wire, red then green.** Five tests written first: `each_interval_literal_type_crosses_as_three_parts`,
  `a_null_interval_crosses_as_a_typed_null`, `an_interval_outside_date_or_timestamp_plus_minus_is_refused`
  (in `wire/expr_writer/tests.rs`), `a_payload_the_wire_cannot_carry_fails_the_plan_and_names_where`
  (now a `Time64Nanosecond` literal, `(#249)`) and `fb_text_prints_an_interval_literal_as_its_three_parts`
  (`wire/tests.rs`). Red: first a compile failure (no `interval_*` accessors, no
  `DataType::IntervalMonthDayNano`), then with the fbs fields 5 FAILED for the missing arms and
  the old `(#168)`. Green after `serialize_scalar_value`, the Binary-arm refusal and `fb_text`:
  `wire::` 52 passed.
- **The refusal lives on `write_expr`'s Binary arm** (`refuse_interval_off_the_device_arm`): an
  interval literal operand is accepted only under `Plus` (either side) or `Minus` (right side)
  with `out_type` `Date32` or a naive `Timestamp`. That covers the zoned timestamp, `Date64`,
  interval-by-number and an interval comparison by one rule, read off the declared output type.
- **Merge fixture** moved from `mixed-join mode=tp1-single` to pbench
  `timestamp-s-key-group mode=tp1-single` (#264); `corpus_golden::` 9 passed.
- **C++ split.** `add_interval` and the routing went to a new `cpp/src/interval.cpp`
  (`is_interval_literal`, `shift_by_interval`, declared in `peacock/expr.h`): `expr.cpp` would
  have reached 1079 lines. `literal_is_valid` lost its `static` so the one reader of `is_null`
  stays one. The gtests are a new file, `cpp/tests/gpu/test_interval_arithmetic.cpp`, since
  `test_plan_executor.cpp` is already 3043 lines; both are added to `cpp/CMakeLists.txt`.
  The red build therefore reverses only `build_column_binary`'s two routing lines (reversing
  all of `expr.cpp` would make `literal_is_valid` static again and fail the link).
- Syntax-checked locally against cuDF 25.02's headers (`cpp/build/compile_commands.json`,
  `-fsyntax-only`): `interval.cpp`, `expr.cpp`, both gtest files compile. No local GPU.

### DataFusion 45's answers (scratch program over `SessionContext`, deleted)

| SQL | answer |
|---|---|
| `DATE '2024-01-31' / '2023-01-31' / '2024-03-31' + INTERVAL '1 month'` | 2024-02-29, 2023-02-28, 2024-04-30 |
| the same `- INTERVAL '1 month'` | 2023-12-31, 2022-12-31, 2024-02-29 |
| `DATE '2024-03-10' + '36 hours'`, `+ '12 hours'`, `- '12 hours'`, `- '36 hours'` | 2024-03-11, 2024-03-10, 2024-03-10, 2024-03-09 |
| `INTERVAL '1 day' + DATE '2024-03-10'` | 2024-03-11, `Date32` |
| `arrow_cast(TIMESTAMP '2024-01-30 23:00:00.000000123', <unit>) + INTERVAL '1 month 2 days 3 hours'` | 2024-03-03T02:00:00 at s, ms, us; `…02:00:00.000000123` at ns; the unit kept |
| ms `2024-01-01 00:00:01` ± `INTERVAL '0.0000015 seconds'` | 1704067201000, 1704067200999 |
| `DATE '2024-01-29'` and `'2024-01-30' + INTERVAL '1 month 2 days 3 hours'` | 2024-03-02 both |
| `DATE '2024-03-10' + CAST(NULL AS INTERVAL)` | NULL, `Date32` |
| `INTERVAL '90' DAY` | `Interval(MonthDayNano)` |

### DuckDB's timestamps, measured (DuckDB 1.5.4, pbench sf1)

- `typeof` of `f_ts_ms`, `f_ts_us`, `f_ts_ns` and `f_dt` ± `INTERVAL '1 month 2 days 3 hours'` is
  `TIMESTAMP` (microseconds) for all four. DataFusion keeps `Timestamp(ms|us|ns)` and `Date32`.
- interval-mixed-units, 400 rows, `mini.result.txt` against `duckdb-result.txt` cell by cell:
  `ms`, `us` and `ns` are identical text in all 400 rows (the values are whole microseconds, and
  both writers print arrow-rs's fraction groups). `dt` differs in all 400, by DuckDB's time part
  only. interval-month-clamp: `next_month` and `prev_month` differ in 4 of 4, by the time part
  only.
- So **`timestamp("<column>")` was not added**: no timestamp column differs, even in print.
  `date("dt")` and `date("next_month", "prev_month")` are on the lines, and `date` joins `FACETS`.
- `cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- duckdb_pbench_interval`:
  6 passed.
- The queries keep the interval in a device expression at tp1-single (read off
  `tp1-single.plans.txt`): a filter predicate (interval-where), an aggregate's argument
  (interval-days-filter), a project after a group-by on `f_dt` (interval-month-clamp), a project
  over four types (interval-mixed-units), a keyed residual `f_dt <= d_dt + 1 mons`
  (interval-join-keyed), and a keyless nested-loop condition `(f_dt + 7 days) < d_dt`, whose
  left operand the session hoists (interval-join-keyless). None needed rewriting.
- `/tmp/duckdb-1.5.4` was made from miniforge's python: the system python has no `ensurepip`.

### Device cycle 1 (nebius-gpu, `~/peacockdb-K`, 2026-10-21)

- Red patch: `build_column_binary`'s two routing lines removed (`/tmp/K-interval-red.patch`).
  Red build EXIT 0. `peacock_plan_tests --gtest_filter='IntervalArithmetic.*:Literals.EveryWireType*'`:
  9 ran, 8 FAILED, each `unsupported scalar type: IntervalMonthDayNano` (the last one fails its
  `(#249)` assertion on that message); the walk row passes in red, as planned.
  `test_gpu_corpus mixed_join`: 5 ran, 5 FAILED, `join_probe … unsupported scalar type:
  IntervalMonthDayNano`.
- Green build EXIT 0, no warnings. `peacock_plan_tests --gtest_filter='IntervalArithmetic.*:Literals.*:Case*'`:
  20 passed. `PCK_WRITE_GPU_RESULT=1 test_gpu_corpus mixed_join`: 5 passed;
  `… interval_`: 30 passed. rsync home: 35 sections added (tpch 5, pbench 30), none removed.
- Soft stop 09:40Z.

### Corpus, wiki, ticket

- mixed-join: plan goldens regenerated, only its five `tpch.sf1/*.plans.txt` sections moved
  (`not runnable … (#168)` became a recipe list); cells on in `corpus_cases.inc` and the registry
  (`168` struck). `NOT_RUNNABLE = &[]`.
- Six pbench lines and registry rows; plan, cpu (`*-mini.cpu.txt`, `.cost.txt`,
  `mini.result.txt`) and `duckdb-result.txt` sections added and nothing else moved.
  `report.html` regenerated after the `gpu-result.txt` sections came home.
- `date` joined `FACETS` (`test_support/mod.rs`), since two lines now use it.
- #168 archived under Done; `tickets.md` 85 → 84, corpus-coverage 11 → 10; the Scalars section of
  `corpus-coverage.md` went with its one ticket. #249's text says what stays refused under it;
  df-upgrade's #168 link points at the archive. Historical reports under `llm-wiki/reports/` still
  link `corpus-coverage.md#t168`; left as records.
- `build-test.md`: counts (lib 875, cpu corpus 2308, gpu corpus 944, plan gtests +8 → 126, C++
  230, Rust 5012, total 5640) and the rows that describe the new tests. `architecture.md`: an
  interval-fields row in *What the Rust side puts in the flat buffers*, and `interval.cpp` in the
  C++ layout.
- CPU, local (`/tmp/K-cpu-suite.sh`): `--lib` 873 passed + 2 ignored; `test_cpu_corpus` 2308
  passed; `test_corpus_goldens` 28; `test_cost_model` 3; `cost-report` 43 + 32. One warning in
  the run, pre-existing and not this branch's: cost-report's `sha_links` is never used.
  `scripts/test_ticket_widget.py` 3 OK, `testdata/test_duckdb_result.py` 22 OK.

### Device cycle 2: the full L40S list

- First full run (build of the tree above): every binary green but `peacockdb_core_gpu_lib`
  (rc 101, 738 passed, 1 failed): `every_pbench_join_shape_has_a_node_level_case` named
  `interval-join-keyed` and `interval-join-keyless`. The guard wants a node-level case for every
  pbench join query, and the build-test.md rule says so; the plan missed it. `test_gpu_corpus`
  944 passed, `peacock_plan_tests` 126, `peacock_join_session_tests` 56, `peacock_cpu_tests` 26,
  `peacock_gpu_tests` 2, `peacock_gpu_benchmarks` 8, `test_node_timing` 1.
- Fix: two cases in `tests/gpu_tests/join_key_type_cases.rs` over the `key_types` sides' `d`:
  `an_inner_join_with_an_interval_residual_agrees` (`p_d <= b_d + 1 month`, keyed on `k32`) and
  `a_nested_loop_join_on_an_interval_condition_agrees` (`b_d + 7 days < p_d`, hoisted), listed in
  `PBENCH_SHAPES`. Rebuilt; `peacockdb_core_gpu_lib gpu_tests::` 741 passed. build-test.md counts
  follow (gpu lib 741, operator harness 477, Rust 5014, total 5642).
- Drift seen, not this branch's: build-test.md's Join session (C++) row says 54; the binary runs 56.
- Final full L40S list on the final build: `ALL rc=0` — `peacock_cpu_tests` 26, `peacock_gpu_tests` 2,
  `peacock_join_session_tests` 56, `peacock_plan_tests` 126, `peacockdb_core_gpu_lib` 741,
  `test_gpu_corpus` 944, `peacock_gpu_benchmarks` 8 (3 `bench_` skipped), `test_node_timing` 1;
  no segfault in any log. `cost-report` rerun after the gpu-case edit: 43 + 32. Soft stop 10:22Z.
- Deferred, as the L40S list always defers: sf40 binaries, `--run-benchmarks`, Nsight, H200.
