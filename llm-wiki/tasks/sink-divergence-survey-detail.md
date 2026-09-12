# sink-divergence-survey — run detail

Spec: [`sink-divergence-survey.md`](sink-divergence-survey.md). Plan:
[`sink-divergence-survey-impl.md`](sink-divergence-survey-impl.md).

## Chain position and branch

Chain `ENS-drop-mode-name`, task 7, **a prototype**: branch `ENS-sink-divergence-survey`, forked at
`2cb92f63`, the tip of task 6's branch. No PR, no reviewer, never merged; `done` means the branch
is pushed with the report at `llm-wiki/reports/sink-divergence.md` and the signoff on the spec.
`pipeline.yml` runs on pushes to master and on PRs, so CI does not run here.

## How this task is dispatched

Plan task 1 (the message, with a CPU unit test) as one dispatch; plan tasks 2 and 3 (the rollout,
then the report from its evidence) as the next, since the report is the rollout's product and the
developer who collected the evidence writes it. The coordinator commits after each.

## Hosts at dispatch (2026-09-12)

Ubuntu 24.04 / glibc 2.39 box; verda's hostname does not resolve; shad-gpu up with a neighbour
at 37 GiB of 143.7; `cpp/build`, `cpp/install` and `target-cudf-rapids-cuda-12.2` warm from task
6's cycle; `target/` warm for `rust-only`; `CUDF_ROOT=/home/dmitry/data/miniforge3/envs/rapids-cuda-12.2`.

## What the plan leaves open, settled here

- **The error site** is `executor/gpu_backend/mod.rs:239` on this tree (`concat_batches`), not the
  plan's 176-184; task 6 moved lines. `GpuBackend`, `GpuBatch` and `GpuContext` are `pub(crate)`
  behind `cfg_attr(not(feature = "test-support"), allow(dead_code))` since task 6; nothing here
  changes that. The surface guard `SURFACE` refuses any new bare `pub`, and a prototype is held to
  the layout test like any branch — the one structural change the plan allows (a free function
  taking two schemas, for the unit test) is `pub(crate)` or private.
- **A disabled device cell is no test.** `corpus_query!` with `gpu_modes = none` expands to
  nothing in `test_gpu_corpus`, so the survey cannot filter its way to a disabled cell. On this
  throw-away branch the schema-disabled queries are enabled at **one mode, `tp1_single`**, in
  `tests/common/corpus_cases.inc`, in a commit of its own that says it is the survey's enablement
  and not a change to the corpus. `testdata/cost-registry.csv` is untouched — the spec's "no
  registry edit" — and the registry assertion is filtered out of every survey run
  (`PCK_TEST_FILTER` names queries, and it is not a query). One mode, because a divergence class is
  a property of types, not of lanes or batching; the report says so and names any query whose
  sink message would need another mode to reach.
- **The cell set** comes from the registry's `tickets` column: queries naming #183, #187, #191 or
  #163 — 94 rows, most of them `152 183`, where #152's refusal comes first and the sink is never
  reached; those are the "disagrees with its ticket" rows the spec asks for, not failures of the
  survey. The developer writes the exact list here before running anything.

## Run log

### 2026-09-12 — plan task 1 dispatched: the message names every diverging column

Board set to `building`.

### 2026-09-12 — plan task 1 done: the message names every diverging column

**Shape.** `try_new`'s sentence stays as the prefix; the appendix is ` (declared vs exported:
{index} {name}: {declared} vs {exported}; …)`, types only, and it is omitted when no zipped
column differs by type (a count-only refusal keeps the bare prefix). From the test:
`0 l_comment: Utf8View vs Utf8; 2 l_extendedprice: Decimal128(15, 2) vs Decimal128(38, 2)` —
a matching column between two diverging ones is skipped, its index is not renumbered. A
nullability-only difference yields the empty string.

**Where.** The comparison is `schema_divergence(&Schema, &Schema) -> String`, `pub(crate)` in
`peacockdb-core/src/executor/errors.rs`, with `#[cfg_attr(feature = "rust-only", allow(dead_code))]`
on the `wire_nodes` precedent: its one production caller is the sink in `gpu_backend/mod.rs`,
which `rust-only` compiles out. It sits there rather than beside the site because
`gpu_backend` is `cfg(not(feature = "rust-only"))`: a `tests` module inside it would be named
for the floor rung and never run on it, and would only ever run under `--lib` of an FFI-linked
build, which the rung path filters do not select. The tests are
`peacockdb-core/src/executor/errors/tests.rs` (`#[cfg(test)] mod tests;`, three cases: one
column with both types, two clauses across a matching column, nullability alone is none), red
first against a stub returning `""` (two assertion failures, the nullability case vacuously
green), then green. The sink's wrapping — prefix, conditional parenthetical — is three lines
without a unit test; the rollout is what exercises it.

**Results.** `cargo build --features rust-only -p peacockdb-core` 0 warnings;
`cargo-cudf.sh build -p peacockdb-core --features gpu` 0 warnings;
`--test test_module_layout` 17 passed; `--features rust-only --lib` 517 passed, 2 ignored
(514 + 3); `cargo-cudf.sh test --lib --features gpu --no-run` 0 warnings. Running the three
cases from that gpu-shape binary needs `LD_LIBRARY_PATH=$CUDF_ROOT/lib` locally (it fails at
load on `libcudf.so` otherwise — the loader path `build-test-shadgpu.sh` supplies as
`PATCHED_LD`); with it, 3 passed. rustfmt-check clean on the three touched files with
`--edition 2024 --style-edition 2024` (the crate's; the default 2021 sort reorders every
import in the tree). Not committed.

