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

The seven #183 rows below were retired by Task 8 (see its section); the register is kept
as the record of dispatch 1.

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
reached: the recipe casts no key); the residuals green on the hash join on both of its paths
(the AST for a cross-side string equality, the column path for a string literal and a
decimal cast — see review round 1); the cross-then-mask path green for `Inner` with the mask
columns in `filter_columns` order.

### The rust-only proof

This worktree has no `testdata/{tpch,tpcds}.sf1` (generated per checkout, gitignored), so the
first `cargo test --features rust-only -p peacockdb-core --lib` failed 35 plan-golden cases at
`register the tables: … NotFound` — the dataset, not the change; nothing here compiles under
`rust-only`. With the main checkout's sf1 linked in for the run (and unlinked after):
`test result: ok. 533 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in
189.49s` — the 535 the page counts. `--test test_module_layout`, for the `pub(crate)` on
`declaring_view_strings`: `17 passed; 0 failed`.

### 2026-09-16, Task 8: the view cases retired

The spec declares no view type, so every case that declared `Utf8View` tested a declaration
the engine is about to stop making, and each had a `Utf8` twin. Retired, one line each:

- `an_inner_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key` —
  `join_dimension_cases.rs`; twin of `an_inner_join_on_a_utf8_key_agrees`.
- `a_right_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key` — twin of
  `a_right_join_on_a_utf8_key_agrees`.
- `a_left_anti_join_on_a_declared_utf8view_key_answers_on_the_device_as_on_a_utf8_key` —
  twin of `a_left_anti_join_on_a_utf8_key_agrees`.
- `bug_an_inner_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device`,
  `bug_a_right_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device`,
  `bug_a_left_anti_join_keeping_a_declared_utf8view_key_hands_it_up_as_utf8_from_the_device`
  — the #183 pins at the join; the unload pin in `harness_cases.rs` is the one #183 keeps.
- `bug_a_filter_passes_a_column_declared_utf8view_back_as_utf8_from_the_device`,
  `bug_a_projected_column_declared_utf8view_comes_back_utf8_from_the_device` —
  `exec_cases.rs`, the same reason.
- `bug_a_coalesce_hands_a_column_declared_utf8view_up_as_utf8_from_the_device` —
  `accumulate_cases.rs`, the same reason.
- `bug_a_scatter_on_a_key_declared_utf8view_hands_every_lane_up_as_utf8_from_the_device` —
  `emit_cases.rs`, the same reason.
- `an_inner_join_on_a_declared_utf8view_key_over_a_zero_row_probe_answers_zero_rows_on_the_device`
  is kept as `an_inner_join_on_a_utf8_key_over_a_zero_row_probe_answers_zero_rows`: `Key::Utf8`,
  whole outputs compared through `.same()`.

Helpers gone with them: `Key::Utf8ViewDeclared`, `Key::declared_type` (`keyed_side` reads
`data_type`), `keyed_zero_row_probe` (its one script is inline in the kept case),
`without_key`, `device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key`,
`exported_type`; the `declaring_view_strings` imports in the three operator files, and the
function itself is private again. The harness gap recorded above is now moot for this branch:
no case feeds `Utf8` data under a `Utf8View` declaration but the unload pin, which slices
without re-validating. Plan drift: `run_gpu` never existed under `src/tests`; the plan's
step-3 command puts `--no-run` after `--`, where the test binary rejects it — the compile it
asks for happens before that and is what the step proves.

