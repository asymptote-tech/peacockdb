# case-expectations — run record

Chain E, task 3. Branch `ENS-case-expectations` off `ENS-aggregate-cases` at `15a713b9` (tasks
1 and 2 done, PRs #155 and #156 green, both rebased onto master `302d91dc` today); PR targets
`ENS-aggregate-cases`.

## Dispatch 1 — 2026-09-16

- Hosts: **verda down** (`Could not resolve hostname`), so the rust-only proofs run locally;
  the sf1 symlinks in this worktree resolve. **shad-gpu up**, 0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from aggregate-cases' Task 8; `cpp/build26`
  absent, so `--build` re-runs cmake there (minutes, not the cold hour).
- Routing: the developer works `case-expectations-impl.md` task by task — the comparator, the
  composite-key pin, the fixture, the record — with the rust-only red-then-green for Task 1
  local, and one shad-gpu device cycle over `_cases` (foreground `--build`,
  `--push-binaries --patch`, `--run`, each under a timeout) proving Tasks 2 and 3 together;
  then the whole `gpu_tests::` rung once as the handoff run. Rust-only `--lib` and
  `test_module_layout` local.
- The spec's restriction is the one to hold: a case that goes red under a tightened
  expectation is a ticket and a `bug_`, never a fixture restored, and `WELFORD_RELATIVE` does
  not move.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, harness gaps)

### 2026-09-16, the three expectations