### 2026-09-12 — plan task 2: the cell set, written before any run

**Selection.** `testdata/cost-registry.csv` rows whose `tickets` column names #183, #187, #191
or #163: 94 queries, every one `disabled` at `gpu_tp1_single` and `plan_status=ok`. No other
ticket in the column reads as a schema cause except #180 (a nullability declaration the merge
contradicts, `tpcds/q96` at the three tp4 modes); it is excluded because its cells are enabled
at both tp1 modes already, so the one mode this survey runs cannot show it. Ticket frequency
over the whole column, for the record: #152 79, #183 60, #97 32, #163 23, #187 10, #191 1.

**Enablement.** Each of the 94 gets `tp1_single` as its `gpu_modes` in
`peacockdb-core/tests/common/corpus_cases.inc`, nothing else on the line; the registry is
untouched, so `the_registry_matches_the_gpu_corpus_in_both_directions` would go red and is
never selected — `PCK_TEST_FILTER` names test-name prefixes. `PCK_RUN_CPP=0` for every run:
the C++ suites are not what changed and `peacock_tpch_tests` wants 69 GiB beside a 37 GiB
neighbour. No `UPDATE_CANONICAL` or `PCK_UPDATE_SECTIONS` in the environment (checked).

**Batches.** The runner hands each staged binary `PCK_TEST_FILTER` as one substring
(`scripts/lib/rung-args.sh` prints it as a single argument), so a batch is a prefix of the
test names `gpu_<dataset>_<query>_tp1_single`, not a list of five. Twenty batches; the
ticket string beside each query is the registry's:

| # | filter | queries (registry tickets) |
|---|---|---|
| 1 | `gpu_tpch_q1` | q1 [163], q10 [152 183], q12 [152 183], q15 [183 184], q16 [59 62 80 97 152 175 183], q17 [163], q18 [97 152 183]; q19 already enabled rides along |
| 2 | `gpu_tpch_q2` | q2 [152 187], q20 [97 152 183], q21 [59 80 97 152 183], q22 [59 80 97 163] |
| 3 | `gpu_tpch_q4_` | q4 [97 183] |
| 4 | `gpu_tpch_q5_` | q5 [152 183] |
| 5 | `gpu_tpch_q7_` | q7 [152 183] |
| 6 | `gpu_tpch_q8_` | q8 [152 191] |
| 7 | `gpu_tpch_q9_` | q9 [152 183] |
| 8 | `_join_tp1_single` | anti_join [183], cross_join [183], hash_join [116 152 187], nested_loop_join [183], nested_loop_left_join [116 183], rollup_over_join [152 183 189], semi_join [183] |
| 9 | `gpu_tpch_shuffle_` | shuffle_additive [183], shuffle_additive_avg [163], shuffle_stddev [183] |
| 10 | `gpu_tpch_aggregate_groupby_` | aggregate_groupby [183] |
| 11 | `gpu_tpch_filter_project_` | filter_project [187] |
| 12 | `gpu_tpcds_q1` | q1 [163], q10 [97 152 183], q11 [152 183], q13 [163], q14 [65 97 163], q15 [152 183], q16 [59 62 80 97 152 187], q17 [163], q18 [65 163], q19 [152 183] |
| 13 | `gpu_tpcds_q2` | q21 [152 183], q22 [65 163], q23 [152 183], q24 [45 163], q25 [152 183], q26 [163], q29 [152 183] |
| 14 | `gpu_tpcds_q3` | q3 [152 183], q30 [163], q31 [152 183], q32 [163], q33 [97 152 187], q34 [152 183], q35 [97 163], q37 [152 183], q39 [57 163] |
| 15 | `gpu_tpcds_q4` | q4 [152 183], q40 [97 152 183], q42 [152 183], q43 [152 183], q45 [97 152 183], q46 [152 183] |
| 16 | `gpu_tpcds_q5` | q50 [152 183], q52 [152 183], q55 [152 183], q56 [97 152 183], q58 [97 152 183], q59 [152 183] |
| 17 | `gpu_tpcds_q6` | q6 [163], q60 [97 152 183], q61 [46 152 187], q62 [152 183], q65 [163], q66 [55 152 183], q68 [152 183], q69 [59 80 97 152 183] |
| 18 | `gpu_tpcds_q7` | q7 [163], q73 [152 183], q74 [152 183], q76 [115 152 183], q77 [47 65 97 152 175 187], q79 [152 183] |
| 19 | `gpu_tpcds_q8` | q8 [152 183], q80 [65 97 152 183 189], q81 [163], q82 [152 183], q83 [97 152 183], q84 [152 183], q85 [163] |
| 20 | `gpu_tpcds_q9` | q9 [63 163], q90 [152 180 187], q91 [152 183], q92 [163], q94 [59 62 80 97 152 187], q95 [62 97 152 187], q99 [152 183] |

Host at start: neighbour pid 1814433 at 37,034 MiB of 143,771; no gate run in flight
(`.run-state/gate.rc` carries a finished id).

#### batch 1 — `PCK_TEST_FILTER=gpu_tpch_q1`, run 20260912T041408-227121, exit 1

