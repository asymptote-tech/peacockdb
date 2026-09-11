# The walk drives every plan, and records what it cannot

Kind: production

`test_gpu_recipe_walk.rs` drives a recipe plan by hand — `begin_plan`, the calls each recipe names,
handles threaded between them. It is the only place in the tree that executes a plan the way the
driver does without being the driver, which makes it the only instrument that can ask what a single
call produced.

It cannot drive most of the engine. Not because the device refuses, but because **the harness panics
on call shapes it was never taught**, and because it asserts as invariants things that were merely
true of the queries it had when it was written. Those two are different and the difference is the
whole task: one is a gap to fill, the other is a claim to demote.

This task teaches the harness the shapes it panics on, turns a device refusal into a recorded outcome
instead of an abort, and leaves behind a statement of what it can and cannot drive. It **declares no
schemas and measures no types** — that is [`declared-schemas-derived.md`](declared-schemas-derived.md),
which cannot be written against a harness that refuses its plans.

## This task is conditional

It sits behind [`sink-divergence-survey.md`](sink-divergence-survey.md) and
[`declared-schemas.md`](declared-schemas.md), and **the survey may shrink it to nothing.** If every
divergence class the corpus produces already surfaces at the sink, a harness that can drive
intermediate calls buys little, and the `bug_` tests above are worth more than the teaching.

So the first thing this task does is read `reports/sink-divergence.md` and say, in one paragraph,
which of the shapes below are worth teaching given what the survey found. **The human decides whether
it proceeds**, on that paragraph. A task that re-argues its own justification after the evidence
arrives is cheaper than one that assumes it.

## Why this is its own task

Three tasks were planned against a single sighting at the sink and each was planned wrong. The reason
they could only see the sink is this harness: everything above it panics or asserts. A catalog of
per-call schemas, a check that a join's pad has the right width, a measurement of what an aggregate's
state actually looks like — none of them are reachable, and each earlier task worked around that by
predicting instead of measuring.

So the harness is the shared blocker, and it is bought once.

## What it refuses today, and which kind each is

**The list is a grep, not a reading.** Build it with

```bash
grep -n 'panic!\|assert!\|assert_eq!\|unreachable!' <the walk>
```

— 46 sites today, ten of which are refusals rather than ordinary assertions — and classify every one.
A table assembled by reading is how the first draft of this spec listed six of ten.

**The walk's own messages classify themselves**, which is the discriminator to use. A message ending
*"and no shape here plans one"* or *"none is planned"* says the queries in this file do not produce
that shape: a **harness gap**. A message citing a ticket says the engine cannot do it: an **engine
limit**.

### Harness gaps — shapes it was never taught

The device runs these under the real driver every day.

| site | what it refuses | what it should do |
|---|---|---|
| `:221` | `Input::AccumulatedKeys`, `RowGroups`, `RowRange` — *"not a handle the walk holds"* | resolve each like any other input |
| `:230` | more than one handle where it expects one | take the set the call names |
| `:290` | a `Call::bare` — *"takes runtime bounds rather than a seq, and no shape here plans one"* | drive a call with no seq |
| `:309` | a scan's recipe of more than one call | drive it; `declared-schemas` declares per call, so this stops being true |
| `:379` | a recipe with nothing at `AtDone` — *"a streaming limit is the shape that reaches here, and none is planned"* | drive a streaming limit |
| `:466` | a sink's recipe of more than one call | same reason as `:309` |
| `:460` | an export with a row range — *"no shape here plans one"* | supply the range the pattern implies |
| `:350` | a repartition where another kind appears | say which kind, and drive it |

### Engine limits — refusals that are real, and get a `bug_` test

These are not harness gaps. The production code cannot drive the recipe, and the walk is telling the
truth about the engine.

