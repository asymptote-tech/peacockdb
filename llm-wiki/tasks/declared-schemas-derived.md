# The calls that emit state, keys and pads

Kind: production

> **Not finalized — do not dispatch.** A review of this draft against the code found it wrong in
> several load-bearing places: nine arms remain rather than ten (`sort` is already task 1's), the
> pads carry a count and not a shape so their declaration would have to be guessed, five of its
> tickets are aimed at things a declared-versus-device catalog cannot see, and most of its queries
> are undrivable by the walk as it stands — which is why
> [`walk-drives-every-plan.md`](walk-drives-every-plan.md) now sits between this and task 1. It is
> committed as a record of the scope, not as a spec, and is rewritten before it reaches the board.

The second half of the catalog. [`declared-schemas.md`](declared-schemas.md) declared the calls whose
schema was already in hand and left every other call at `None`, skipped by one guard. This task
derives the rest and **deletes that guard**, so that after it no call in the engine produces a table
nothing has a claim about.

Same discipline throughout: it **fixes nothing**. Every disagreement between a declaration and what
the device exported becomes a `bug_` test asserting the observed value, with its ticket named in a
comment, one test per (divergence class, node kind). Same instruments too: the thin test exporter, so
the answer is cuDF's rather than our exporter's defaults; one named test per query rather than a
generic assert-every-call loop; the mode chosen per query and stated with it; and the tests in
`wire/gpu_tests/`, in-crate, sharing the walk's harness as `pub(crate)`.

## What is left

Ten of `attach_recipes`' sixteen arms (`wire/attach.rs:76`). Task 1 took six — scan, filter, project,
sort, coalesce-all, unload. These remain:

| arm | why its declaration is not simply the node's schema |
|---|---|
| `aggregate` (`:163`) | the accumulate call emits **state**, the finalize emits values |
| `aggregate_batches` (`:245`) | two `PerCompaction` calls emit state, a distinct `AtDone` finalize emits the node's rows |
| `accumulate_and_sort` (`:200`) | compaction emits runs, done emits the sorted table |
| `hash_join` (`join.rs:29`) | up to six calls: a probe-keys project, the join per probe batch, two `AtDone` finishes, a `NullPad` project and a `Narrow` project — six shapes, one node |
| `nested_loop_join` (`join.rs:343`) | one call, but its output is the node's rows only when a projection is not dropped ([#190](active-tickets.md#t190)) |
| `cross_join` (`:324`) | the product's shape |
| `merge_sorted_partitions` (`:286`) | input schema, across lanes |
| `emit_partitions` (`:305`) | the scatter: one table per lane, each the input's schema |
| `limit` (`:341`) | input schema, but only the straddling batches are called |

Task 1 left one of its own six here too: `sort`. Every `ORDER BY` in the corpus puts a `GpuSort`
under a `GpuAccumulateBatchesAndSort`, so the sort arm is declared there but no query reaches it
until this task's accumulator queries do.

`MergePartitions`, `Union` and `Interleave` return `Ok(None)` — they attach no recipe and make no
call, so there is nothing to declare and nothing to skip. Say so in the code rather than letting them
fall through the same arm as an undeclared call, or "structural, no calls" and "nobody got to it yet"
become the same thing again.

## Where each declaration comes from

Most of it exists and has simply never been written down.

- **Aggregate state**: `GpuAggregate::intermediate()` and `GpuAggregateBatches::intermediate()`
  (`plan/mod.rs:627`, `:654`) already return the state `Schema`, and `aggregate_writer::aggregate`
  already receives it. Nobody has ever compared it to what the device produces. Three tickets suspect
  it is wrong — [#163](../tickets.md#t163) signed-versus-unsigned count state,
  [#94](../tickets.md#t94) the Welford count child's width, [#55](../tickets.md#t55) a partial-phase
  operand cast absent from the final phase's input schema — which makes this the single most valuable
  measurement in either task.
- **Probe keys**: the join's key schema exists in `wire/join.rs` and is what the `ProjectRole::
  ProbeKeys` call emits.
- **The pads**: `NullPad { nulls }` and `Narrow` carry their own widths, so their output shape is a
  function of data already on the call.
- **Routing arms** — `emit_partitions`, `merge_sorted_partitions`, `limit`: the input schema,
  unchanged. Cheap, and they are in this task only because they are not in the other one.

Nothing is invented. Where a shape genuinely cannot be derived from what the recipe writer holds,
that is a finding: record it, leave the call at `None`, file a ticket naming what the writer would
need. **Do not guess a declaration.** A guessed declaration that the device contradicts produces a
`bug_` test against our own invention, which is worse than no test — it pins a fiction.

## Multi-lane plans

Task 1 planned everything at `tp1-single`. That cannot reach this task's work: `emit_partitions` is
the scatter and `merge_sorted_partitions` merges across lanes, so **neither node exists in a
single-partition plan**. This task plans at a `tp4` mode as well, and the difference is not
incidental — three of the classes below only appear above a shuffle.

The lane dimension adds an assertion task 1 had no use for: **the lanes of one call agree with each
other.** A scatter emitting four tables whose schemas differ is a bug nothing else would see, and
[#122](../tickets.md#t122) is exactly that shape suspected for decimal scale across an aggregate's
partials.

## The queries

Chosen from the tickets that name a divergence this task can reach. As in task 1 they extend the
shared list in `src/test_support/` rather than replacing it, and the existing entries keep their
assertions.

| query | what it pins |
|---|---|
| `stddev(v)` / `var(v)` over a numeric column | [#94](../tickets.md#t94) — the Welford state's count child, typed differently by different cuDF runtimes |
| a grouped `count(*)` | [#163](../tickets.md#t163) — the count state declared `UInt64` and produced `Int64` |
| `sum(<decimal> / <int>)` with a `GROUP BY` | [#55](../tickets.md#t55) — the partial phase's operand cast missing from the final phase's input schema |
| `sum(l_extendedprice * l_discount)` at a `tp4` mode | [#122](../tickets.md#t122) — decimal **scale** agreement between the partials of one aggregate |
| a shuffled `count(*)` with a `GROUP BY` | [#180](active-tickets.md#t180) — nullability introduced by a state merge, same width and same bytes |
| `GROUP BY ROLLUP(...)` above a join at `tp4` | [#189](active-tickets.md#t189) — a `UInt8` grouping-set id in the key set; with [#95](../tickets.md#t95), a group-key type the shuffle hasher refuses |
| a non-`SELECT *` nested-loop join | [#190](active-tickets.md#t190) — a dropped projection: the node declares two fields and three arrive. The ticket's uncaught half is a projection that **reorders** or drops one while keeping the count |
| a join whose key needs a string cast | [#45](../tickets.md#t45) — a cast target that is not fixed-width, refused at the join |
| a `Right`, `Full` or `LeftAnti` join | the `NullPad` and `Narrow` projects, whose shapes nothing has ever checked |
| `min`/`max` over a string or a date column | [#195](../tickets.md#t195) — a string reduce in the merge, with zero corpus uses today |
| a wide `SELECT DISTINCT` | [#195](../tickets.md#t195) — the other shape with no corpus query at all |

## What task 1 assumed, and what happens if it was wrong


This spec is written before task 1's measurements exist, so it rests on four of its assumptions.

**If any of them is refuted, this task goes to `blocked(<its state>)` and stops.** Not adapted, not
worked around, not narrowed to whatever still fits. The coordinator writes the block, names which
assumption failed and what measurement refuted it in `<task>-detail.md`, and waits for the human —
who rewrites this spec, because only the human writes a spec. A spec amended mid-flight to survive
its own falsified premise is exactly how the three tasks before this one went wrong: each discovered
that its premise was false, built around the discovery, and shipped something nobody had specified.

The block is cheap and the alternative is not. Every one of these four is knowable from task 1's
results before a line of this task is written, so the usual case is that the block never happens —
and when it does, it happens at the start rather than three days in.

1. **One schema per call.** `Call::output_schema` is a single `Option<Schema>`, so a call whose
   firings disagree has no place to say so. Nothing known contradicts it, and task 1's query 13 —
   several batches into a coalesce at `tp1-rowgroup` — is what tests it. If it fails, the unit is
   (call, firing role) and both specs change. This task is where it is most likely to fail:
   `PerCompaction` compacts here and never does in task 1.
2. **A handle can be exported mid-plan and survive it.** The whole harness rests on the export being
   readable without consuming what it read. This task's calls produce **state** handles, which are
   the likeliest to refuse — cuDF groupby state is not obviously an Arrow table at all. A state
   handle that cannot be exported is a capability gap: a ticket and one ignored test, never a `bug_`
   test, because a `bug_` test has to pass today. If most of them refuse, this task's product is that
   finding plus the declarations, and the comparison waits for a mechanism that can read them.
3. **The thin exporter reaches state handles too.** It was built in task 1 against tables of ordinary
   columns. Passing declared precision and nullability through for a state row — a Welford triple, a
   partial sum beside its count — is not the same problem, and if it cannot, this task measures
   through the production exporter and says so at every affected assertion.
4. **The catalog's shape holds at scale.** Task 1 checks six node kinds at two modes; this adds ten
   kinds across lanes. If the per-(class, node kind) rule still produces an unreadable number of
   tests, the rule is wrong and this task says so rather than thinning the coverage to fit it.

## API changes

**None**, and for the same reason as task 1: everything is in-crate. `Call::output_schema` is
`pub(crate)` and was added there; the renderer, the harness and the shared query list all exist and
are `pub(crate)` within `wire/gpu_tests/` and `plan_text`. `test_support` stays untouched, as its own
rule requires. The only deletion is task 1's `None` guard, which is why that guard was written as one
place rather than scattered through the cases.

## Restriction

**This task fixes nothing.** No cast, no refusal, no correction to a declaration, no change to an
aggregate's state layout or a join's projection. Code changes are limited to setting
`output_schema` on the ten arms above, the queries, and the tests. No wire change, no C++, no ABI
symbol, no refactor of `attach.rs` or `join.rs` while passing through. Anything else found on the way
is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `recipe-payloads.txt` section B | **grows** | ten more arms declare, so more nodes print a schema |
| `*.plans.txt` at the `tp4` modes | **no change** | task 1's `Exporter` schema already landed there; this task adds no node-line field |
| `recipe-payloads.txt` section A, every `sha256=` | **no change** | the wire still does not change |
| `*.plans.txt` | **no change** | the declaration is on the call, not the node line |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | nothing executes differently |
| `testdata/cost-registry.csv` | **no change** | no cell is enabled or re-ticketed by a catalog |

## Coverage

**This task claims no cells**, for the same reason as task 1: the registry scores refusals and this
removes none. It is judged on what it writes down — a declaration for every remaining call, checked
on a device at one and four lanes, and a named test for each disagreement.

What it is worth is the input it gives the tasks that follow. Three of them are waiting on it.

## Device workflow

`build-test-shadgpu.sh`, in batches of about five queries as T19 does, at `tp1-single` and one `tp4`
mode. Section B's golden is CPU-generated and lands first, so the device run checks a declaration
that is already reviewable. Expect the divergences the eleven tickets predict; record whatever else
turns up, which is the point.