FAILED. 1 passed; 7 failed; 0 ignored; 0 measured; 94 filtered out; finished in 8.05s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q10_tp1_single`: tpch/q10 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 c_name: Utf8View vs Utf8; 3 c_acctbal: Decimal128(15, 2) vs Decimal128(38, 2); 4 n_name: Utf8View vs Utf8; 5 c_address: Utf8View vs Utf8; 6 c_phone: Utf8View vs Utf8; 7 c_comment: Utf8View vs Utf8)
- `gpu_tpch_q12_tp1_single`: tpch/q12 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_shipmode: Utf8View vs Utf8)
- `gpu_tpch_q15_tp1_single`: tpch/q15 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 s_name: Utf8View vs Utf8; 2 s_address: Utf8View vs Utf8; 3 s_phone: Utf8View vs Utf8)
- `gpu_tpch_q16_tp1_single`: tpch/q16 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 p_brand: Utf8View vs Utf8; 1 p_type: Utf8View vs Utf8)
- `gpu_tpch_q17_tp1_single`: /home/info/peacockdb/testdata/goldens/tpch.sf1/tp1-single-mini.cpu.txt: `q17` moved — line 1, column 0 — expected `skipped: not enabled at this mode` — actual `early_exit=none`
- `gpu_tpch_q18_tp1_single`: tpch/q18 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_name: Utf8View vs Utf8; 4 o_totalprice: Decimal128(15, 2) vs Decimal128(38, 2); 5 sum(lineitem.l_quantity): Decimal128(25, 2) vs Decimal128(38, 2))
- `gpu_tpch_q19_tp1_single`: ok
- `gpu_tpch_q1_tp1_single`: tpch/q1 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_returnflag: Utf8View vs Utf8; 1 l_linestatus: Utf8View vs Utf8; 2 sum_qty: Decimal128(25, 2) vs Decimal128(38, 2); 3 sum_base_price: Decimal128(25, 2) vs Decimal128(38, 2); 6 avg_qty: Decimal128(19, 6) vs Decimal128(38, 6); 7 avg_price: Decimal128(19, 6) vs Decimal128(38, 6); 8 avg_disc: Decimal128(19, 6) vs Decimal128(38, 6))

#### batch 2 — `PCK_TEST_FILTER=gpu_tpch_q2`, run 20260912T041523-227235, exit 1

FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 98 filtered out; finished in 5.91s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q20_tp1_single`: tpch/q20 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_name: Utf8View vs Utf8; 1 s_address: Utf8View vs Utf8)
- `gpu_tpch_q21_tp1_single`: tpch/q21 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_name: Utf8View vs Utf8)
- `gpu_tpch_q22_tp1_single`: tpch/q22 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 cntrycode: Utf8View vs Utf8; 2 totacctbal: Decimal128(25, 2) vs Decimal128(38, 2))
- `gpu_tpch_q2_tp1_single`: tpch/q2 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(15, 2) but found Decimal128(38, 2) at column index 0 (declared vs exported: 0 s_acctbal: Decimal128(15, 2) vs Decimal128(38, 2); 1 s_name: Utf8View vs Utf8; 2 n_name: Utf8View vs Utf8; 4 p_mfgr: Utf8View vs Utf8; 5 s_address: Utf8View vs Utf8; 6 s_phone: Utf8View vs Utf8; 7 s_comment: Utf8View vs Utf8)

#### batch 3 — `PCK_TEST_FILTER=gpu_tpch_q4_`, run 20260912T041559-227316, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 0.94s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q4_tp1_single`: tpch/q4 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 o_orderpriority: Utf8View vs Utf8)

#### batch 4 — `PCK_TEST_FILTER=gpu_tpch_q5_`, run 20260912T041604-227376, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 1.53s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q5_tp1_single`: tpch/q5 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 n_name: Utf8View vs Utf8)

#### batch 5 — `PCK_TEST_FILTER=gpu_tpch_q7_`, run 20260912T041609-227436, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 1.57s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q7_tp1_single`: tpch/q7 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 supp_nation: Utf8View vs Utf8; 1 cust_nation: Utf8View vs Utf8; 2 l_year: Int32 vs Int16)

#### batch 6 — `PCK_TEST_FILTER=gpu_tpch_q8_`, run 20260912T041614-227496, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 1.79s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q8_tp1_single`: tpch/q8 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Int32 but found Int16 at column index 0 (declared vs exported: 0 o_year: Int32 vs Int16)

#### batch 7 — `PCK_TEST_FILTER=gpu_tpch_q9_`, run 20260912T041619-227556, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 1.68s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_q9_tp1_single`: tpch/q9 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 nation: Utf8View vs Utf8; 1 o_year: Int32 vs Int16)

#### batch 8 — `PCK_TEST_FILTER=_join_tp1_single`, run 20260912T041624-227616, exit 1

