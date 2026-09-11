# Walk drives every plan implementation plan

**Goal:** Teach the recipe walk the call shapes it panics on, turn a device refusal into a recorded
outcome, record the engine's real limits as `bug_` tests, and leave behind a checked statement of
what the walk can and cannot drive.

**Architecture:** Ten refusal sites, split by a discriminator the walk's own messages supply. Harness
gaps get taught, one at a time, each with a query that reaches it. Engine limits become `bug_` tests
naming their ticket. Nothing in production changes.

**Tech stack:** Rust. Device runs on `shad-gpu`.

**Spec:** [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — frozen.

## Global constraints

- **No production code.** Everything is the walk, the session helper it drives through, and
  `build-test.md`. If driving a shape needs a production change, **stop and report**: it means the
  driver and the walk disagree about what the engine does.
- **No schemas.** No field on `Call`, no assertion about types. That is `declared-schemas`.
- **No engine limit is lifted.** A `bug_` test records a refusal; it never removes one.
- `driven()` stays a match, never a list — a kind added must stop it compiling.
- Commit messages at most 10 lines.

---

### Task 0: Decide whether this task should happen at all

The spec makes this conditional and the decision is a deliverable, not a formality.

**Files:**
- Modify: `llm-wiki/tasks/walk-drives-every-plan-detail.md`

- [ ] **Step 1: Read what the survey found**

```bash
sed -n '1,120p' llm-wiki/reports/sink-divergence.md
```

If the report does not exist, `sink-divergence-survey.md` has not run. **Stop and say so.**

- [ ] **Step 2: Write one paragraph and hand it back**

Which of the eight harness-gap shapes are worth teaching, given what the survey measured at the sink.
If every divergence class already surfaces there, the `bug_` tests in Task 3 are worth more than the
teaching, and this task shrinks to Tasks 2 and 3.

**The human decides on that paragraph.** Write it to the detail file and stop. Do not begin Task 1
on your own judgement — the whole point of the conditionality is that the evidence arrives after the
spec was written.

---

### Task 1: Build the refusal table by grep, not by reading

**Files:**
- Modify: `llm-wiki/tasks/walk-drives-every-plan-detail.md`

- [ ] **Step 1: Enumerate every site**

```bash
grep -n 'panic!\|assert!\|assert_eq!\|unreachable!' <the walk file>
```

46 sites at the time the spec was written, ten of them refusals. **The file has moved under
`test-layout.md`; find it rather than using the spec's path.**

- [ ] **Step 2: Classify each by its own message**

A message ending *"and no shape here plans one"* or *"none is planned"* is a **harness gap** — the
queries in this file do not produce that shape. A message citing a ticket is an **engine limit**.

Where a message says neither, classify it yourself and **write the reasoning down**. Those are the
ones the spec's first draft got wrong, and a site classified without an argument is a site that will
be reclassified by the next reader.

- [ ] **Step 3: Locate the `rc == 0` site**

The spec names `Session::execute:154` from a review, and it is **not in the walk file**. Find it:

```bash
grep -rn 'rc == 0\|assert_eq!(rc' peacockdb-core/src peacockdb-core/tests --include=*.rs
```

If it does not exist, say so — the spec's §"A refusal treated as a crash" would then be describing
something that has already changed.

- [ ] **Step 4: Commit the table to the detail file**

No code yet. This table is what the rest of the task executes against.

---

### Task 2: A refusal is recorded, not fatal

Do this before teaching any shape: several shapes cannot be driven until a refusal stops aborting.

**Files:**
- Modify: the session helper found in Task 1 step 3

- [ ] **Step 1: Write the failing test**

```rust
/// A plan the device declines aborts the walk today, so no test can say anything about
/// a refusal beyond that it happened. Four known refusals are unreachable for that
/// reason: #45, #189, #95 and #55.
#[test]
fn a_refused_plan_reports_its_code_and_the_call_that_produced_it() {
    // Drive one of the four known refusals and assert the error names both the rc and
    // the call. Proved on something that actually refuses, not on a fixture: a fixture
    // would pass against a mechanism that never sees a real non-zero rc.
}
```

- [ ] **Step 2: Watch it fail, then return the code instead of asserting on it**

`execute` returns the code and the call rather than asserting `rc == 0`. Callers that want the old
behaviour say so at their own site.

- [ ] **Step 3: Run green, commit**

```bash
git commit -m "a device refusal is an outcome, not an abort

execute asserted rc == 0, so a plan the device declines took the walk with
it and four known refusals were unreachable to any test wanting to observe
them. It returns the code and the call now."
```

---

### Task 3: The engine limits, as `bug_` tests

**Files:**
- Modify: the walk file; `llm-wiki/build-test.md`

- [ ] **Step 1: Write one test per engine limit**

Three from the spec's table, confirmed against Task 1's classification.

```rust
/// A streamed probe of more than one batch consumes the build handle and there is no ABI
/// symbol to copy it, so the second call has nothing to join against.
/// [#152](../../../../llm-wiki/tickets.md#t152) — delete this with the fix.
#[test]
fn bug_a_join_refuses_a_second_probe_batch() { … }

/// A finish pass accumulates probe keys across batches, which the recipe cannot express.
/// #136 — delete this with the fix.
#[test]
fn bug_a_join_refuses_a_finish_pass_that_accumulates_keys() { … }

/// Left and Full joins, cross and nested-loop joins and the pad projections are refused
/// by driven(), because the device cannot run them.
/// [#175](../../../../llm-wiki/tickets.md#t175),
/// [#152](../../../../llm-wiki/tickets.md#t152) — delete each with its fix.
#[test]
fn bug_the_walk_refuses_the_join_shapes_the_device_cannot_run() { … }
```

Each asserts the refusal **and names the shape**, so it fails informatively when the engine gains the
capability rather than just going red.

- [ ] **Step 2: Add them to `build-test.md`'s Known-wrong behaviour table**

`declared-schemas.md` created that table; this adds rows. If it does not exist, that task has not
landed — create it with the same shape and say so, rather than inventing a second home.

- [ ] **Step 3: Commit**

```bash
git commit -m "the engine's real limits are bug_ tests, not assertions

Three refusals were invariants buried in a harness, which is building
around a bug. As bug_ tests they are greppable, and each dies with its own
fix: refcounted-tables deletes two by closing #152."
```

---

### Task 4: The shapes, one at a time

Only those Task 0's paragraph kept. One commit and one device cycle per shape, or per pair where the
spec says they share one.

**Files:**
- Modify: the walk file

- [ ] **Step 1: The pair that unlocks the join family**

`Input::AccumulatedKeys` at the resolver, and the `AtDone` join call at the phase assertion. Neither
is useful alone: keys with no call to consume them, or a call with no keys to resolve.

Test first, naming a query that produces the shape. **A shape taught with no query reaching it is not
taught** — assert that the query drove the new path, not merely that nothing panicked.

- [ ] **Step 2: The independent ones**

`Call::bare`, the row range at the export, a scan or sink recipe of more than one call, and the
repartition arm. Each is its own commit with its own test.

`:309` and `:466` — "a scan's recipe is one call per batch", "a sink's recipe is one call per handle"
— stop being true once `declared-schemas` declares per call. Check which of the two landed first and
say so in the commit.

- [ ] **Step 3: The multi-batch probe, last and most carefully**

`probe_lane.len() == 1` is the one whose failure is silent: a dropped batch looks like a smaller
answer rather than an error. Assert the per-call outputs are still threaded correctly, by row count
and by content, not just that the drive completed.

**This one overlaps an engine limit.** #152 says the call consumes the build handle, so a second
probe batch may be refused rather than driven. If so it belongs in Task 3, not here — move it and say
why.

---

### Task 5: The statement of reach

The deliverable neither earlier spec produced.

**Files:**
- Modify: `llm-wiki/build-test.md`; the walk's existing coverage test

- [ ] **Step 1: Extend the existing check rather than adding a register**

`the_kinds_a_device_has_run_against_the_kinds_the_file_claims` already checks a version of this in
both directions. Extend it to call shapes. **One artifact, checked** — a second register drifts
against the first, and a prose table drifts against both.

- [ ] **Step 2: Write the table**

Every node kind and call shape against one of three verdicts: drivable; refused by the device, with a
ticket; not yet taught, with why. It goes in `build-test.md` beside the walk's row, because it is what
a later reader needs before planning anything that rests on the walk.

- [ ] **Step 3: Run the full suite, commit**

---

## Self-review against the spec

- **Conditionality** — Task 0, which stops rather than deciding for the human.
- **§ the refusal table** — Task 1, built by grep, with the classification argument written down for
  any site whose message does not classify itself.
- **§ harness gaps** — Task 4, one at a time, each with a query.
- **§ engine limits as `bug_` tests** — Task 3, including the `build-test.md` rows.
- **§ a refusal is recorded** — Task 2, deliberately before Task 4 because shapes cannot be driven
  while a refusal aborts.
- **§ the statement of reach** — Task 5.
- **Restriction: no production code** — no task touches `src/` outside the test tree; Task 4 step 3
  names the case where that would be needed and routes it to Task 3 instead.
- **Not covered, deliberately:** the spec's `driven()`-stays-a-match rule is a global constraint
  rather than a step, because it applies to every edit in Task 4 and a step would let the other edits
  off.
