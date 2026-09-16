# join-cases — run record

Chain `ENS-join-cases`, base `master`. Task 1's branch is the chain branch, `ENS-join-cases`,
forked at `c0660fe6`. PR targets `master`.

## Dispatch 1 — 2026-09-15

- Hosts at dispatch: **verda down** for us (`Permission denied (publickey)` — host answers,
  key rejected; not diagnosed), so the rust-only check runs locally. **shad-gpu up**, a
  neighbour holding ~62 GiB of 144; the gpu lib binary's 1 GiB budget fits.
- Routing: the developer works the impl plan (`join-cases-impl.md`) task by task, one
  shad-gpu device cycle per family, foreground calls (`--build`, then `--push-binaries
  --patch`, then `--run`), never one backgrounded chain. Local rust-only proof:
  `cargo test --features rust-only -p peacockdb-core --lib` unchanged (the spec's bar).
- One target dir per worktree: cudf builds go to this worktree's own
  `target-cudf-<basename CUDF_ROOT>`, never another checkout's.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, harness gaps)

### 2026-09-15, the cases written before the first device cycle

- Cold worktree: no `target-cudf-rapids-cuda-12.2`, no `cpp/build`, no `target/`, no ccache on
  this host. The first `--build` is the cold cuDF build; started before the cases were written
  so the DataFusion stack compiles while they are.
- Plan drift: the impl plan says `declaring_view_strings` becomes `pub(super)`, but
  `test_module_layout::nothing_is_pub_super` forbids that level everywhere in `src/`. It is
  `pub(crate)`, the level the layout uses.
- The anti and mark forms under the plan's scripts are #59 in disguise: `one_probe` and
  `two_probes` carry null keys on both sides, and the device's hardcoded `EQUAL` matches them.
  A first draft ran those cases under `null_equals_null = true`; that made them agree but
  emptied them — a probe over the whole key domain leaves an anti join nothing to keep. The
  cases now take scripts with misses and no null pair (`probes_with_misses`,
  `build_with_misses`, `keyed_anti_script`, four rows), so they run under the SQL default with
  rows to compare and #59 with nothing to match. The `keyed_anti_script` first tried eight rows:
  row 4's `i32` is null, which on the composite key is a null pair again (cpu 30 rows, device
  27 on `a_left_anti_join_on_a_composite_key` — #59, not a new finding).
- The plan's composite key `(key, i32)` never matches: `i32` has a thousand values, and
  32 × 48 rows over 7 000 pairs give zero inner rows, so the case passed with nothing compared.
  `keyed` folds the second key to `i32 % 5` for `Composite` (nine values; 12 inner rows at the
  full probe, one at the four-row anti probe against 18 on the first key alone, so a device
  honouring only the first key is a different row count).
- Plan drift, `RightSemi`/`RightAnti` with a crossing projection take one probe batch: their
  second is #152's refusal, pinned without a projection already, and a second pin would answer
  an answered question. `Right`'s two-probe form is pinned under #152 as the plan says, since
  the plan asked for it by name.
- `an_inner_join_with_a_residual_over_two_probe_batches` is written pinned under #152 from the
  start (`BUILD_COPY`), as the plan's expected outcome; the device cycle verified the pin.
- `join_cases.rs` is 1483 lines after this task, past `coding-style.md`'s 1000-line rule. The
  spec and the dispatch both say the builders are local to `join_cases.rs` and nothing else is
  added; a split is a reviewer's call, not made here.

### The device cycles

Three cycles, each `--build`, `--push-binaries --patch`, `--run`, foreground, from this
worktree's `target-cudf-rapids-cuda-12.2` (cold: 57 min for the first build, 30 s to relink
after). Logs under `/tmp/join-cases/` on dev for this session only.

1. `tests::gpu_tests::join_cases`: 111 passed, 8 failed. Seven were every declared-`Utf8View`
   key case, the cpu refusing before any join; one was `a_right_anti_join_with_a_crossing
   _projection` at cpu 4 rows, device 0 — #59 (null probe keys against null build keys).
