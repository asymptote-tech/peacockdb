# decimal-precision-at-export — run record

Chain B, task 2. Branch `ENS-decimal-precision-at-export` off `ENS-utf8-everywhere` at
`0b8c05fb`; PR targets `ENS-utf8-everywhere`. Task 1 sits at `completeness approved` with its
PR #158 conflicting against master and no CI run possible until the human calls a rebase; that
rebase will reach this branch as `rebase needed(...)` in its turn.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down** (name resolution), so rust-only proofs run locally; **shad-gpu up**,
  0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from task 1's cycles; `cpp/build` present,
  `cpp/build26` absent.
- Pre-dispatch greps match the spec's baseline: 28 `output_schema` hits across `cpp`,
  `peacockdb-core`, `flatbuffers`; 64 `Utf8View|BinaryView` hits in `cpp` and `flatbuffers`;
  `DECIMAL32|DECIMAL64` in `cpp/src/gpu_executor.cpp` and `scan.cpp`; no
  `peacock_handle_schema` anywhere.
- Regeneration: `peacockdb-core/build.rs` runs the vendored flatc over `gpu_plan.fbs`, so a
  rust-only build regenerates the Rust bindings; `cpp/CMakeLists.txt:135` does the C++ side.
- Routing: the developer works `decimal-precision-at-export-impl.md` task by task — the ABI
  and export with its C++ cases, the Rust callers, the plan rule red then green, the one wire
  rebuild, `peacock_handle_schema`, then one shad-gpu cycle for the harness and one for the
  53-query rollout at `tp1_single`, then the record. Both sides rebuild together on the device
  cycle: staged binaries from before this task cannot read the plan after it.
- The spec's restriction holds: no cast, no range check, no relabel on the Rust side after
  decode; the label is set once, on the imported Arrow schema in `export_table_to_ipc`.

## Developer notes

### Dispatch 1 — the plan's tasks 1 to 4b, local, before the first device cycle

Tests first, each run red for its reason:
- The seven gtests in `test_plan_executor.cpp` (`Export.*` ×4, `ExportInternal.*`,
  `HandleSchema.*` ×2) failed to compile on the old `peacock_result_from_handle` signature,
  the absent `peacock::export_table_to_ipc` and the absent `peacock_handle_schema`.
- `common::tests::a_decimals_precision_is_declared_and_every_other_column_is_zero`
  (`src/common/tests.rs`, new child module): `unresolved import`, then green.
- `plan::validate::tests::a_decimal256_column_is_refused` and
  `a_cast_to_a_decimal256_is_refused`: `got Ok(())`, then green. The rule is one
  classifier, `plan/common.rs::unholdable`, whose `Some(reason)` both `check_expr_types`
  and `validate.rs::device_holdable_types` (was `no_view_types`) print; the view messages
  are unchanged, a decimal reads "`Decimal256(50, 2)` is not Decimal128, the one decimal
  that crosses to the device".

Deviations from the spec's letter, each with its reason:
- The DECIMAL64 refusal test lives in `test_plan_executor.cpp`, not `test_cudf_nodes.cpp`:
  `peacock_cudf_node_tests` is `EXCLUDE_FROM_ALL`, manual, sf40-bound and does not link
  `peacock_gpu`, so a case there is run by no gate. The column is built with
  `cudf::make_column_from_scalar` over a `fixed_point_scalar<decimal64>`; no cudftestutil.
- `cpp/CMakeLists.txt` links `Arrow::arrow_shared` into `peacock_plan_tests`: reading back
  the stream's schema message is the assertion, and Arrow was private to the library.
- A fifth export case, `ADeclarationOfAnotherWidthIsRefused`, pins the `n_columns` check
  the spec's item 1 requires.
- `export_table_to_ipc` asks `cudf::is_fixed_point` rather than naming DECIMAL32/64, so the
  Verification bar's grep returns `scan.cpp` alone as it says.
- The architecture sentence for "Every cast is explicit" says the precision alone is the
  label the export is told; the plan's draft also named timezone and string flavour, which
  the export is told nothing about.
- Comment-only fixes outside the Scope table, each a sentence the change made false:
  `spark_hash_partition.cu:134,177` (the view type), `test_support/result_text.rs:88-94`
  and `tests/synthetic.rs:3-5` (the widening as a present fact), and `harness_cases.rs:120`
  where a `///` above a macro invocation was the one warning in the gpu build.
- `architecture.md` also had "seventeen symbols" and the IPC-export row of the cuDF
  options table falsified; both corrected with the two sentences the plan names.