FAILED. 0 passed; 7 failed; 0 ignored; 0 measured; 95 filtered out; finished in 3.93s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_anti_join_tp1_single`: tpch/anti-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 2 (declared vs exported: 2 o_orderstatus: Utf8View vs Utf8; 3 o_totalprice: Decimal128(15, 2) vs Decimal128(38, 2); 5 o_orderpriority: Utf8View vs Utf8; 6 o_clerk: Utf8View vs Utf8; 8 o_comment: Utf8View vs Utf8)
- `gpu_tpch_cross_join_tp1_single`: tpch/cross-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 r_name: Utf8View vs Utf8; 2 r_comment: Utf8View vs Utf8; 4 n_name: Utf8View vs Utf8; 6 n_comment: Utf8View vs Utf8)
- `gpu_tpch_hash_join_tp1_single`: tpch/hash-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(25, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 sum(l.l_quantity): Decimal128(25, 2) vs Decimal128(38, 2); 2 sum(o.o_totalprice): Decimal128(25, 2) vs Decimal128(38, 2))
- `gpu_tpch_nested_loop_join_tp1_single`: tpch/nested-loop-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 r_name: Utf8View vs Utf8; 2 r_comment: Utf8View vs Utf8; 4 n_name: Utf8View vs Utf8; 6 n_comment: Utf8View vs Utf8)
- `gpu_tpch_nested_loop_left_join_tp1_single`: tpch/nested-loop-left-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 r_name: Utf8View vs Utf8; 2 r_comment: Utf8View vs Utf8; 4 n_name: Utf8View vs Utf8; 6 n_comment: Utf8View vs Utf8)
- `gpu_tpch_rollup_over_join_tp1_single`: tpch/rollup-over-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 c_mktsegment: Utf8View vs Utf8)
- `gpu_tpch_semi_join_tp1_single`: tpch/semi-join at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 2 (declared vs exported: 2 o_orderstatus: Utf8View vs Utf8; 3 o_totalprice: Decimal128(15, 2) vs Decimal128(38, 2); 5 o_orderpriority: Utf8View vs Utf8; 6 o_clerk: Utf8View vs Utf8; 8 o_comment: Utf8View vs Utf8)

#### batch 9 — `PCK_TEST_FILTER=gpu_tpch_shuffle_`, run 20260912T041700-227696, exit 1

FAILED. 0 passed; 3 failed; 0 ignored; 0 measured; 99 filtered out; finished in 1.67s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_shuffle_additive_avg_tp1_single`: tpch/shuffle-additive-avg at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_returnflag: Utf8View vs Utf8; 1 l_linestatus: Utf8View vs Utf8; 3 avg_qty: Decimal128(19, 6) vs Decimal128(38, 6); 4 sum_qty: Decimal128(25, 2) vs Decimal128(38, 2))
- `gpu_tpch_shuffle_additive_tp1_single`: tpch/shuffle-additive at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_returnflag: Utf8View vs Utf8; 1 l_linestatus: Utf8View vs Utf8; 3 sum_qty: Decimal128(25, 2) vs Decimal128(38, 2); 4 sum_base_price: Decimal128(25, 2) vs Decimal128(38, 2))
- `gpu_tpch_shuffle_stddev_tp1_single`: tpch/shuffle-stddev at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_returnflag: Utf8View vs Utf8; 1 l_linestatus: Utf8View vs Utf8)

#### batch 10 — `PCK_TEST_FILTER=gpu_tpch_aggregate_groupby_`, run 20260912T041705-227760, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 0.66s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_aggregate_groupby_tp1_single`: tpch/aggregate-groupby at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 l_returnflag: Utf8View vs Utf8; 1 sum(lineitem.l_quantity): Decimal128(25, 2) vs Decimal128(38, 2))

#### batch 11 — `PCK_TEST_FILTER=gpu_tpch_filter_project_`, run 20260912T041710-227820, exit 1

FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 101 filtered out; finished in 0.68s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpch_filter_project_tp1_single`: tpch/filter-project at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(15, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 l_quantity: Decimal128(15, 2) vs Decimal128(38, 2))

#### batch 12 — `PCK_TEST_FILTER=gpu_tpcds_q1`, run 20260912T041726-227936, exit 1

FAILED. 0 passed; 10 failed; 0 ignored; 0 measured; 92 filtered out; finished in 31.32s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q10_tp1_single`: tpcds/q10 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 cd_gender: Utf8View vs Utf8; 1 cd_marital_status: Utf8View vs Utf8; 2 cd_education_status: Utf8View vs Utf8; 6 cd_credit_rating: Utf8View vs Utf8)
- `gpu_tpcds_q11_tp1_single`: tpcds/q11 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 customer_id: Utf8View vs Utf8; 1 customer_first_name: Utf8View vs Utf8; 2 customer_last_name: Utf8View vs Utf8; 3 customer_preferred_cust_flag: Utf8View vs Utf8)
- `gpu_tpcds_q13_tp1_single`: tpcds/q13 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(11, 6) but found Decimal128(38, 6) at column index 1 (declared vs exported: 1 avg2: Decimal128(11, 6) vs Decimal128(38, 6); 2 avg3: Decimal128(11, 6) vs Decimal128(38, 6); 3 sum(store_sales.ss_ext_wholesale_cost): Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q14_tp1_single`: /home/info/peacockdb/testdata/goldens/tpcds.sf1/tp1-single-mini.cpu.txt: `q14` moved — line 1, column 0 — expected `skipped: not enabled at this mode` — actual `early_exit=none`
- `gpu_tpcds_q15_tp1_single`: tpcds/q15 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 ca_zip: Utf8View vs Utf8; 1 sum(catalog_sales.cs_sales_price): Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q16_tp1_single`: tpcds/q16 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(27, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 total shipping cost: Decimal128(27, 2) vs Decimal128(38, 2); 2 total net profit: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q17_tp1_single`: tpcds/q17 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 s_state: Utf8View vs Utf8)
- `gpu_tpcds_q18_tp1_single`: tpcds/q18 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 ca_country: Utf8View vs Utf8; 2 ca_state: Utf8View vs Utf8; 3 ca_county: Utf8View vs Utf8; 4 agg1: Decimal128(16, 6) vs Decimal128(38, 6); 5 agg2: Decimal128(16, 6) vs Decimal128(38, 6); 6 agg3: Decimal128(16, 6) vs Decimal128(38, 6); 7 agg4: Decimal128(16, 6) vs Decimal128(38, 6); 8 agg5: Decimal128(16, 6) vs Decimal128(38, 6); 9 agg6: Decimal128(16, 6) vs Decimal128(38, 6); 10 agg7: Decimal128(16, 6) vs Decimal128(38, 6))
- `gpu_tpcds_q19_tp1_single`: tpcds/q19 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 brand: Utf8View vs Utf8; 3 i_manufact: Utf8View vs Utf8; 4 ext_price: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q1_tp1_single`: tpcds/q1 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_customer_id: Utf8View vs Utf8)

#### batch 13 — `PCK_TEST_FILTER=gpu_tpcds_q2`, run 20260912T041802-228013, exit 1

FAILED. 0 passed; 7 failed; 0 ignored; 0 measured; 95 filtered out; finished in 16.16s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q21_tp1_single`: tpcds/q21 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 w_warehouse_name: Utf8View vs Utf8; 1 i_item_id: Utf8View vs Utf8)
- `gpu_tpcds_q22_tp1_single`: tpcds/q22 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_product_name: Utf8View vs Utf8; 1 i_brand: Utf8View vs Utf8; 2 i_class: Utf8View vs Utf8; 3 i_category: Utf8View vs Utf8)
- `gpu_tpcds_q23_tp1_single`: tpcds/q23 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8)
- `gpu_tpcds_q24_tp1_single`: tpcds/q24 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 s_store_name: Utf8View vs Utf8; 3 paid: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q25_tp1_single`: tpcds/q25 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 s_store_id: Utf8View vs Utf8; 3 s_store_name: Utf8View vs Utf8; 4 store_sales_profit: Decimal128(17, 2) vs Decimal128(38, 2); 5 store_returns_loss: Decimal128(17, 2) vs Decimal128(38, 2); 6 catalog_sales_profit: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q26_tp1_single`: tpcds/q26 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 2 agg2: Decimal128(11, 6) vs Decimal128(38, 6); 3 agg3: Decimal128(11, 6) vs Decimal128(38, 6); 4 agg4: Decimal128(11, 6) vs Decimal128(38, 6))
- `gpu_tpcds_q29_tp1_single`: tpcds/q29 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 s_store_id: Utf8View vs Utf8; 3 s_store_name: Utf8View vs Utf8)

#### batch 14 — `PCK_TEST_FILTER=gpu_tpcds_q3`, run 20260912T041838-228093, exit 1

FAILED. 0 passed; 9 failed; 0 ignored; 0 measured; 93 filtered out; finished in 16.01s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q30_tp1_single`: tpcds/q30 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_customer_id: Utf8View vs Utf8; 1 c_salutation: Utf8View vs Utf8; 2 c_first_name: Utf8View vs Utf8; 3 c_last_name: Utf8View vs Utf8; 4 c_preferred_cust_flag: Utf8View vs Utf8; 8 c_birth_country: Utf8View vs Utf8; 9 c_login: Utf8View vs Utf8; 10 c_email_address: Utf8View vs Utf8; 12 ctr_total_return: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q31_tp1_single`: tpcds/q31 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 ca_county: Utf8View vs Utf8)
- `gpu_tpcds_q32_tp1_single`: tpcds/q32 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(17, 2) but found Decimal128(38, 2) at column index 0 (declared vs exported: 0 excess discount amount: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q33_tp1_single`: tpcds/q33 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(27, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 total_sales: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q34_tp1_single`: tpcds/q34 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 c_salutation: Utf8View vs Utf8; 3 c_preferred_cust_flag: Utf8View vs Utf8)
- `gpu_tpcds_q35_tp1_single`: tpcds/q35 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 ca_state: Utf8View vs Utf8; 1 cd_gender: Utf8View vs Utf8; 2 cd_marital_status: Utf8View vs Utf8)
- `gpu_tpcds_q37_tp1_single`: tpcds/q37 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 i_current_price: Decimal128(7, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q39_tp1_single`: tpcds/q39 at tp1-single on a device: call failed: GpuFilter lane 0: execute_node(#19 CudfFilter): [in CudfFilter] value-form CASE not supported in column path
- `gpu_tpcds_q3_tp1_single`: tpcds/q3 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 2 (declared vs exported: 2 brand: Utf8View vs Utf8; 3 sum_agg: Decimal128(17, 2) vs Decimal128(38, 2))

#### batch 15 — `PCK_TEST_FILTER=gpu_tpcds_q4`, run 20260912T041913-228171, exit 1

FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 96 filtered out; finished in 15.51s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q40_tp1_single`: tpcds/q40 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 w_state: Utf8View vs Utf8; 1 i_item_id: Utf8View vs Utf8; 2 sales_before: Decimal128(33, 2) vs Decimal128(38, 2); 3 sales_after: Decimal128(33, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q42_tp1_single`: tpcds/q42 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 2 (declared vs exported: 2 i_category: Utf8View vs Utf8; 3 sum(store_sales.ss_ext_sales_price): Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q43_tp1_single`: tpcds/q43 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_store_name: Utf8View vs Utf8; 1 s_store_id: Utf8View vs Utf8; 2 sun_sales: Decimal128(17, 2) vs Decimal128(38, 2); 3 mon_sales: Decimal128(17, 2) vs Decimal128(38, 2); 4 tue_sales: Decimal128(17, 2) vs Decimal128(38, 2); 5 wed_sales: Decimal128(17, 2) vs Decimal128(38, 2); 6 thu_sales: Decimal128(17, 2) vs Decimal128(38, 2); 7 fri_sales: Decimal128(17, 2) vs Decimal128(38, 2); 8 sat_sales: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q45_tp1_single`: tpcds/q45 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 ca_zip: Utf8View vs Utf8; 1 ca_city: Utf8View vs Utf8; 2 sum(web_sales.ws_sales_price): Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q46_tp1_single`: tpcds/q46 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 ca_city: Utf8View vs Utf8; 3 bought_city: Utf8View vs Utf8; 5 amt: Decimal128(17, 2) vs Decimal128(38, 2); 6 profit: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q4_tp1_single`: tpcds/q4 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 customer_id: Utf8View vs Utf8; 1 customer_first_name: Utf8View vs Utf8; 2 customer_last_name: Utf8View vs Utf8; 3 customer_preferred_cust_flag: Utf8View vs Utf8)

#### batch 16 — `PCK_TEST_FILTER=gpu_tpcds_q5`, run 20260912T041949-228252, exit 1

FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 96 filtered out; finished in 11.55s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q50_tp1_single`: tpcds/q50 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_store_name: Utf8View vs Utf8; 2 s_street_number: Utf8View vs Utf8; 3 s_street_name: Utf8View vs Utf8; 4 s_street_type: Utf8View vs Utf8; 5 s_suite_number: Utf8View vs Utf8; 6 s_city: Utf8View vs Utf8; 7 s_county: Utf8View vs Utf8; 8 s_state: Utf8View vs Utf8; 9 s_zip: Utf8View vs Utf8)
- `gpu_tpcds_q52_tp1_single`: tpcds/q52 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 2 (declared vs exported: 2 brand: Utf8View vs Utf8; 3 ext_price: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q55_tp1_single`: tpcds/q55 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 brand: Utf8View vs Utf8; 2 ext_price: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q56_tp1_single`: tpcds/q56 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 total_sales: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q58_tp1_single`: tpcds/q58 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 item_id: Utf8View vs Utf8; 1 ss_item_rev: Decimal128(17, 2) vs Decimal128(38, 2); 3 cs_item_rev: Decimal128(17, 2) vs Decimal128(38, 2); 5 ws_item_rev: Decimal128(17, 2) vs Decimal128(38, 2); 7 average: Decimal128(23, 6) vs Decimal128(38, 6))
- `gpu_tpcds_q59_tp1_single`: tpcds/q59 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_store_name1: Utf8View vs Utf8; 1 s_store_id1: Utf8View vs Utf8; 3 sun_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 4 mon_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 5 tue_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 6 wed_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 7 thu_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 8 fri_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6); 9 sat_sales_ratio: Decimal128(23, 6) vs Decimal128(38, 6))