Environment, for whoever builds here next: this worktree's directory was renamed from
`peacockdb-ENS-drop-mode-name`, and both `target/` and `target-cudf-rapids-cuda-12.2/` still
carried that absolute path in the `output`/`root-output` of the build scripts
(`flatc-fork`, `zstd-sys`, `bzip2-sys`, `lzma-sys`, `blake3`, `psm`, `peacockdb-core`, and
`peacockdb-ffi` in the cudf dir). Cargo's fingerprint does not see a rename, so
`peacockdb-core`'s build script ran a `flatc` at the old path: `failed to run flatc: No such
file or directory`. Fixed by `cargo clean -p` on those packages in each target dir
(`scripts/cargo-cudf.sh clean -p …` for the cudf one); the DataFusion stack stayed warm.

Proofs:

- shad-gpu, `PCK_RUN_CPP=0 PCK_TEST_FILTER='_cases' … --run`: `test result: ok. 262 passed;
  0 failed; 0 ignored; 0 measured; 596 filtered out; finished in 1.54s`.
- shad-gpu, the whole rung, `PCK_TEST_FILTER='gpu_tests::'`: `test result: ok. 320 passed;
  0 failed; 0 ignored; 0 measured; 538 filtered out; finished in 3.49s` — 330 less the ten.
  `test_gpu_corpus` ran 0 tests under both filters, as every cycle. No neighbour on the card.
- local, `cargo test --features rust-only -p peacockdb-core --lib`: `test result: ok. 533
  passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 54.22s`.
- local, `--test test_module_layout`: `test result: ok. 17 passed; 0 failed; 0 ignored;
  0 measured; 0 filtered out; finished in 0.12s`.
- The gpu-feature build of the lib binary (`--build`) and the rust-only build both finished
  with no warning. `grep -rn -i utf8view peacockdb-core/src/tests/gpu_tests/` hits
  `harness_cases.rs` alone.

`build-test.md` moved by ten: grand total 1873 → 1863, Rust 1437 → 1427, the gpu block
338 → 328 with `--lib -- gpu_tests::` 330 → 320, the harness row 275 → 265; the row's
declared-`Utf8View` sentences replaced by what is there now.

### Not done, for the reviewer

- `join_cases.rs` at 1483 lines against the 1000-line style rule; the spec pins the builders
  to that file. A split — the key-type row and its builders into a sibling — is one commit.
- The `Utf8View` cases read the device alone (the harness gap above). A `run_both` that casts
  the cpu's upload to the leaf's declared types would let every one of them compare both
  engines; that is a harness task, not this one.

## Coordinator — 2026-09-15, to reviewing

Committed as `9c0fd6b7`, PR #155 against `master` (base verified, 2 commits). Reviewer round 1
dispatched. Spec-side judgements for the reviewer and the analyst: the harness gap (cpu refuses
`Utf8` under a `Utf8View` declaration everywhere but the unload) is recorded above, not
ticketed; `join_cases.rs` at 1483 lines is a known style overrun the spec's "builders local to
that file" produced.

## Review round 1 — 2026-09-15

Reviewer: 43 cases counted, every matrix row and empty shape present, `bug_` cases precise,
counts consistent, the `Utf8View` device-only reading accepted as answering the spec's question,
the extra local functions judged ordinary file-local builders and not the harness mechanism the
spec's "no other helper" clause guards. Four findings, routed to the developer (1–3) and the
coordinator (4):

1. important, `join_cases.rs:86-87` and the string-residual case: `b_s = p_s` is AST-able
   (`is_ast_able` only forces the column path for a string *literal*), so the case ran the AST
   path, not the column path the spec's row 3 names; comment, detail file and #215's wording
   ("a decimal or string operand") repeat the premise. Fix: a literal operand to reach the
   column path, or rename and record; #215 to say "string literal".
2. important, `join_cases.rs` at 1483 lines against the 1000-line rule. Split by
   responsibility into a sibling file; the kind guard reads a registry, not files, so it does
   not move. Coordinator's ruling: the spec's "local to `join_cases.rs`" meant "not in the
   shared harness"; a builder local to the sibling that holds the cases using it honours that.
   Recorded here as a deliberate deviation from the spec's path list for the signoff.
3. nit, the device-only oracle helper discards `device.cpu` unasserted: assert it is the
   refusal, so a `run_both` that closes the gap turns these cases red and says so.
4. nit, `build-test.md` harness row is one 195-word sentence: the coordinator splits it.

### Review round 1, addressed

1. The string residual. `is_ast_able` (`expr.cpp:415-435`) refuses a string only as a
   *literal* operand; `b_s = p_s` over two STRING columns is admitted and `build_column`
   hands it to `cudf::compute_column`. The case is now two: `an_inner_join_with_a_string_
   residual_on_the_ast_agrees` (`b_s = p_s`) and `an_inner_join_with_a_string_literal_residual_
   on_the_column_path_agrees` (`b_s = p_s AND p_s <> ''`). Read under `PEACOCK_GPU_DEBUG=1`
   with `PCK_TEST_FILTER='string_'`: the AST case makes one `build_column kind=Binary` call
   and nothing beneath it; the literal case recurses — the `And`, its left `Binary` (the
   equality, an AST subtree, no `ColumnRef` under it), its right `Binary` then
   `ColumnRef idx=1 type_id=23` — the column path at the top and at the literal comparison.
   A cross-side string comparison alone cannot reach the column path: it is AST-able by the
   C++'s own rule. #215 reworded to "a decimal operand or a string literal". One case added:
   the rung is 330.
2. The split. `join_dimension_cases.rs` (618 lines) holds rows 1–3 with `Key`, `keyed`,
   `hash_join_keyed`, the keyed scripts, `crossing_projection`, `without_key`, the three
   residuals, `probes_with_misses`, `build_with_misses`, the oracle helper and `exported_type`;
   `join_cases.rs` (926) keeps task 9's cases and the builders both files use, now
   `pub(crate)`: `build_batch`, `probe_batch`, `side`, `padded`, `residual`, `hash_join_with`,
   `hash_join`, `script`, `one_probe`, `two_probes`, `empty_build`, `gpu_refuses_with`,
   `BUILD_COPY`. `mod.rs` gains the line; the kind guard is untouched.
3. `device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key` asserts
   `device.cpu.is_err()` with the message "the cpu takes the declaration now: read this case
   with .same()".

Proof: `PCK_TEST_FILTER='gpu_tests::' … --run` → `test result: ok. 330 passed; 0 failed;
0 ignored; 0 measured; 538 filtered out; finished in 16.88s`; `--test test_module_layout`
→ `17 passed`; rust-only `--lib` (sf1 linked in, unlinked after) → `533 passed; 0 failed;
2 ignored`. `build-test.md` not touched this round: the harness row and the gpu counts
move by one (274 → 275, 329 → 330, 337 → 338, 1872 → 1873, Rust 1436 → 1437).

## Review round 2 — 2026-09-16

All four round-1 findings closed as stated (the literal residual traced through `expr.cpp` to
the column-scalar path; the split byte-identical on the moved cases; thirteen `pub(crate)`
items, each imported). Two nits from the split — a "pinned above" that now points at the other
file, a missing blank line — fixed by the coordinator as comment-only edits, with a
cross-reference added to `join_cases.rs`'s top comment. Nothing important outstanding: to
`completing`.

## Completeness pass — 2026-09-16

Reviewer (what is wrong): clean, 0/0. Analyst (what is missing): 0 blocking, 2 important, both
record items, applied by the coordinator:

1. **Handoff to `aggregate-cases`.** Its spec's `Utf8View` group-key row and the constraint
   "every `Utf8View` group-key case but the one pin projects the key out" rest on the premise
   this branch showed false: the cpu refuses `Utf8` data under a `Utf8View` declaration
   everywhere but the unload (`cpu_backend/mod.rs` `declared_as` → `try_new`, reached by the
   aggregate at `mod.rs` and `accumulate.rs`). That row takes the reading used here — the
   device under the declaration against the cpu on `Utf8` over the same strings, with the
   cpu's refusal asserted — unless a `run_both` that casts the cpu's upload lands first.
   Written into `aggregate-cases-detail.md` too.
2. **Which fix turns the seven operator-level #183 pins red.** They read the type
   `Device::fetch` takes off the IPC stream, not a sink refusal; a cast at the Rust sink would
   leave them green. #183 now names the export in `gpu_executor.cpp` as the fix site.

`architecture.md` corrected on the analyst's list: the `CudfNestedLoopJoin` row and its
paragraph under "From flat buffer to cuDF call", and the "Inner / Left, non-equi" row under
"Join types and NULL key equality" — the conditional join is the ordinary path, the
cross-then-mask path is the non-AST-able fallback and is Inner-only (#215). Pre-existing drift
the branch put on the record.

For the helper, outside this task: `build-test.md`'s header says C++ 67 and its rows sum to 66;
on master before this branch, untouched here.

## Done — 2026-09-16

CI green on the code as shipped: run for `9e38c41b` (the last commit touching `.rs`) — both
dataset-matrix legs, the 25.02 GPU build, GPU tests on shad-gpu, cost report, S3 check all
success. The head run for `7a59490f` is documentation-only and skipped by design.

## Reopened — 2026-09-16, rebase onto master `fafaaaaf`

The human dropped both chain tasks from `done` to `building` on master: the specs no longer
declare `Utf8View` (commit `c6b7f63c`), and each plan gained a Task 8 that retires the view
cases. The control file said `rebase`. `ENS-join-cases` rebased onto `fafaaaaf` with no code
conflict — the board took master's side at every step, and `join-cases-impl.md` kept both
master's "Amended" note and the branch's ticks. Master's four commits touch
`cpp/src/gpu_executor.cpp` (a comment) and `scripts/`, so this is not a documentation-only
rebase: the task re-proves. `aggregate-cases` marked `rebase needed(building)`; it is rebased
onto the new `ENS-join-cases` only after this task is back to `done`.

Outstanding: Task 8 of the plan, then the proving commands; PR #155 stays, CI re-runs on the
new head.

## Dispatch 2 — 2026-09-16, Task 8

- The control file said `rebase` again at 19:28 (after the reopening commit at 19:23). The
  lowest branch already sits on master's head `fafaaaaf` and `aggregate-cases` is marked, so
  the rest of the protocol is this task's remaining workload first, then the child's rebase.
  File cleared.
- Hosts at dispatch: **verda down** (`Could not resolve hostname`), so the rust-only check
  runs locally; `testdata/{tpch,tpcds}.sf1` are symlinks here now and resolve, no linking
  needed. **shad-gpu up**, 0 MiB of 144 GiB held — no neighbour.
- Caches: `target-cudf-rapids-cuda-12.2` (22 GB) is warm from the first run; `cpp/build26`
  is absent, so `--build` re-runs cmake there.
- Plan drift seen before dispatch: `run_gpu` exists nowhere under `peacockdb-core/src/tests`
  (the plan's step 2 already says to grep first); the device-only oracle is
  `device_on_a_declared_utf8view_key_answers_as_the_cpu_on_a_utf8_key` in
  `join_dimension_cases.rs`, and it goes with the cases that call it.
- Routing: Task 8's six steps as written, one device cycle over `_cases`, then the rust-only
  `--lib` and `test_module_layout` proofs; `build-test.md` counts and the three
  `Utf8View` sentences in the harness row are the developer's in the same change.