- The three `bug_` pins for #187 (`aggregate_cases.rs`, `exec_cases.rs`,
  `source_cases.rs`) became positive `same(...)` cases rather than being deleted, so the
  decimal path keeps a case per operator family; the harness count does not move.

Verification bar, local: `grep -rn output_schema cpp peacockdb-core flatbuffers` returns
the seven `check_output_schema` lines in `plan/validate.rs`, `plan/mod.rs`,
`validate/tests.rs` and `planner/pipeline.rs` — a Rust function about the root's schema
against DataFusion's, not the wire field; `grep -rnw` returns nothing. Renaming a function
to satisfy a substring grep is outside the Scope table, so it is left and reported.

### Dispatch 1 — the device cycles and the rollout

Build-script fix (outside Scope, reported): `scripts/lib/shadgpu-env.sh`'s json artifact
reader closed cargo's pipe at the first match and cargo's trailing `build-finished` line
then made `--build` die with `Broken pipe (os error 32)` and "building
peacockdb_core_gpu_lib failed". The reader now drains stdin and keeps the first match. This
is pre-existing infrastructure; it only surfaced here because the staged lib rebuilt.

Device cycles on shad-gpu, 0 MiB held at each, no `[rmm] … could not be built`:
- Harness, `PCK_TEST_FILTER='_cases'`: first run red on two harness cases that the export's
  new refusal reached — `bug_a_bare_decimal_literal_is_a_float64_column_on_the_device` (#210,
  the AST float column, now refused by name as "not a decimal") and
  `bug_a_cross_join_projection_is_dropped_on_both` (#207, sixteen columns to an export told
  two). Both were pinning the device *answering*; with the export told a precision they now
  refuse, so `Device::fetch` returns the refusal as a `BackendError` (as `unload` does) and the
  two pins assert the refusal message. Second run green: `peacock_plan_tests` 41 (the seven new
  gtests among them), `peacockdb_core_gpu_lib` `test result: ok. 229 passed`.
- Rollout, the 53 decimal queries flipped to `tp1_single` in `corpus_cases.inc`,
  `PCK_TEST_FILTER='_tp1_single'`: `test_gpu_corpus 10 passed; 48 failed` (10 = pre-existing
  greens + registry, red by construction while the csv was untouched). No `declared vs
  exported`, no precision refusal, no `narrow fixed_point` anywhere in the log — the export
  fix took at every cell. The 48 failures are the next cause behind the decimal, classified
  below.
- Final, csv and corpus edits applied (5 enabled): `peacockdb_core_gpu_lib 289 passed`,
  `test_gpu_corpus 16 passed` (14 corpus cells + regen + `the_registry_matches_the_gpu_corpus`).

### Rollout — 53 decimal queries at tp1_single

Each cell's outcome read from the sink message or the golden's node line at the reported line.
No cell fails on the decimal any more; the "next cause" is what the string class hid, exactly
as `utf8-everywhere`'s rollout found.