#### batch 17 — `PCK_TEST_FILTER=gpu_tpcds_q6`, run 20260912T042025-228333, exit 1

FAILED. 0 passed; 8 failed; 0 ignored; 0 measured; 94 filtered out; finished in 18.32s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q60_tp1_single`: tpcds/q60 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 total_sales: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q61_tp1_single`: tpcds/q61 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(17, 2) but found Decimal128(38, 2) at column index 0 (declared vs exported: 0 promotions: Decimal128(17, 2) vs Decimal128(38, 2); 1 total: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q62_tp1_single`: tpcds/q62 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 w_substr: Utf8View vs Utf8; 1 sm_type: Utf8View vs Utf8; 2 web_name: Utf8View vs Utf8)
- `gpu_tpcds_q65_tp1_single`: tpcds/q65 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_store_name: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 revenue: Decimal128(17, 2) vs Decimal128(38, 2); 3 i_current_price: Decimal128(7, 2) vs Decimal128(38, 2); 4 i_wholesale_cost: Decimal128(7, 2) vs Decimal128(38, 2); 5 i_brand: Utf8View vs Utf8)
- `gpu_tpcds_q66_tp1_single`: tpcds/q66 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 w_warehouse_name: Utf8View vs Utf8; 2 w_city: Utf8View vs Utf8; 3 w_county: Utf8View vs Utf8; 4 w_state: Utf8View vs Utf8; 5 w_country: Utf8View vs Utf8)
- `gpu_tpcds_q68_tp1_single`: tpcds/q68 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 ca_city: Utf8View vs Utf8; 3 bought_city: Utf8View vs Utf8; 5 extended_price: Decimal128(17, 2) vs Decimal128(38, 2); 6 extended_tax: Decimal128(17, 2) vs Decimal128(38, 2); 7 list_price: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q69_tp1_single`: tpcds/q69 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 cd_gender: Utf8View vs Utf8; 1 cd_marital_status: Utf8View vs Utf8; 2 cd_education_status: Utf8View vs Utf8; 6 cd_credit_rating: Utf8View vs Utf8)
- `gpu_tpcds_q6_tp1_single`: tpcds/q6 at tp1-single on a device: call failed: GpuHashJoin lane 0: this join's recipe copies its probe batch — the key project keeps the keys and the join below it reads the same batch — and the ABI has no copy, so neither call can run without erasing the other's input (#152)

#### batch 18 — `PCK_TEST_FILTER=gpu_tpcds_q7`, run 20260912T042106-228416, exit 1

FAILED. 0 passed; 6 failed; 0 ignored; 0 measured; 96 filtered out; finished in 12.92s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q73_tp1_single`: tpcds/q73 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 c_salutation: Utf8View vs Utf8; 3 c_preferred_cust_flag: Utf8View vs Utf8)
- `gpu_tpcds_q74_tp1_single`: tpcds/q74 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 customer_id: Utf8View vs Utf8; 1 customer_first_name: Utf8View vs Utf8; 2 customer_last_name: Utf8View vs Utf8)
- `gpu_tpcds_q76_tp1_single`: tpcds/q76 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 4 (declared vs exported: 4 i_category: Utf8View vs Utf8; 6 sales_amt: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q77_tp1_single`: tpcds/q77 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(27, 2) but found Decimal128(38, 2) at column index 2 (declared vs exported: 2 sales: Decimal128(27, 2) vs Decimal128(38, 2); 3 returns_: Decimal128(32, 2) vs Decimal128(38, 2); 4 profit: Decimal128(33, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q79_tp1_single`: tpcds/q79 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_last_name: Utf8View vs Utf8; 1 c_first_name: Utf8View vs Utf8; 2 substr(ms.s_city,Int64(1),Int64(30)): Utf8View vs Utf8; 4 amt: Decimal128(17, 2) vs Decimal128(38, 2); 5 profit: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q7_tp1_single`: tpcds/q7 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 2 agg2: Decimal128(11, 6) vs Decimal128(38, 6); 3 agg3: Decimal128(11, 6) vs Decimal128(38, 6); 4 agg4: Decimal128(11, 6) vs Decimal128(38, 6))

#### batch 19 — `PCK_TEST_FILTER=gpu_tpcds_q8`, run 20260912T042141-228493, exit 1

FAILED. 0 passed; 7 failed; 0 ignored; 0 measured; 95 filtered out; finished in 27.79s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q80_tp1_single`: tpcds/q80 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 1 (declared vs exported: 1 id: Utf8View vs Utf8; 2 sales: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q81_tp1_single`: tpcds/q81 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 c_customer_id: Utf8View vs Utf8; 1 c_salutation: Utf8View vs Utf8; 2 c_first_name: Utf8View vs Utf8; 3 c_last_name: Utf8View vs Utf8; 4 ca_street_number: Utf8View vs Utf8; 5 ca_street_name: Utf8View vs Utf8; 6 ca_street_type: Utf8View vs Utf8; 7 ca_suite_number: Utf8View vs Utf8; 8 ca_city: Utf8View vs Utf8; 9 ca_county: Utf8View vs Utf8; 10 ca_state: Utf8View vs Utf8; 11 ca_zip: Utf8View vs Utf8; 12 ca_country: Utf8View vs Utf8; 13 ca_gmt_offset: Decimal128(5, 2) vs Decimal128(38, 2); 14 ca_location_type: Utf8View vs Utf8; 15 ctr_total_return: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q82_tp1_single`: tpcds/q82 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 i_item_id: Utf8View vs Utf8; 1 i_item_desc: Utf8View vs Utf8; 2 i_current_price: Decimal128(7, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q83_tp1_single`: tpcds/q83 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 item_id: Utf8View vs Utf8)
- `gpu_tpcds_q84_tp1_single`: tpcds/q84 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 customer_id: Utf8View vs Utf8; 1 customername: Utf8View vs Utf8)
- `gpu_tpcds_q85_tp1_single`: tpcds/q85 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 substr(reason.r_reason_desc,Int64(1),Int64(20)): Utf8View vs Utf8; 2 avg2: Decimal128(11, 6) vs Decimal128(38, 6); 3 avg(web_returns.wr_fee): Decimal128(11, 6) vs Decimal128(38, 6))
- `gpu_tpcds_q8_tp1_single`: tpcds/q8 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 s_store_name: Utf8View vs Utf8; 1 sum(store_sales.ss_net_profit): Decimal128(17, 2) vs Decimal128(38, 2))

