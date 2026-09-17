# Case expectations implementation plan

**Goal:** Three expectations tightened on chain E's branch: the Welford comparator checks names
and types, the composite-key #59 form is pinned, the Welford partial fixture counts a null as 0.

**Architecture:** Edits to two helpers and one new `bug_` case, in the files the branch already
carries. A case that turns red under a tightened expectation is pinned with a ticket, never
loosened back.

**Tech stack:** Rust at the gpu rung; one `shad-gpu` cycle.

**Spec:** [`case-expectations.md`](case-expectations.md) — frozen.

## Global constraints

- Cases only; no production code, no new mechanism. `WELFORD_RELATIVE` does not move.
- Every divergence gets a ticket before it gets a `bug_` test.
- Commits at most 10 lines; `rustfmt`; device cycle foreground.

---

### Task 1: `same_within_welford` checks the whole batch's names and types

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs:156-213`

- [x] **Step 1: The failing test**, rust-only, in the same file's test module (drift: `gpu_tests/` does not compile rust-only, so the comparator moved to `tests/compare.rs` and the two tests sit beside `assert_same`'s own — see the detail file): call
  `same_within_welford` with an expected batch and a "device" batch whose column 1 is renamed
  (`stddev_x` vs `stddev_y`) and with values within tolerance; `#[should_panic(expected =
  "stddev_x")]`. A second with column 1 retyped `Float32`, `#[should_panic(expected =
  "Float64")]`. Both pass today because nothing panics — that is the red.
- [x] **Step 2:** At the top of `same_within_welford`, before the exact projection:
  `crate::tests::compare::names_and_types(&expected.schema(), &gpu.schema())` asserted equal
  (reuse the helper `compare.rs:106` already has; make it `pub(super)` if it is not). Then
  the existing exact/approximate split unchanged.
- [x] **Step 3:** Both new tests green (`tests::compare`, 11 passed; the `aggregate_cases` filter runs 0 tests rust-only, as it must); `cargo test --features rust-only -p peacockdb-core
  --lib -- tests::gpu_tests::aggregate_cases` compiles (the device cases do not run rust-only).
- [ ] **Step 4: Commit** (the coordinator's). `git commit -m "same_within_welford: names and types exact, values within tolerance on the finalize alone"`.

### Task 2: The composite-key #59 pin

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/join_dimension_cases.rs` (beside the anti-join cases, `:492-518`)

- [x] **Step 1:** Restore the eight-row probe as a builder local to the case — the shape
  `join-cases-detail.md:33-39` describes: `keyed(8, 3, Key::Composite)` whose row 4 has a null
  `i32` in the second key column — and write:

```rust
// #59 — the hash join compares keys with a hardcoded EQUAL, so a null in the second column of
// a composite key matches on the device where SQL says it cannot: the anti join keeps 27 rows
// where the cpu keeps 30.
operator_case! {
    GpuHashJoin,
    fn bug_a_left_anti_join_on_a_composite_key_matches_a_null_in_the_second_column_on_the_device() {
        let node = hash_join_keyed(JoinType::LeftAnti, Key::Composite, None);
        let out = run_both(&node, eight_row_anti_script());
        // the cpu answers 30 rows; the device 27 — assert both, so a change either way is seen
        assert_eq!(out.cpu_rows(), 30);
        assert_eq!(out.gpu_rows(), 27);
    }
}
```

  Use `Outcome`'s existing accessors for the two sides' row counts (`script.rs`); if it has
  none, `gpu_refuses`-style read of the slots through `assert_same` against a hand-written
  27-row expectation is the alternative — the count is the contract.
- [x] **Step 2:** Device cycle (`_cases`, with Task 3) `PCK_TEST_FILTER='join_dimension_cases'`: the pin green (it
  asserts the divergence), everything else untouched. Add the case to #59's pin list in
  `tickets.md` and to the `bug_` table.
- [ ] **Step 3: Commit** (the coordinator's). `git commit -m "#59 pinned on a composite key: a null in the second column matches on the device"`.

### Task 3: `welford_partial` counts a null as 0

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/aggregate_cases.rs:683-694`

- [x] **Step 1:** `ones` becomes a `UInt64Array` (or `Int64Array`, whichever `welford_state()`
  declares on this branch) built from `s.column(4)`'s validity: `1` where the `f64` is valid,
  `0` where null; the mean column stays `s.column(4)` (null where null), m2 stays 0. Doc
  comment: "what an init over one row emits — a null row counts nothing."
- [x] **Step 2:** Device cycle (`_cases`, with Task 2; the keyless merge pin re-pinned on the counts' stddev, no ticket needed — see the detail file) `PCK_TEST_FILTER='tests::gpu_tests::aggregate'`: the two
  finalize cases (`aggregate_dimension_cases.rs:567-581`) and the three merge pins that use
  the fixture. Each green: record it. Any red: the phantom-zero agreement was hiding a real
  difference — file a ticket naming which backend mishandles a null mean in the merge
  (`VarianceAccumulator::merge_batch` reading `means.value(i)` unguarded, or cuDF's
  `merge_m2`), rewrite the case as `bug_`, and say so in the detail file. Do not restore the
  fixture.
- [ ] **Step 3: Commit** (the coordinator's). `git commit -m "welford_partial: a null value counts nothing, as an init would emit"`.

### Task 4: The record

- [x] `build-test.md`: counts (the `bug_` registers are the two detail files'), the `bug_` table (+1, or more if Task 3 pinned). Detail files:
  the analyst's three findings and what each became. `git commit -m "case-expectations: the record"`.
