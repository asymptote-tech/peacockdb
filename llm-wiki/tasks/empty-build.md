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

**But it must not stop dropping unconditionally.** Measured off the goldens: **389,331** dropped
empty batches across 692 emitter sites, of which only **244** sit on a permanently-empty lane.
Keeping them all would be roughly a million extra FFI calls to deliver 244 useful ones, since every
consumer but the forwarder makes a backend call per batch. So the drop stays as the default and is
lifted only where a join above owes rows — **the guard is the task**, not the keeping.

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

### 1. The `false` branch that was never written

The decision point already exists, in the right place, with the right information. The driver marks
the lane, the join reads its own type, and `empty_build_answers_nothing` already answers:

```rust
// driver/single_partition.rs:183 — the driver routes an empty build lane
ExecutorCategory::Join if self.awaits_build() && !avail.has[BUILD_SLOT] => LaneCall::NoBuild,

// gpu_backend/join.rs:103 — and the join asks the type what it owes
let owes_nothing = self.join_type.map_or(true, empty_build_answers_nothing);
if owes_nothing { return Ok(()); }
Err(BackendError::new("... takes a call over a build table that does not exist (#175)"))
```

**Nothing new marks anything.** `empty_build_answers_nothing` already makes the right decision in the
right place; what is missing is only what to do when it answers `false`. An earlier draft of this
spec proposed teaching a generic accumulator the join type through a plan-time or execution-time
flag. That was solving a problem that does not exist, because the decision never belonged at the
coalesce.

What `without_build` lacks is a zero-row build **table** to call with.

### 2. The table it lacks already exists, and the driver drops it

`driver/partitioned.rs:381`:

```rust
for (lane, out) in outputs.into_iter().enumerate() {
    // Empty scatter outputs are dropped here, so nothing empty traverses a chain
    // because of hash skew.
    if out.num_rows() == 0 { continue; }
```

That `out` is a fully typed zero-row table — `RecordBatch::new_empty(schema)` on the CPU
(`cpu_backend/emit.rs:73`), and on the device a deep-copied `cudf::slice(pv, {start, start})` carrying
column names (`node_session.cpp:405`). It is dropped, and one lane later the join is told its build
side is empty and refuses.

**Keep it where the join above owes rows, and nowhere else.** Not every empty lane: the spike priced
that at 389,331 dropped batches to deliver the 244 that matter — roughly a million extra FFI calls,
since every consumer but the forwarder makes a backend call per batch. The guard is
`empty_build_answers_nothing` on the consuming join's type, the same predicate `without_build` reads,
so the two cannot drift.

**The open question, and it is answered before any code is written:** does the driver, at the
scatter, know which consumer each lane feeds? It is the same `Driver` that later decides `NoBuild`,
and `self.index` holds the tree, so the fact is reachable in principle — establish how. If the
scatter cannot cheaply know its consumer, the fallback is to keep only what a join consumes and say
so in the report. **It is not to invent a marker.**

### 3. What the C++ then already computes

With a zero-row build batch the lane takes `SetBuild` rather than `NoBuild`, and
`operators/join.cpp:299` computes the answer that was missing: `Right` is `left_join(probe, build)`
with `left_policy = NULLIFY`, which **is** the build-side pad, and `RightAnti` is `left_anti_join`
over empty keys, which returns every probe row.

So the parked spec's entire §2 — a new `ProjectRole`, a drain-walk routing change, a `without_build`
signature change — buys something the C++ already does. None of it is in scope.

### 4. The comments that lie, and the ticket that is wrong

`cpu_backend/accumulate.rs:130` justifies the CPU emitting nothing *because* "the device's collapse of
no handles is a refusal (#173)" — a false premise, so the two engines agree today for a reason that is
not true. `gpu_backend/accumulate.rs:222` says "the device refuses that (#173)" where the code returns
`Ok(Vec::new())`. Both are fixed here.

Correct #173's text to the one site that refuses, and #175's corpus reach: the registry names `tpcds
q77` and `tpch q16`, not `q21` — `q21` is enabled at all five CPU modes and carries no #175.

### 5. The hazard, named rather than discovered

An empty probe batch is an extra probe call, and `gpu_backend/join.rs:303` refuses a second one — that
is [#152](../tickets.md#t152). This keeps batches on the **build** side, so it should not reach it.
Check rather than assume: a lane that gains a build batch must not gain a probe call.

### 6. Tests

- **an empty build lane reaches `SetBuild`, not `NoBuild`** — the driver-level claim, asserted where
  the routing happens rather than through the answer it eventually produces.
- **the six types that owe nothing still drop their empty lanes** — the guard from the other side.
  Without it the change is "keep every empty lane", which is the 389,331-batch version.
- **`Right` pads from the build side and `RightAnti` returns every probe row** — end to end, so "the
  C++ already computes this" is proved rather than argued.
- **a build batch does not become a probe call** — the #152 hazard.
- **`tpcds q77` and `tpch q16` run** — the two queries the registry says are waiting, on the CPU.

## Scope of code changes

| file | change |
|---|---|
| `executor/driver/partitioned.rs:381` | the drop becomes conditional: a zero-row scatter output is kept where the consuming join's type owes rows |
| `executor/gpu_backend/join.rs:103` | `without_build`'s `false` branch stops erroring — or becomes unreachable, if step 2 routes the lane to `SetBuild` before it is called. **Which of the two is the design, and this spec does not choose**: routing earlier is cleaner and may be impossible if the driver cannot know the consumer at the scatter |
| `executor/cpu_backend/join.rs:185` | the CPU counterpart of whichever shape step 2 takes |
| `executor/cpu_backend/accumulate.rs:130` | a doc comment resting on a false premise |
| `executor/gpu_backend/accumulate.rs:222` | the other lying comment |
| `llm-wiki/tickets.md` | #173 cut to its one real site; #175's corpus reach corrected |
| `llm-wiki/architecture.md` | `:606` and `:856` describe the old behaviour |
| goldens | 6 `.cpu.txt`, 6 `.cost.txt`, `cost-registry.csv` |

**Not touched:** `operators/join.cpp`, which already computes both answers; `gpu_backend/join.rs`'s
`finish_without_keys`, which is #173's surviving site and lives on the **probe** side; the ABI; the
wire; and `empty_build_answers_nothing` itself, which is read and never changed.

**No new marker of any kind** — not a node field, not a recipe field, not a plan-text attribute. If
the work appears to need one, the reading above is wrong and that is a finding to report rather than a
scope increase.

## Restriction

**The drop stays the default.** `driver/partitioned.rs:381` keeps dropping empty scatter outputs
everywhere except where the consuming join's type owes rows. An implementation that removes the drop
and relies on something downstream to absorb the cost is the 389,331-batch version, and it is the one
failure this task can produce that a green test suite would not catch.

**No new ABI symbol, no `ProjectRole`, no `without_build` signature change, no wire change, and no
new marker.** If the work appears to need one, the reading in §1 and §2 is wrong, and that is a
finding to report rather than a scope increase.

Code changes are limited to the conditional drop, whichever of the two shapes §2 settles on, the two
doc comments, and the tests.

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
