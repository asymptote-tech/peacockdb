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

## Why this is its own task

Three tasks were planned against a single sighting at the sink and each was planned wrong. The reason
they could only see the sink is this harness: everything above it panics or asserts. A catalog of
per-call schemas, a check that a join's pad has the right width, a measurement of what an aggregate's
state actually looks like — none of them are reachable, and each earlier task worked around that by
predicting instead of measuring.

So the harness is the shared blocker, and it is bought once.

## What it refuses today, and which kind each is

**Harness gaps — shapes it was never taught.** These are not engine limitations; the device runs them
under the real driver every day.

| site | what it does | what it should do |
|---|---|---|
| `resolve:220` | panics on `Input::AccumulatedKeys` | resolve it like any other input |
| `make:289` | panics on a `Call::bare` | drive a call with no seq |
| `resolve` | panics on `Input::RowRange` | supply the range the call's pattern implies |
| `Walk::join:422` | asserts **every** join call is `PerProbeBatch` | drive `AtDone` join calls — the finish pass |
| `Walk::join:440` | asserts `probe_lane.len() == 1` | drive a probe of several batches, which is what any tp4 join has |
| `Walk::unload:460` | asserts `row_interval().is_none()` | drive an export with a row range |

**A refusal treated as a crash.** `Session::execute:154` asserts `rc == 0`, so a plan the device
declines aborts the walk. Several known refusals — a cast to a non-fixed-width target, a group key
the shuffle hasher will not take — are then unreachable to any test that wants to *observe* them.
Record the code and which call returned it, and let the walk continue or stop deliberately. A
refusal is an outcome, not an accident.

**Genuine engine limitations, which stay refusals.** `driven()` declines Left and Full joins, cross
and nested-loop joins, and the pad projections, because [#175](../tickets.md#t175) and
[#152](../tickets.md#t152) mean the device cannot run them yet. **Do not teach the harness to drive
those.** They are refused for a reason that lives outside this file, and a harness that pretends
otherwise turns a known engine gap into a confusing test failure. What this task owes them is that
the refusal says *which ticket*, so the next reader does not re-derive it.

Telling the two apart is the judgement this task exists to make, and the deliverable below is where
the judgement is written down.

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

- **each newly taught shape, driven** — one test per row of the first table, each naming a query that
  produces that shape. A shape taught with no query reaching it is not taught.
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
— that is the next task, and bundling them is how a harness change and a measurement change become
one diff nobody can review.

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