| site | what it refuses | ticket |
|---|---|---|
| `:422` | a join whose calls are not all `PerProbeBatch` — *"a finish pass accumulates probe keys across batches"* | #136 |
| `:441` | a probe of more than one batch — *"the call consumes the build handle with no ABI symbol to copy it"* | [#152](../tickets.md#t152) |
| `driven()` | Left and Full joins, cross and nested-loop joins, the pad projections | [#175](../tickets.md#t175), [#152](../tickets.md#t152) |

**Do not teach the harness to drive these.** But do not leave them as a bare `assert!` either:
**each becomes a `bug_` test**, asserting that the engine refuses this shape, with its ticket named in
a comment above it. `coding-style.md`'s rule applies exactly — the refusal is known-wrong production
behaviour, and an assertion buried in a harness is building around it. A `bug_` test makes it
greppable, makes "which shapes can this engine not run" a search rather than a memory, and goes red
the day `refcounted-tables` closes #152 — which is the signal to delete it.

```rust
/// A streamed probe of more than one batch consumes the build handle and there is no ABI
/// symbol to copy it, so the second call has nothing to join against.
/// [#152](../../../../llm-wiki/tickets.md#t152) — delete this with the fix.
#[test]
fn bug_a_join_refuses_a_second_probe_batch() { … }
```

**A refusal treated as a crash.** `Session::execute` asserts `rc == 0`, so a plan the device declines
aborts the walk rather than reporting. Several known refusals — a cast to a non-fixed-width target, a
group key the shuffle hasher will not take — are unreachable to any test that wants to *observe* them.
Return the code and the call that produced it. **Locate this site first**: it was named from a review
and is not in the walk file itself.

Telling the two kinds apart is the judgement this task exists to make, and the reach table below is
where the judgement is written down.

## The work

### 1. The shapes, one at a time

Each row of the first table above, taught and then exercised by a query that reaches it. Order them
so each lands with its own test rather than as one change — `AccumulatedKeys` and the `AtDone` join
call are the pair that unlock the join family, and the row range and bare call are independent.

`driven()` stays a match rather than becoming a list. Its own comment is the rule this task must not
break: a kind added stops it compiling, which is why nothing has ever silently fallen out of it.

### 2. A refusal is recorded, not fatal

`Session::execute` returns the code and the call that produced it. The walk decides what to do with
it; a test asserting a refusal asserts the code and the call, and a test not expecting one fails
with both in the message.

This is what makes the four known refusals observable — [#45](../tickets.md#t45),
[#189](active-tickets.md#t189), [#95](../tickets.md#t95) and [#55](../tickets.md#t55) all abort the
walk today, so no test can say anything about them beyond that they happened.

### 3. The statement of reach

The deliverable, and the thing neither earlier spec produced: **a table of every node kind and call
shape, and whether the walk can drive it** — drivable, refused by the device with a ticket, or not
yet taught. It lives in `build-test.md` beside the walk's row, because it is what a later reader
needs before planning anything that rests on the walk.

`the_kinds_a_device_has_run_against_the_kinds_the_file_claims` already checks a version of this in
both directions. Extend it rather than adding a second register: one artifact, checked, not a prose
table that drifts.

### 4. Tests

- **each newly taught shape, driven** — one test per row of the harness-gap table, each naming a query
  that produces that shape. A shape taught with no query reaching it is not taught.
- **each engine limit, as a `bug_` test** — one per row of the second table, asserting the refusal and
  naming its ticket. These are the tests that outlive this task: they are the record of what the
  engine cannot run, and each dies with its own fix rather than all at once.
- **a refused plan reports its code and its call** — asserted on one of the four known refusals, so
  the mechanism is proved by something that actually refuses rather than by a fixture.
- **the reach table matches the kinds the walk drives**, both directions, which is the existing
  test's shape extended to call shapes.
- **a join at several probe batches** — the assertion at `:440` demoted, with the multi-batch probe
  driven and its per-call outputs still threaded correctly. This is the one most likely to be wrong
  in a way tests do not catch, because a dropped batch looks like a smaller answer rather than an
  error.

## Restriction

**No schemas.** This task declares nothing, adds no field to `Call`, and asserts nothing about types
— that is `declared-schemas`, and bundling them is how a harness change and a measurement change
become one diff nobody can review.

**No engine limit is lifted.** A `bug_` test records a refusal; it never removes one. If teaching a
shape turns out to need the engine to gain a capability, that is a different task — and it is
probably [`refcounted-tables.md`](refcounted-tables.md), which closes #152 and would delete two of
these tests on its own.

**No production code.** Everything here is the walk, `Session`, and the reach table. If driving a
shape requires a production change, that is the finding: stop and report it, because it means the
driver and the walk disagree about what the engine does, which is a ticket and possibly a bug.

Code changes are limited to `test_gpu_recipe_walk.rs` and whatever it moves to `wire/gpu_tests/`
under `test-layout.md`, the session helper it drives through, and `build-test.md`.

## Goldens

None move. The walk asserts; it writes no golden. If a newly driven shape turns out to change a
recorded batch count somewhere, that is a finding to report rather than a golden to accept.

## Coverage

**No cells.** It enables no query and re-tickets none. What it buys is that the next three tasks can
be verified at all — which is the thing the last three did not have.

## Device workflow

`build-test-shadgpu.sh`. Each taught shape needs a device cycle to prove, so order the work by what
can share one: the join family together, the row range and bare call together.