- **Plan drift, where the comparator's tests live.** `tests/gpu_tests/` is declared
  `#[cfg(all(test, feature = "gpu"))]`, so a `#[should_panic]` test beside
  `same_within_welford` in `aggregate_cases.rs` cannot run rust-only, which the plan's Step 1
  and Step 3 and the dispatch both ask for (the plan's "`-- tests::gpu_tests::aggregate_cases`
  compiles" runs 0 tests under `rust-only`, as it must). The comparator reads two
  `RecordBatch`es and nothing of a device, so `same_within_welford`, `close` and
  `WELFORD_RELATIVE` (value unchanged, `1e-9`) moved to `tests/compare.rs`, the tier that
  already tests `assert_same` against itself, and `names_and_types` is reused there without a
  visibility change. `aggregate_cases.rs` and `aggregate_dimension_cases.rs` import them from
  `crate::tests::compare`; no case body in either file changed for it. This puts one file
  outside the spec's scope table; the alternative — the two tests in the gpu rung, red observable
  only on shad-gpu — would have met the scope and not the rust-only proof. The coordinator's call
  if it should go back.
- **Task 1, red then green.** With the tests in place and the comparator as it was:
  the renamed column did not panic at all (`test did not panic as expected`), and the retyped one
  panicked inside arrow's downcast (`panic message: "primitive array"`, expected `Float64`) —
  the comparator read column 1 `as_primitive::<Float64Type>()` blind. The check landed as an
  `assert_eq!` over `names_and_types` of both batches at the top of the function, before the
  exact projection; the message is the `schema differs` shape `same_slot` uses.
- **Task 2.** `composite_anti_script_with_a_null_second_key()` is `keyed(32, 11)` against
  `keyed(8, 3)` on `Key::Composite`, the eight-row probe join-cases cut to four. The pin reads
  every slot of each side concatenated (LeftAnti emits at the finish alone) and asserts cpu 30
  rows, device 27, and that each of the three rows the device alone dropped has a null `b_i32`
  — the mechanism, not just the count. Green on the first cycle; `hash_join_keyed` stayed as
  it was (no `null_equals_null` parameter), so the oracle form the nine `join_cases.rs` pins
  use was not available without touching the other callers.
- **Task 3.** `welford_partial`'s count column is `u64::from(s.column(4).is_valid(row))`; the
  mean stays `s.column(4)` (null where null), m2 0. Under it: both grouped finalizes green
  (`a_grouped_stddev_finalize_agrees_within_welford`, `…var…`, now also under the exact
  name-and-type check), `bug_a_welford_merge_exports_its_count_as_int64` green (#163's pin,
  through `welford_answered` → `same_within_welford`), `bug_a_global_stddev_finalize_is_refused
  _on_the_device` and `bug_a_keyless_var_merge_is_refused_as_unsupported_on_the_device` green
  (refusals, fixture-independent). So the phantom-zero agreement hid no divergence on the
  grouped merge: DataFusion's `VarianceAccumulator::merge_batch` skips a zero count and cuDF's
  `merge_m2` weights the null mean by it. No ticket, no new `bug_`.
- **One re-pin, no fixture restored.** `bug_a_keyless_welford_merge_answers_the_stddev_of_its
  _counts_on_the_device` (#216) asserted `0.0`, a literal that was a function of the all-ones
  fixture. Cycle 1, the device verbatim: `| 0.2439750182371333 |` against the pinned `0.0` —
  `sqrt(3.75 / 63)`, the sample stddev over the 64 counts (60 ones, 4 zeros: rows 12 and 25 of
  each 32-row partial carry the null `f64`). The mechanism the ticket names is unchanged, so the
  pin now computes the counts' sample stddev from its own arrivals and asserts the device's one
  value against it with `close`; its comment says the counts are 1 and 0 rather than all 1.
  The register row in `aggregate-cases-detail.md` says both values.
- **Cycles**, from this worktree's `target-cudf-rapids-cuda-12.2`, each `--build` (the first
  re-ran cmake into `cpp/build26`, ~5 min; the second a relink), `--push-binaries --patch`,
  `--run`, foreground under `timeout`; logs `/tmp/case-expectations/` on dev, this session only.
  No `[rmm]` line in any run. The C++ suites were skipped (`PCK_RUN_CPP=0`): nothing here
  touches C++ and the sf40 pair would not fit a 590 s bound.
  1. `PCK_TEST_FILTER='_cases'`: `test result: FAILED. 327 passed; 1 failed` — the keyless pin.
  2. `PCK_TEST_FILTER='_cases'`: `test result: ok. 328 passed; 0 failed; 0 ignored; 0 measured;
     598 filtered out; finished in 1.73s`.
  3. `PCK_TEST_FILTER='gpu_tests::'`: `386 passed; 0 failed` (3.61s). Then two unused imports
     the re-pin left in `aggregate_dimension_cases.rs` surfaced under `cargo-cudf.sh … --no-run`
     — `build-test-shadgpu.sh`'s log shows cargo's `Compiling`/`Finished` lines and not its
     warnings, so read the build's warnings there, not in the script's log — dropped, rebuilt,
     pushed, and the handoff run repeated:
  4. Handoff, `PCK_TEST_FILTER='gpu_tests::'`: `test result: ok. 386 passed; 0 failed; 0 ignored;
     0 measured; 540 filtered out; finished in 3.67s` — 385 + the composite pin; the 540 are the
     538 plus the two comparator tests, which live outside the rung. `test_gpu_corpus` ran 0
     tests under the filter, as every cycle. The gpu lib builds with 0 warnings.
- **Rust-only, local** (the sf1 symlinks resolve): `cargo test --features rust-only -p
  peacockdb-core --lib -- --test-threads=2` → `test result: ok. 535 passed; 0 failed; 2 ignored;
  0 measured; 0 filtered out; finished in 89.92s` (533 + 2); `--test test_module_layout` →
  `test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in
  0.13s`. `tests::compare` alone: 11 passed, the two `should_panic` among them. The gpu lib
  built with no warning.
- **`build-test.md`**: `--lib` 535 → 537 and cpu 1006 → 1008; harness helpers 14 → 16; the
  operator harness 330 → 331, `-- gpu_tests::` 385 → 386, gpu 393 → 394; Rust 1492 → 1495,
  grand total 1928 → 1931. The known C++ 67-against-66 drift is master's and untouched.
- **Files touched**: `peacockdb-core/src/tests/compare.rs`, `tests/gpu_tests/aggregate_cases.rs`,
  `tests/gpu_tests/aggregate_dimension_cases.rs`, `tests/gpu_tests/join_dimension_cases.rs`;
  `llm-wiki/build-test.md`, `tickets.md` (#59's pin list), `tasks/join-cases-detail.md` and
  `tasks/aggregate-cases-detail.md` (the registers), the impl plan's ticks, this file. All four
  Rust files were rustfmt-clean before and are after (`rustfmt --edition 2024` on the leaves).

## Coordinator — 2026-09-16, to reviewing

The `compare.rs` move stays: the spec's verification bar asks for the comparator's red under
`rust-only`, and `gpu_tests/` cannot give one. It is the one file outside the spec's scope
table, named here so the reviewer and the analyst read it as a decision rather than a
surprise. Delivered as one commit on PR #157 against `ENS-aggregate-cases`. What the reviewer
reads: the comparator and its two `should_panic` tests, the composite-key pin, the fixture and
the one re-pin (#216's keyless merge, now asserting the counts' stddev computed from its own
arrivals), the registers and the counts (386 on the rung).

## Review round 1 — 2026-09-16

0 blocking, 0 important, 2 nits, both comment-only and applied by the coordinator: the
`welford_partial` doc claimed the cpu's init emits a null mean for a null row (it writes `0.0`;
`VarianceGroupsAccumulator::state`), so the doc now states the fixture's purpose instead; and
`compare.rs`'s module doc gained the clause naming `same_within_welford` as its one inexact
comparison. The reviewer simulated the composite-key pin's rows in Python (device drops build
rows 4, 9, 14 — `key=0`, `i32=NULL`; both engines' counts red under a fix either way), confirmed
the two `should_panic` tests are red on the old comparator (`did not panic` / arrow's
`primitive array`), reproduced #216's `sqrt(3.75/63)` from 60 ones and 4 zeros, and recounted
every `build-test.md` figure from the tree. `architecture.md`: no sentence falsified. Named and
left: `build-test.md`'s C++ 67 against rows summing to 66 is master's, on the helper's list.
To `completing`.

## Completeness pass, analyst — 2026-09-16

0 blocking, 0 important. Read as `git diff ENS-aggregate-cases...ENS-case-expectations`, four
commits, PR #157 based on `ENS-aggregate-cases` (4 commits, confirmed).

- **Finding 1** delivered whole: `same_within_welford` (`tests/compare.rs:123`) asserts
  `names_and_types` over both batches before the exact projection. Two callers in the tree,
  `welford_answered` (`aggregate_cases.rs:157`, the two #163 pins) and `finalized_within_welford`
  (`aggregate_dimension_cases.rs:470`, the stddev and var finalizes); no other. `welford_answered`
  borrows the device's names and casts the count, by its own documented design, so the check
  binds on the two finalize cases the spec named.
- **Finding 2** delivered as the spec wrote it — the eight-row probe, cpu 30 and device 27, the
  device's 27 pinned as the cpu's 30 minus three null-`i32` rows — and listed beside the nine in
  #59 and in join-cases' register. It is not the nine pins' oracle shape
  (`device_answers_as_if_null_equals_null`): `hash_join_keyed` carries no flag. The count form
  still goes red under a fix in either direction, which is what retires a pin.
- **Finding 3** delivered: count is `is_valid`, mean stays null, m2 0. The fixture's inheritors
  in the tree are six, not the spec's five: the two grouped finalizes; `bug_a_welford_merge_
  exports_its_count_as_int64` (#163) directly; and through `keyless_welford_partial` the three
  #216 pins — the keyless merge (re-pinned on the counts' stddev, reason above), the global
  stddev finalize refusal and the keyless var merge refusal. All six ran in the 328 `_cases`
  cycle; #216's ticket text still describes the re-pin's mechanism, and the register row carries
  both values.
- **Verification bar** holds as written; every `build-test.md` figure recounted from the tree
  (321 `operator_case!` + 10 bare tests = 331; 11 + 5 = 16; +3 overall).
- **`architecture.md`: no sentence falsified.** The branch is test-only, and the three claims it
  touches — the hardcoded `EQUAL` on anti (#59, "What the finish pass computes" and "Join types
  and NULL key equality"), the keyless Welford merge reducing the count column (#216, "The
  aggregate sequence" and "From node to seqs"), the Int64 count export (#163) — are each
  confirmed by a pin rather than contradicted.
- **`build-test.md`, operator harness and harness helpers: no sentence falsified.** The helpers
  row's new clause is true and its count right. The harness row's "compared slot by slot and
  exactly — schema included" was already loose for the Welford path before this branch; the
  branch made its schema half true there and disclosed the one inexact comparison two rows up.
- For the signoff: two files outside the Scope table, `tests/compare.rs` (the accepted move) and
  `aggregate_dimension_cases.rs` (the sanctioned re-pin and its imports); no shortcut, no bandaid.

## Completeness pass — 2026-09-16, the two lists compared

Reviewer (what is wrong): 0 blocking, 0 important — the two `should_panic` tests red on the old
comparator, the composite pin's three drops replicated in Python, the re-pin's shape assert
before its value, every count recounted. Analyst (what is missing): 0 blocking, 0 important —
six inheritors of the fixture, not five, all in the `_cases` cycle; the composite pin as wide as
the spec, by count rather than the single-key pins' oracle shape. `architecture.md`: no sentence
falsified, by both readings; `build-test.md`'s harness rows true. The signoff appended to the
spec. To `completeness approved`; `done` waits on the CI run for `eb808fd4`.
