# The build side that produced nothing still answers

Kind: production

**This task closes [#175](../tickets.md#t175)** — an empty build side leaves `Right`, `Full` and
`RightAnti` owing rows they cannot make — by emitting a zero-row batch where the build side today
emits nothing at all.

It replaces the parked `empty-answers.md`, which bundled this with [#173](../tickets.md#t173) and
proposed building a capability the engine turns out not to need. A spike measured both; what follows
is what the measurement says.

## What the spike found

**The lane's table already exists and is thrown away.** Both engines build a fully typed zero-row
table for an empty lane — `cpu_backend/emit.rs:73` via `RecordBatch::new_empty(schema)`, and the
device via a deep-copied `cudf::slice(pv, {start, start})` carrying column names
(`node_session.cpp:405`). The driver then drops it at the scatter.

**But the scatter is the wrong place to stop dropping.** Measured off the goldens: **389,331**
dropped empty batches across 692 emitter sites, of which only **244** sit on a permanently-empty
lane. Keeping them all would be roughly a million extra FFI calls to deliver 244 useful ones, since
every consumer but the forwarder makes a backend call per batch. **The scatter is out of scope and
stays as it is.**

**`Right`, `Full` and `RightAnti` need no new mechanism.** Given a zero-row build batch,
`avail.has[BUILD_SLOT]` is true, so the lane takes `SetBuild` rather than `NoBuild` — and
`operators/join.cpp:299` already computes the answer. `Right` is `left_join(probe, build)` with
`left_policy = NULLIFY`, which *is* the build-side pad. `RightAnti` is `left_anti_join` over empty
keys, which returns every probe row. The parked spec's entire §2 — a new `ProjectRole`, a drain-walk
routing change, a `without_build` signature change — buys something the C++ already does.

**[#173](../tickets.md#t173) is smaller than its text.** Of its four named sites only one refuses:
`gpu_backend/join.rs:209`. `accumulate.rs:247` and `:387` return `Ok(empty)` under doc comments
claiming the device refuses, and the C++ throw at `node_session.cpp:278` is unreachable because the
Rust guards short-circuit first. It blocks **zero** cells in `cost-registry.csv`.

## The work

### 1. The build side emits a zero-row batch

Where the build's `GpuCoalesceAllBatches` emits nothing for a lane that received no rows, emit the
zero-row batch instead, **guarded on `!empty_build_answers_nothing(join_type)`** so the six join
types that owe nothing keep draining for free and pay nothing for this.

That guard is what keeps the cost at 244 batches rather than 389,331. `empty_build_answers_nothing`
already exists and already makes the right decision in the right place; what was missing was what to
do when it answers false.

Both engines move identically — there is one driver (`executor/mod.rs:630`) — and the CPU
counterpart is `cpu_backend/accumulate.rs:138`.

### 2. The comments that lie, and the ticket that is wrong

`accumulate.rs:223` and `:325` say the device refuses where the code returns `Ok(empty)`. Fix them in
this change; a doc comment asserting a refusal that does not happen is why #173's text describes four
sites when it has one.

Correct #173's text to what is true, and correct #175's corpus reach: the registry names `tpcds q77`
and `tpch q16`, not `q21` — `q21` is enabled at all five CPU modes and carries no #175 at all. The
wiki's account of both is stale and is fixed here rather than filed.

### 3. The hazard, named rather than discovered

An empty probe batch is an extra probe call, and `gpu_backend/join.rs:303` refuses a second one —
that is [#152](../tickets.md#t152). This task adds batches on the **build** side, not the probe side,
so it should not reach it. Check it rather than assume it: a lane that gains a build batch must not
gain a probe call.

### 4. Tests

- **an empty build lane answers with a zero-row batch** — for a join type that owes rows, and the
  batch carries the declared column types rather than an empty shape. CPU first, since the CPU is
  where the behaviour is cheap to assert.
- **the six types that owe nothing still emit nothing** — the guard, asserted from the other side.
  Without this the change is "every empty lane emits", which is the 389,331-batch version.
- **`Right` pads from the build side and `RightAnti` returns every probe row** — the two answers the
  C++ already computes, asserted end to end so that the claim "no new mechanism is needed" is proved
  rather than argued.
- **a build batch does not become a probe call** — the #152 hazard above.
- **`tpcds q77` and `tpch q16` run** — the two queries the registry says are waiting, on the CPU.

## Scope of code changes

Every file this touches, and the one decision the spec does not make for you.

| file | change |
|---|---|
| `executor/cpu_backend/accumulate.rs` | `one_batch` (`:138`) returns `Ok(Vec::new())` for an empty lane. It emits a zero-row batch of the declared schema instead, when the lane must answer. Its doc comment (`:130`–`:137`) rests on a false premise and is rewritten |
| `executor/gpu_backend/accumulate.rs` | the same decision on the device side; `SortedRuns`' doc (`:222`) says "the device refuses that (#173)" while the code returns `Ok(Vec::new())` — one of the two lying comments |
| `plan/join.rs:452` | `empty_build_answers_nothing` is read, not changed. It already decides correctly; what was missing is a caller for the `false` branch |
| `llm-wiki/tickets.md` | #173's text cut to the one site that refuses; #175's corpus reach corrected to `tpcds q77` and `tpch q16` |
| `llm-wiki/architecture.md` | `:606` and `:856` describe the old behaviour and stop being true |
| goldens | 6 `.cpu.txt`, 6 `.cost.txt`, `cost-registry.csv` |

**The open decision: how the accumulator learns it must answer.** `one_batch` and `SortedRuns` are
generic accumulators — they do not know a join sits above them, and `empty_build_answers_nothing`
takes a `JoinType` they have no access to. Two shapes, and the author picks one with reasons:

- **Plan time.** The planner builds the join and its build-side child together, so it can mark that
  child "answers even when empty". A field on the node, and the accumulator reads it. Costs a node
  field and possibly a recipe field; the decision is visible in the plan text, which is where a
  reader would look for it.
- **Execution time.** `executors_for` builds the join executor knowing its children, so the backend
  can hand the flag down when it constructs the accumulator. Costs no plan change and renders
  nowhere, so a wrong answer has no artifact to have been caught by.

**Estimate the first before choosing the second.** A flag that renders in `.plans.txt` is a flag a
reviewer can see, and this task's whole premise came from reading goldens. If plan time turns out to
need a wire field, say so and stop — that is a larger task than this one and it should not grow into
one quietly.

**Not touched:** `driver/partitioned.rs` and the scatter; `operators/join.cpp`, which already computes
both answers; `gpu_backend/join.rs`'s `finish_without_keys`, which is #173's surviving site and lives
on the probe side; the ABI; the wire.

## Restriction

**The scatter is not touched.** `driver/partitioned.rs`'s drop of empty scatter outputs stays; the
spike priced keeping it at ~10⁶ extra calls for 244 useful ones.

**No new ABI symbol, no `ProjectRole`, no `without_build` change, no wire change.** If the work seems
to need one, the spike's reading was wrong and that is a finding to report, not a scope increase.

Code changes are limited to the build-side emit and its guard, the two doc comments, and the tests.

## Sequencing

After [`declared-schemas.md`](declared-schemas.md). Not a build dependency — this needs nothing that
task produces — but that task's zero-row query is what establishes that a zero-row table crossing the
boundary really does carry full type information, which is the premise this one rests on. Landing it
the other way round means asserting the premise here and measuring it later.

It does **not** depend on `wire-schema.md`. The parked spec's claim that `output_schema` on the wire
was the enabling change turns out to be about #173, which blocks nothing.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `<mode>-<tier>.cpu.txt` | **yes**, 6 files | 244 batches appear where none were counted |
| `<mode>-<tier>.cost.txt` | **yes**, 6 files | derived from the `.cpu.txt` sections |
| `*.plans.txt`, `recipe-payloads.txt` | **no change** | plan time is unaffected; nothing about the recipe moves |
| `--- memory ---` sections | **no change** | plan-time accounting |
| `<tier>.result.txt` | **no change** | digest-sorted, so batch ordering does not reach it |
| `testdata/cost-registry.csv` | **yes** | `tpcds q77` and `tpch q16` move off #175 |

## Coverage

Two queries carry #175, and the honest expectation is that they go green on the CPU and their device
cells land on [#152](../tickets.md#t152) rather than going green — the causes are ordered. Close #175
when its cells are gone from the registry, not when the code lands.

## Device workflow

`build-test-shadgpu.sh`. Five things the run must prove, in the order they are likely to fail:

1. `filtered_join` can be **constructed** from a zero-row build — the likeliest failure;
2. `left_join` and `left_anti_join` over a zero-row build;
3. `concatenate` and `merge` with zero-row members;
4. `to_arrow_host` on a zero-row string column;
5. `q77` and `q16` green on the CPU, with their device cells landing on #152.

## Adjacent, filed rather than fixed

[#199](../tickets.md#t199) — a global aggregate on an empty lane emits nothing on the device and its
identity row on the CPU, because `gpu_backend/accumulate.rs:307` lacks the CPU's `!self.grouped`
clause. A wrong answer, found by reading rather than by a run, and deliberately **not** fixed here:
it is a different bug on the same page, and the parked spec's habit of bundling the two is what this
task exists to undo.