#### batch 20 — `PCK_TEST_FILTER=gpu_tpcds_q9`, run 20260912T042217-228572, exit 1

FAILED. 0 passed; 7 failed; 0 ignored; 0 measured; 95 filtered out; finished in 10.01s — `test_gpu_corpus` ran; `peacockdb_core_gpu_lib` matched nothing (0 of its gpu_tests cases).

- `gpu_tpcds_q90_tp1_single`: tpcds/q90 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(23, 8) but found Decimal128(38, 8) at column index 0 (declared vs exported: 0 am_pm_ratio: Decimal128(23, 8) vs Decimal128(38, 8))
- `gpu_tpcds_q91_tp1_single`: tpcds/q91 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 call_center: Utf8View vs Utf8; 1 call_center_name: Utf8View vs Utf8; 2 manager: Utf8View vs Utf8; 3 returns_loss: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q92_tp1_single`: tpcds/q92 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(17, 2) but found Decimal128(38, 2) at column index 0 (declared vs exported: 0 Excess Discount Amount: Decimal128(17, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q94_tp1_single`: tpcds/q94 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(27, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 total shipping cost: Decimal128(27, 2) vs Decimal128(38, 2); 2 total net profit: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q95_tp1_single`: tpcds/q95 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Decimal128(27, 2) but found Decimal128(38, 2) at column index 1 (declared vs exported: 1 total shipping cost: Decimal128(27, 2) vs Decimal128(38, 2); 2 total net profit: Decimal128(27, 2) vs Decimal128(38, 2))
- `gpu_tpcds_q99_tp1_single`: tpcds/q99 at tp1-single on a device: call failed: GpuUnload lane 0: the exported stream is not the sink's rows: Invalid argument error: column types must match schema types, expected Utf8View but found Utf8 at column index 0 (declared vs exported: 0 w_substr: Utf8View vs Utf8; 1 sm_type: Utf8View vs Utf8)
- `gpu_tpcds_q9_tp1_single`: tpcds/q9 at tp1-single on a device: call failed: GpuProject lane 0: execute_node(#128 CudfProject): [in CudfProject] CUDF failure at: /opt/conda/conda-bld/work/cpp/src/copying/copy.cu:367: Both inputs must be of the same type

### 2026-09-12 — plan tasks 2 and 3 done: the rollout and the report

**Run.** Twenty batches, 04:14–04:23, all detached and polled with `--run-status`; every run
`FINISHED, exit 1` (the corpus binary fails by design here). Per batch `test_gpu_corpus`
executed and `peacockdb_core_gpu_lib` matched nothing — the filter is a corpus test-name
prefix, so its `gpu_tests::` rung listed 577 cases and 0 matched, which with a filter set is
not a fault. 95 tests ran across the batches: the 94 survey cells and `tpch/q19`, which passed.
Batch wall time 5–40 s; the neighbour stayed at 37 GiB and no pool line appeared.

**Tally.** 89 cells failed at the sink; 2 ran clean on the device and failed on the cpu golden
section (`tpch/q17`, `tpcds/q14`: their cpu cell is disabled, so the section reads
`skipped: not enabled at this mode`); 3 failed above the sink (`tpcds/q39` #57, `tpcds/q9`
#63, `tpcds/q6` #152). Classes at the sink: three — `Utf8View → Utf8` (76 q, 204 cols),
`Decimal128(p, s) → Decimal128(38, s)` (53 q, 105 cols, 12 declared precisions), `Int32 →
Int16` (3 q). Report: `llm-wiki/reports/sink-divergence.md`.

**Per-cell rows where the device reported a class the registry row does not name** (50):

| cell | registry tickets | device at tp1_single |
|---|---|---|
| `tpcds/q1` | #163 | Utf8View at the sink; not named: #183 |
| `tpcds/q13` | #163 | Decimal128 at the sink; not named: #187 |
| `tpcds/q15` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q17` | #163 | Utf8View at the sink; not named: #183 |
| `tpcds/q18` | #65 #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q19` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q22` | #65 #163 | Utf8View at the sink; not named: #183 |
| `tpcds/q24` | #45 #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q25` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q26` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q3` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q30` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q32` | #163 | Decimal128 at the sink; not named: #187 |
| `tpcds/q35` | #97 #163 | Utf8View at the sink; not named: #183 |
| `tpcds/q37` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q40` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q42` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q43` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q45` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q46` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q52` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q55` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q56` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q58` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q59` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q60` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q65` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q68` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q7` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q76` | #115 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q79` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q8` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q80` | #65 #97 #152 #183 #189 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q81` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q82` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q85` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpcds/q91` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpcds/q92` | #163 | Decimal128 at the sink; not named: #187 |
| `tpch/aggregate-groupby` | #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/anti-join` | #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/q1` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpch/q10` | #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/q18` | #97 #152 #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/q2` | #152 #187 | Decimal128, Utf8View at the sink; not named: #183 |
| `tpch/q22` | #59 #80 #97 #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |
| `tpch/q7` | #152 #183 | Int32, Utf8View at the sink; not named: #191 |
| `tpch/q9` | #152 #183 | Int32, Utf8View at the sink; not named: #191 |
| `tpch/semi-join` | #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/shuffle-additive` | #183 | Decimal128, Utf8View at the sink; not named: #187 |
| `tpch/shuffle-additive-avg` | #163 | Decimal128, Utf8View at the sink; not named: #187, #183 |

**Not done, on purpose.** No registry edit, no ticket edit, no golden regenerated, no cell
enabled beyond the survey's own 94 at one mode. `UPDATE_CANONICAL` and `PCK_UPDATE_SECTIONS`
were absent from the environment for every run. Scratch lived in `/tmp` only.

**Two notes for whoever reads the registry next.** The `tickets` column is per query across
five modes, so "#152" on a row says nothing about `tp1_single`, where it fired on 1 of 94 cells
(and that row does not name it). And #180's message — null values under a non-nullable
declaration — is a `try_new` check the spec's premise says does not exist; it is a different
sentence from the type check and the survey's appendix does not cover it.

### 2026-09-12 — done: the branch is pushed

Prototype: no PR, no reviewer, no CI. The coordinator read the report against the spec's four
questions and the section per waiting task before committing: `563cf005` is the survey's
enablement alone, `f4990e3f` the report, the detail and the signoff. `done` is this push. The
branch is never merged; the report is the product, and the tasks after it plan against it.