2. `tests::gpu_tests::`: 263 passed, 11 failed — the seven `Utf8View` pins (no device refusal
   anywhere but the sink), the composite anti (#59 through `i32`'s null), and the three
   nested-loop cases: the `Left` refusal (#215, new), and #190 on both projected forms.
3. `tests::gpu_tests::`: 274 passed. Then the handoff run over the whole rung,
   `PCK_TEST_FILTER='gpu_tests::'`: `test result: ok. 329 passed; 0 failed; 0 ignored;
   0 measured; 538 filtered out; finished in 17.12s`. `test_gpu_corpus` ran 0 tests under that
   filter, as every cycle; nothing here touches it.

### Harness gap: the cpu cannot take a `Utf8View` declaration over `Utf8` data

The spec's premise — "the cpu holds to the declaration and answers", true of the unload pin —
holds for `CpuUnload` alone, which slices without re-validating. Every other cpu executor
checks its batches against the declared schema and refuses `Utf8` under `Utf8View`:

- the hash join concatenates its build side under the build leaf's schema (DataFusion's
  `HashJoinExec`, `column types must match schema types, expected Utf8View but found Utf8 at
  column index 1`), and the anti finish concatenates the accumulated keys under `key_schema`
  (`cpu_backend/join.rs`, `concat_batches`, column index 0);
- the filter's `declared` check names the whole schema (`the node declares … and DataFusion
  answered with …`); the project's output `try_new` refuses at column 0; the coalesce says
  `joining the lane's batches: …`; the scatter `a lane's batch: …`.

In production the cpu's strings come from DataFusion's parquet reader under the same schema,
so this is the harness feeding data of one type under a declaration of another, not a defect
— no ticket. Consequences for the cases:

- The declared-`Utf8View` key row is read on the device only, against the cpu on a `Utf8` key
  over the same strings (`device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key`);
  `Key::Utf8` exists for that oracle and gets its own three green cases, the string key path
  no case had. The zero-row empty shape is the same reading.
- The #183 pins cannot be refusals: nothing but the sink checks the export's type, so
  `Device::fetch` hands the column up as `Utf8` and the pin asserts that type in the slot —
  `exported_type` in `join_cases.rs`, inline in the other three files. The cpu half is not read.
- Closing the gap is a `run_both` change — the cpu's upload would cast a batch to the leaf's
  declared types, which the device cannot take — outside this task's "no helper" bound.

### The `bug_` register (the known-wrong table arrives with `declared-schemas`)

| case | file | asserts | ticket |
|---|---|---|---|
| `bug_a_right_join_with_a_crossing_projection_refuses_its_second_probe_batch_on_the_device` | join_cases | `BUILD_COPY` on batch 2 under a projection | #152 |
| `bug_an_inner_join_with_a_residual_refuses_its_second_probe_batch_on_the_device` | join_cases | `BUILD_COPY` on batch 2 under a residual | #152 |
| `bug_an_inner_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device` | join_cases | slot 0 columns 1 and 9 are `Utf8` | #183 |
| `bug_a_right_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device` | join_cases | the same after the swap | #183 |
| `bug_a_left_anti_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device` | join_cases | slot 1 (the finish) column 1 is `Utf8` | #183 |
| `bug_a_projected_column_declared_utf8view_comes_back_utf8_from_the_device` | exec_cases | the copy is `Utf8` | #183 |
| `bug_a_filter_passes_a_column_declared_utf8view_back_as_utf8_from_the_device` | exec_cases | column 5 is `Utf8` | #183 |
| `bug_a_coalesce_hands_a_column_declared_utf8view_up_as_utf8_from_the_device` | accumulate_cases | the done slot's column 5 is `Utf8` | #183 |
| `bug_a_scatter_on_a_key_declared_utf8view_hands_every_lane_up_as_utf8_from_the_device` | emit_cases | four lanes, column 5 `Utf8` in each | #183 |
| `bug_an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_is_dropped_on_the_cpu` | nested_cases | cpu `number of columns(16) must match number of fields(3)`; device equals the cpu's unprojected answer projected | #190 |
| `bug_a_left_nested_loop_join_with_a_decimal_predicate_is_refused_on_the_device` | nested_cases | `non-AST-able NestedLoopJoin filter is only supported for Inner joins` | #215 |
| `bug_a_left_nested_loop_join_with_a_decimal_predicate_and_a_projection_is_refused_on_both` | nested_cases | the cpu's #190 message and the device's #215 message | #215, #190 |

Findings the matrix's right column allowed for and did not land: no wrong ordinal on any
projection (row 1 green on every type, both sides, after the swap and through the finish); no
key-type refusal on the device for `Int64`, `Date32`, a composite or a string (#45 not
reached: the recipe casts no key); both column-path residuals green on the hash join; the
cross-then-mask path green for `Inner` with the mask columns in `filter_columns` order.

### The rust-only proof

This worktree has no `testdata/{tpch,tpcds}.sf1` (generated per checkout, gitignored), so the
first `cargo test --features rust-only -p peacockdb-core --lib` failed 35 plan-golden cases at
`register the tables: … NotFound` — the dataset, not the change; nothing here compiles under
`rust-only`. With the main checkout's sf1 linked in for the run (and unlinked after):
`test result: ok. 533 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in
189.49s` — the 535 the page counts. `--test test_module_layout`, for the `pub(crate)` on
`declaring_view_strings`: `17 passed; 0 failed`.

### Not done, for the reviewer

- `join_cases.rs` at 1483 lines against the 1000-line style rule; the spec pins the builders
  to that file. A split — the key-type row and its builders into a sibling — is one commit.
- The `Utf8View` cases read the device alone (the harness gap above). A `run_both` that casts
  the cpu's upload to the leaf's declared types would let every one of them compare both
  engines; that is a harness task, not this one.