| verdict | queries | registry |
|---|---|---|
| green, enabled | tpch aggregate-groupby, filter-project, shuffle-additive; tpcds q37, q82 | `gpu_tp1_single` enabled |
| golden, in_rows at a BatchAccumulator (#185) | tpcds q3 q8 q15 q33 q40 q42 q43 q46 q52 q55 q56 q58 q59 q60 q61 q68 q76 q77 q79 q80 q90; tpch hash-join q10 q18 | `185` added, `187` struck |
| golden, join batching (#220) | tpcds q16 q19 q25 q45 q91 q94 q95; tpch anti-join semi-join q2 | `220` added, `187` struck |
| device clean, cpu off (#163) | tpcds q7 q18 q24 q26 q30 q65 q81 q85; tpch q1 q22 shuffle-additive-avg | `187` struck (q13 q32 q92 carried 163 alone, unchanged) |

`187` stays on `tpch/filter-project` alone: enabled at tp1_single, its other four modes never
run and carrying no other ticket, so it is the marker the registry rule requires — the #183
precedent. Every other of the 50 rows lost `187`.

Two #185 cells are out of sample at `GpuAccumulateBatchesAndSort` (tpcds q46, q59): the same
"in_rows = own output" rule #185 states generally, at a sibling BatchAccumulator rather than
`GpuAggregateBatches`. Attributed to #185, noted here rather than opening a duplicate.

### Verification bar

- rust-only (local, verda down, `--test-threads=2`): `--lib` `543 passed; 2 ignored` (545);
  `test_module_layout` 17; `test_ci_coverage` 8; `test_golden_format` 26; `test_cost_model` 3;
  `test_corpus_goldens` 20; `test_cpu_corpus` `448 passed` (registry cross-check among them).
  The validation Decimal256 tests red (`got Ok(())`) then green.
- C++: `ctest -L cpu` `100% tests passed`; the seven new plan-executor cases green on the
  device (`Export.*` ×4, `ExportInternal.ADecimal64ColumnIsRefusedByName`, `HandleSchema.*` ×2).
- device: harness green (`Device::fetch` with declarations); rollout applied, `test_gpu_corpus
  16 passed` including the registry cross-check, on the real 5-enabled corpus.
- greps: `grep -rnw output_schema cpp peacockdb-core flatbuffers` → nothing (the wire field is
  gone); `grep -rn "DECIMAL32\|DECIMAL64" cpp/src` → `scan.cpp` alone; `grep -rn
  "Utf8View\|BinaryView" cpp flatbuffers` → nothing.

**Drift reported (code authoritative):** the spec's grep `grep -rn "output_schema"` expects
nothing, but the substring survives in the unrelated `check_output_schema` (the root's schema
against DataFusion, `plan/validate.rs`, `plan/mod.rs`, `planner/pipeline.rs`) — a pre-existing
validator, not the wire field. Renaming it is outside the Scope table and cosmetic. The
whole-word grep is clean, which is the meaningful check.

### Re-proof after the rebase

Local, verda down, shad-gpu 0 MiB held; nothing had been built since the replay.

`exec_cases.rs`: master's import superset is right as taken — every name is used — but the
replay kept this branch's deletion of `batch_of` while master's #191 and #219 pins call it, so
the helper is restored. `bug_a_cast_to_decimal_is_exported_at_precision_38` inverted to
`a_cast_to_decimal_is_exported_at_its_declared_precision` (`same(...)`, as the scan's and the
arithmetic's pins were): no positive cast-to-decimal case existed to retire it against, so the
count does not move.

First device run red on two of master's #216 pins in `aggregate_dimension_cases.rs`,
`bug_a_global_welford_init_answers_a_finished_stddev_on_the_device` and
`bug_a_keyless_welford_merge_answers_the_stddev_of_its_counts_on_the_device`: both pinned the
device answering one finished `Float64` where the plan declares the Welford triple, and the
export, now told three declarations, refuses with `3 declared precisions for a table of 1
columns` — the #207 shape. Converted as #207 and #210 were: `gpu_refuses()` asserting that
message, the cpu's three columns kept; the finished value is no longer observable through the
harness. Three imports dropped with the arithmetic. #216 is unchanged and its ticket still
names both pins; the underlying defect is the ticket's, not the rebase's.

Proofs, rust-only (`--test-threads=2`, goldens unmoved — `git status --short testdata/` empty):
`--lib` `test result: ok. 552 passed; 0 failed; 2 ignored` (554 listed);
`test_module_layout` `ok. 17 passed`; `test_golden_format` `ok. 26 passed`; `test_cost_model`
`ok. 3 passed`; `test_corpus_goldens` `ok. 20 passed`; `test_ci_coverage` `ok. 8 passed`;
`test_cpu_corpus` `ok. 451 passed`. Recompiled, no warnings. Greps: `-rnw output_schema` →
nothing once `cpp/build` regenerated its header (the only hits before the build were the stale
`gpu_plan_generated.h`); `DECIMAL32|DECIMAL64` → `scan.cpp:106` alone; views → nothing.

C++: `ctest -L cpu` in `cpp/build` `100% tests passed`; `peacock_cpu_tests` `[  PASSED  ] 12`.

Device, cycle two (unfiltered, after the #216 conversion; cycle one was `PCK_TEST_FILTER='gpu_'`
and red on the two pins alone, `387 passed; 2 failed`): `peacock_cpu_tests` 12,
`peacock_gpu_tests` 6, `peacock_plan_tests` 41 (`Export.*` ×4, `ExportInternal.*`,
`HandleSchema.*` ×2 each `OK`), `peacock_tpch_tests` 4, `peacock_tpchv_tests` 4;
`peacockdb_core_gpu_lib` `test result: ok. 389 passed; 0 failed; 557 filtered out`;
`test_gpu_corpus` `test result: ok. 16 passed; 0 failed`. No `could not be built`, no warnings.
`a_device_run_under_a_regeneration_writes_no_golden` is the corpus case a `gpu_` filter cannot
select, which is why cycle two ran unfiltered.

`build-test.md` counts against `--list`: `--lib` 554; `gpu_tests::` 389 = harness 334 + recipe
walk 10 + murmur 10 + executors on a device 31 + abi 4; `test_gpu_corpus` 16; C++ 12 and 41;
"Plan types" 38 = `validate::tests` 30 + `layout::tests` 4 + `aggregate::tests` 4. Header 1976
sums. Nothing to correct.

## Reviewing, then rebase needed — 2026-09-17

Dispatch 1 committed as `ddafdb77` on `6bfaa9e9`, pushed; PR #159 against
`ENS-utf8-everywhere`, base verified, 2 commits. The control file said `rebase` as the developer
returned, so before any reviewer: task 1 (`ENS-utf8-everywhere`, `completeness approved`) is
rebased onto master first and carried through CI to `done`; this branch then rebases onto it,
re-proves, and only then goes to review. Until then `rebase needed(reviewing)`.

## Rebased onto the rebased task 1 — 2026-09-17

Task 1 reached `done` on master's tip (`dd2fb406`, PR #158 green), so this branch replayed its
three commits onto it (`--onto`, from the old fork `0b8c05fb`). Resolved by ownership:
`build-test.md` counts take task 1's rebased numbers plus this task's deltas (header 1976, cpu
1028 with `--lib` 554, gpu 405 with `gpu_tests::` 389 and `test_gpu_corpus` 16, harness 334 —
**arithmetic, to be confirmed against `--list`**); #187 this task's closed form; the corpus
comments and csv rows this task's state, q77 keeping master's `212` beside this task's `185`;
`exec_cases.rs`'s import block master's superset, unverified. The join-batching ticket is
**#220** here too — master took #215 meanwhile — renumbered in the csv, the comments, the
rollout table. **Master added a #187 pin this task falsifies**:
`bug_a_cast_to_decimal_is_exported_at_precision_38` (`gpu_tests/exec_cases.rs`) asserts the 38
the export no longer writes; the developer retires or inverts it as the task did for the scan's
pin, and the counts move with it. Pre-rebase tip: tag `pre-rebase/decimal-precision-at-export`.

## Completing — 2026-09-17

Review round 1 on the rebased branch: 0 blocking, 2 important, 7 nits. Both importants were
the coordinator's — `build-test.md` had no row for `common::tests` (header 1976 against tables
summing 1975; row added, sums now) and `peacock_gpu.h`'s doc above `peacock_result_from_handle`
ran 14 lines against the ten cap (trimmed). Nits taken here: the three corpus comments that
narrated the transition now state the cause; `build-test.md`'s "three per-call symbols" says
three of the four, `handle_schema` having no Rust caller; #187 back within the cap. Nits
deferred, to ride with the next developer dispatch if one happens and otherwise dropped:
`Export.NoDeclarationExportsAtTheWidthsMaximum` proves the `n_columns == 0` escape rather than a
per-entry `0` on a decimal column (pass `{0}, 1`); `Export.ADeclarationOfAnotherWidthIsRefused`
means the array length, not a decimal width (rename). Recorded as a deviation from the spec's
letter: `n_columns == 0` with a null array declares nothing — `abi.rs`, the wire session and the
C++ tests have no schema to size an array from — where the spec says the count must equal the
column count or the call fails; every non-zero count is still checked. The reviewer's note that
the export's new code had compiled on 25.02 alone is CI's 26.02 leg to answer.

## Completeness pass — analyst, what is missing — 2026-09-17

Read as one change against `ENS-utf8-everywhere` (6 commits, 39 files). 0 blocking, 2 important.

**Spec items, sub-part by sub-part.** Item 1: `int32_t` array and count at all three sites
(`peacock_gpu.h`, `gpu_executor.cpp`, `ffi/src/lib.rs`). Item 2: label set on the imported
schema between `ImportSchema` and `ImportRecordBatch`, scale off the column; widening loop and
comment gone; both `runtime_error`s name the column; count check present, with the recorded
`n_columns == 0` escape; `export_table_to_ipc` non-static in `plan_executor_internal.h`. Item 3:
`unload` builds from `self.schema`, `concat_batches` untouched; `Device::fetch` takes
`Option<&Schema>`, zeros on `None`; the walk's `Session::export` passes zeros. Item 4: one
classifier `unholdable` reaches node schema, intermediate schema, literal, binary out type, cast
target and scalar return; `Decimal256` refused with two unit tests; `fb_to_type_id` maps
`Decimal128` alone; `DECIMAL32|64` in `cpp/src` is `scan.cpp:106` only. Item 5: both fbs
fields, both `None`s, the union block (dead on every driven path — the writer's `CudfUnion` is
structural and the driver makes no call), `make_plan_node`'s parameter, the `node_session.cpp`
comment, both enum values, the four view arms the parent still had (task 1 took the fifth),
the 45 renames, `peacock_handle_schema` in header, executor and extern with its two gtests.
`test_cudf_nodes.cpp` deviation justified: `EXCLUDE_FROM_ALL`, no gate runs it.

**Verification bar.** Every tier has a `test result:` (or ctest `100%`) line after the rebase:
rust-only local with verda down and said so; `ctest -L cpu` in `cpp/build` (25.02); device on
shad-gpu, cycle two unfiltered. The three greps re-run here and hold (`-rnw output_schema`
empty; the substring survives only in `check_output_schema`). 26.02: `cpp/build26` absent, no
local compile; PR #159's `CI Pipeline (cudf 26.02)` was in progress at reading time — the wait
at `done` is what answers it.

**Registry.** All 50 changed csv rows match the rollout table (5 enabled, 24 → `185`, 10 → `220`,
11 → `163` with cpu off; q13 q32 q92 unchanged on `163`); `187` survives on `tpch/filter_project`
alone; 14 enabled `gpu_` cells = q6 ×5 + nine at `tp1_single`, so `test_gpu_corpus` 16 =
14 + regen + registry. `build-test.md` header 1976 = Rust 1526 + C++ 81 + Python 369; blocks
1028/5/405 sum; `--lib` 554 = 551 + `common::tests` 1 + `validate::tests` 2; harness 334 =
322 `operator_case!` + 12 `#[test]`; plan-executor 41 `TEST(` in the file; cpu gtests 12.
`test_ci_coverage` needs no change: no new `--test` target, gtests run by glob.

**Tickets.** Counter 221 = highest anchor #220 + 1. #187's Done line is the #183 shape and its
cell arithmetic checks (5 + 24 + 10 + 14 = 53). #207, #210, #216 stay true: each describes the
node's output, which is unchanged; only where the harness observes it moved (export refusal).
#185 and #220 are the important finding below.

**Handoff to `device-schema-harness`.** `peacock_handle_schema(executor, handle, out_ipc,
out_len)`: schema message alone via `MakeStreamWriter` + `Close()`, `malloc`'d, freed by
`peacock_result_free` (`std::free`), 0 on success, extern in `peacockdb-ffi/src/lib.rs`, no Rust
caller — exactly what task 5's spec assumes. Eighteen exported symbols counted in
`gpu_executor.cpp`.

**Not scored, for the coordinator's `architecture.md` pass** — three sentences the branch
falsified and did not touch: the `decimal scale` row of the cuDF options table still names
`union.cpp` as a site (the deleted block was the only one); `## Column indexing`'s "48
`.column(…)` calls" is 49 (the export reads `tview.column(i)` twice); `### What guards it`'s
"six sites indexing names use `operator[]`" is eight (`gpu_executor.cpp:79,83`, the refusal
messages). The five edits the branch made are true and complete. `build-test.md`'s "Plan
types" prose still says "names a view type" only, where the row now also counts the two
`Decimal256` refusals — a clause, not a count.

## Completeness pass — 2026-09-17

Two blind readings. **Reviewer (what is wrong): 0 blocking, 1 important** — three places still
described the export that widens: `scan.cpp`'s loader comment ("subsumes the widening the
export also does"), `peacock_gpu.h`'s preamble (IPC crosses at one door; the failure policy's
"four doors") and #210 in `tickets.md` (the device "answers" a Float64 where the export now
refuses it by name). **Analyst (what is missing): 0 blocking, 2 important** — the same
`scan.cpp` comment, and #185 (22 rows stated, 46 in the csv) and #220 (no line for this
rollout's ten). `architecture.md` falsified by the branch and not corrected: the decimal-scale
row still named `union.cpp`, and Column indexing's counts (49 `.column(…)` calls, eight
name-indexing sites) — all three confirmed by counting the tree. Every item comment-only or
markdown; the coordinator applied them all, no developer round. Not scored, recorded: item 4's
"`Decimal32`/`Decimal64` automatically if arrow ever adds them" is a `Decimal256` match today,
arrow-schema 54.3.1 having neither variant nor an `is_decimal` predicate. The analyst's evidence
trail is the section "Completeness pass — analyst, what is missing — 2026-09-17" below.
