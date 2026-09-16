# Archived task specs

Specs for tasks whose PR has merged, newest first, and for tasks dropped with their approach
rejected, marked as such at the top of the entry. Each is the contract the work was done
against, kept verbatim -- including the amendments and corrections made mid-task, since those
are the part a later reader cannot reconstruct from the diff.

The batch-partitioned rollout's task list is the first entry below, archived whole on
2026-09-08: T0 through T19 and T21 are done, T20 is now [#195](../tickets.md#t195) and T22 is
obsolete. The design it was the plan for was folded into
[`architecture.md`](../architecture.md) the same day, which is where the shapes these tasks
built are described; this list is what a number in a commit message resolves to.

The entry after it merged 2026-08-20 as PR #126, opened against ENS-bp-plan-skeleton and
retargeted to master when that base merged.



---

<!-- archived from llm-wiki/tasks/declared-schemas.md -->

**Obsolete — approach rejected, never merged; PR #151 closed 2026-09-16.** A per-call catalogue of declared-versus-exported types measures a divergence whose classes the sink survey already named and whose fixes sit elsewhere: `Utf8View` leaves every plan with `schema_force_view_types` off, decimal precision is a label the export is told (`column_metadata`), and the rest — `extract(year)`'s Int16, `avg`'s count state — are producer bugs fixed at the producer (#191, #163). Its branch `ENS-declared-schemas` stays until `walk-drives-every-plan`, which is built on it, is re-based or dropped. Its derived draft follows as the next entry.

# The schema the device is not held to

Kind: production

The engine declares a schema for every node and **only one of its two backends is held to it.**

On the CPU side `declared_as` (`executor/cpu_backend/mod.rs:239`) runs at every stage and makes the
declaration a fixed point. It absorbs exactly two things and refuses the rest: a decimal a merge
widened is cast back — unsafe cast on purpose, so a value that does not fit raises instead of
becoming a NULL indistinguishable from one the data had — and names and nullability are relabelled.
Anything else, including a column count that does not match, `RecordBatch::try_new` refuses. That is
an emulation doing its job, and it is why the CPU is right about nulls and about decimal precision:
not because DataFusion never diverges, but because something pulls it back to the declaration every
time.

It is private to `cpu_backend` and every call site is inside it (`mod.rs:188`, `accumulate.rs:431`,
`join.rs:272`), so it is not reachable from the device path even in principle.

The device side has no equivalent. It exports whatever cuDF produced, through an exporter that adds
rewrites of its own, and the first thing that compares the result against the declaration is
`concat_batches` at the sink — one table, at the end, after every intermediate divergence has either
cancelled out or been absorbed.

Two consequences of that asymmetry, both of which shape what this task may conclude. **The CPU
cannot adjudicate**: when declared and device disagree, "the CPU agrees with the declaration" is not
evidence, because it agrees by construction. And **nullability is the one relabel with nothing behind
it** — types and column counts are refused by `try_new`, but a column declared non-nullable whose data
carries nulls may simply be re-flagged. Confirm what this arrow version validates before query 8
rests on it.

This task measures the side that is not held. It declares what each **call** produces, exports each
call's output through the export that already exists, compares the two, and records every
disagreement as a `bug_` test. It **fixes nothing**.

It is the first of two. This one takes the six calls whose declaration is already in hand — scan,
filter, project, sort, coalesce-all, exporter. [`declared-schemas-derived.md`](declared-schemas-derived.md)
takes the other ten arms, whose calls emit state, keys and pads.

## Why this shape, and where it measures

An earlier attempt predicted the device's answer in Rust: a function modelling the Arrow → fb → cuDF
→ Arrow round trip, rendered into a golden, consulted at the sink. Wrong instrument twice. A model
can have an arm nobody gave behaviour to and nothing goes red — that is how a `Date64` prediction
shipped with neither a cast nor a refusal — and it duplicates C++ across the FFI with nothing
comparing the two.

But "measure, do not model" is not enough on its own, because **the engine normalizes the schema in
several places and each one hides a class of divergence.** Between cuDF's table and anything Rust can
read there are three:

- `export_table_to_ipc` widens `DECIMAL32`/`DECIMAL64` to `DECIMAL128`;
- `gpu_executor.cpp:49-90` builds `col_meta.push_back({name})` with **no precision**, so
  `to_arrow_schema` falls back to `metadata.precision.value_or(max_precision)` — **every decimal
  exports at 38, always**, whatever cuDF actually holds;
- nullability on the way out is **data-derived**: `child->flags = col.has_nulls() ? NULLABLE : 0`, so
  a batch that happens to contain no null exports as non-nullable regardless of what was declared.

So a measurement taken through the production exporter is a measurement of *our exporter*, not of
cuDF. Precision would read 38 for every decimal at every node, and nullability would read whatever
this batch's data happened to be. That is the finding that reframed this task, and the spec states it
rather than measuring through it.

**What is compared is therefore declared versus exported, and the exporter is a party that can be
wrong.** Two of the three rewrites — precision and nullability — are the exporter's own, so they are
named as limitations rather than reported as divergences (step 3). What remains unexplained is
cuDF's, and that is what the catalog records.

This is not the first per-node assertion in the tree. `executor/gpu_backend`'s device tests already
assert one node's declared output. This generalizes that to every call of every query in the list.

## The work

### 1. The boundary node declares, like every other

`NodeKind::Sink` becomes `NodeKind::Exporter { schema }`. Today a sink carries neither a layout nor a
schema, which makes the export the one call in the engine with nothing to compare against — the
exact boundary where every divergence has historically surfaced.

Eight references, four files: the variant and `layout()`/`schema()` (`plan/mod.rs:249`, `:257`,
`:264`), `GpuUnload::new` (`:970`), three `matches!` in `plan/validate.rs` (`:25`, `:48`, `:301`), one
arm in `executor/tests.rs:135`, one doc line in the rebuilder. `GpuUnload::new` holds its `input`, so
the schema is `input.kind().schema()` at the construction site. `layout()` stays `None`: a sink has
no partition layout and that has not changed.

The rename is part of it because "sink" means "consumes and produces nothing", which stops being true
the moment it declares an output. Nothing treats `schema() == None` as "this is the sink" — validation
matches the variant — so no hidden caller breaks. Eight `.expect("… is not a sink")` messages stay
true but become about a *child*; reword the ones this touches.

**Consequence:** `plan_text/node_text.rs:30` renders a schema wherever `kind().schema()` is `Some`, so
the `GpuUnload` line gains `schema=[…]` and all ten `.plans.txt` goldens move. That is the diff to
review rather than accept, and it is what `exports=` was reaching for — except as the plan's own
declaration rather than a prediction of what the device would do with it.

### 2. A schema on each call

`Call` (`wire/mod.rs`) gains `output_schema: Option<Schema>` — the schema of the table a firing of
that call produces. `None` means "not declared by this task", which is every call of the ten arms
task 2 owns.

The six arms here are the ones where nothing has to be derived: `scan`, `filter`, `project`, `sort`,
`coalesce_all_batches` and `unload` (`wire/attach.rs:100`, `:115`, `:129`, `:145`, `:228`, `:355`)
each produce their node's own schema, already in hand as `inputs[0]` or `node.kind().schema()` — and
`unload` joins them only because step 1 gave it one. **Nothing is derived and nothing is invented**,
which is what keeps every disagreement a disagreement between two facts that both predate this task.

`Schema` here is `plan::Schema`, which is `PartialEq` and not `Eq`, so `Call`'s derive list loses
`Eq`. Check what that breaks before assuming it is free; if something needs `Eq` on a `Call`, say so
in `-impl.md` and carry a key instead of the schema.

**One schema per call is an assumption, and step 5 is where it goes red.** A call fires many times —
`PerBatch` once per input batch, `PerCompaction` on every compaction and again at done. Read before
assuming: `aggregate_batches` (`attach.rs:245`) carries its intermediate-versus-final split on
*separate calls*, so nothing known contradicts it. If firings of one call disagree, the unit is
(call, firing role) and both specs change rather than being patched.

### 3. What the export can and cannot answer

The measurement uses the export that already exists. `peacock_result_from_handle` writes an IPC
stream and `StreamReader::try_new(...).schema()` reads what the device produced — no C++ change, no
new ABI symbol, no cuDF version question.

**Two dimensions it cannot answer, named here rather than measured badly.**

- **Precision.** cuDF's decimal carries a scale and no precision, and `column_metadata` has nowhere to
  put one before 26.02 — it is name and children only. So every decimal exports at 38 whatever the
  plan declared, and "what precision did the device produce" has no answer. The report records the
  class as *our exporter's default*, not as a cuDF divergence. Getting this wrong is what sent
  `wire-schema` to the wrong file.
- **Nullability.** The export derives the flag from `col.has_nulls()`, so it reports what this batch
  happened to contain rather than what was declared. A difference here is data, not a type error.

An earlier draft answered both with a second export path in C++ that cast to the declared type. It is
cut. On 25.02 it restated nothing, so a successful cast meant exported equalled declared by
construction — and the question it genuinely did ask, *does the data fit the declared precision*, is
a **value** check that does not belong in a schema catalog. `declared_as` already asks it on the CPU
side, and that is where the production fix will start.

**What is left is what the catalog is for**: the type each call declares against the type the device
handed back. That is where `Utf8View → Utf8`, an `extract`'s `Int32 → Int16`, a non-fixed-width cast
target and a `Date64` live — and none of them needs a C++ line.

### 4. A second section in `recipe-payloads.txt`

The golden gains a section carrying the declared schemas, per node and per call, **rendered from the
Rust `Call` data before `Writer` serializes anything** and labelled as such.

- **Section A and every `sha256=` line are untouched.** `Writer::push` keeps `output_schema: None`,
  the wire does not change, the C++ reads nothing new, and this task cannot break the executor.
  Putting schemas on the wire is a later task's work; when it lands, the two sections become each
  other's check.
- **Reuse `plan_text`'s `schema_text`/`type_text`** (`plan_text/node_text.rs:317`, `:329`), which
  render `Decimal128(15,2)`. **Not** `wire/fb_text.rs`'s `schema_text` (`:221`), which formats the fb
  enum with `{:?}` and prints a bare `Decimal128`, dropping `decimal_precision` and `decimal_scale`.
  The two have the same name in two modules and the wrong one loses the digits silently — which is
  how the wire renderer got that way. That drift in section A is out of scope and gets a ticket.

The golden is owned by `test_plan_goldens`, which `test-layout.md` moves to `planner/tests/` at the
rust rung. Regenerating needs `UPDATE_CANONICAL=1` **and** `PEACOCK_REWRITE_RECIPE_BYTES=1`.

### 5. One named test per query, on a device

**No generic assert-every-call loop.** Each query gets its own test asserting the schemas that query
produces — `bug_`-prefixed where the assertion records something wrong. A general "every firing
matches its declaration" assertion cannot be green beside named tests asserting the opposite for the
same firings, and every way out of that is either a whitelist or building around a bug. One test per
query has neither problem: the assertion and its exception are the same statement.

The tests live in `wire/gpu_tests/`, a new file beside the walk that `test-layout.md` moves there,
gated `#[cfg(all(test, feature = "gpu"))]`. In-crate because after `visibility.md` `wire` is
`pub(crate)` and an integration test is a separate crate that cannot name `Recipe` or `Call` at all.
The driving harness — `begin_plan`, the calls each recipe names, handles threaded between them — is
shared with the walk as `pub(crate)` within that module rather than copied.

Each test: drive its query's plan by hand; after each firing that produces a handle, export it
through the thin exporter, read the Arrow schema, and assert. Where a call's declaration is `None`,
skip it and **name task 2 in the skip**; that guard is deleted there, so it is written once rather
than scattered through the cases.

A handle that cannot be exported at all is not a schema divergence but a capability gap: a ticket and
one ignored test naming it, never a `bug_` test, because a `bug_` test has to pass today.

The harness must not assume a query has rows — today an `assert!(total_rows > 0)` would refuse query
6 outright.

### 6. The queries

**Check this list against [`reports/sink-divergence.md`](../reports/sink-divergence.md) before
writing any of it.** The survey runs immediately before this task and measures the whole corpus for
the cost of one error message; these thirteen were chosen from tickets and from reading the type
table, which is a weaker basis. Where the survey shows a class does not occur, drop its query and say
so. Where it shows a class nobody predicted, add one. A query kept only because it was written here
first is the habit this chain exists to break.

What the survey cannot tell you is where a divergence *entered* — it sees only the sink — so a class
it reports is still worth measuring per call here. What it can tell you is which classes are worth
the cost.

Added to the shared list rather than replacing it; the queries already there keep their own tests and
their own job of exercising node kinds.

**The mode is chosen per query and stated with it.** `tp1-single` is `OneBatchPerLane`, so it cannot
produce two firings of one call — fixing the mode was what left an assertion in the first draft with
no query that exercised it.

| # | query | mode | what it pins |
|---|---|---|---|
| 1 | `SELECT n_name FROM nation` | tp1-single | `Utf8View` declared; cuDF has no such type, so `Utf8` at every node. One test for the class, not one per node |
| 2 | `SELECT l_extendedprice FROM lineitem WHERE l_orderkey = 1` | tp1-single | narrow `Decimal128(15,2)` — [#187](active-tickets.md#t187). Recorded as our exporter's default, since cuDF holds no precision to have diverged |
| 3 | `SELECT CAST(l_extendedprice AS DECIMAL(38,4)) FROM lineitem WHERE l_orderkey = 1` | tp1-single | a decimal already at max precision, which agrees **for the wrong reason** — 38 is the default, not a measurement. The row exists so the report says so |
| 4 | `SELECT l_shipdate FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Date32` → `TIMESTAMP_DAYS` → back |
| 5 | `SELECT l_orderkey, l_linenumber FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Int64` and `Int32` identity |
| 6 | `SELECT n_name FROM nation WHERE n_nationkey < 0` | tp1-single | the **zero-row** batch, and whether an empty string column types as a populated one |
| 7 | `SELECT n_name AS label, n_nationkey AS id FROM nation` | tp1-single | column **names** survive the crossing. Declared-versus-device only: the CPU relabels names to the declaration, correctly, so it has no opinion to compare against |
| 8 | `SELECT CASE WHEN n_nationkey > 10 THEN n_name END FROM nation` | tp1-single | **nullability**, and what the export can say about it: the flag comes from `col.has_nulls()`, so this records the limitation rather than a divergence |
| 9 | `SELECT n_regionkey, n_nationkey FROM nation` | tp1-single | column **order and arity** — [#190](active-tickets.md#t190)'s uncaught half |
| 10 | `SELECT CAST(n_nationkey AS BIGINT), CAST(n_nationkey AS DOUBLE) FROM nation` | tp1-single | cast targets, fixed-width |
| 11 | `SELECT CAST(n_nationkey AS VARCHAR) FROM nation` | tp1-single | [#45](../tickets.md#t45) — a cast target that is not fixed-width |
| 12 | `SELECT extract(year FROM o_orderdate) FROM orders` | tp1-single | [#191](active-tickets.md#t191) — integer **narrowing**, `Int16` exported where `Int32` was declared |
| 13 | a query whose sink column is a `Date64` | tp1-single | [#200](../tickets.md#t200) — `Date64` returns `Timestamp(ms, None)`, **a type the wire cannot express**. Neither cast nor refused today |
| 14 | a query producing a `Timestamp` at the sink | tp1-single | [#200](../tickets.md#t200)'s other half: `convert_data_type` has no `Timestamp` arm, so the plan should be refused. Assert the refusal is clean and names the type rather than panicking — nobody has exercised it |
| 15 | a scan yielding several batches into a coalesce | tp1-rowgroup | **the firings of one call agree with each other.** The mode is the point: `rowgroup` is what produces more than one batch per lane. Confirm against that mode's plan text and name the query in `-impl.md` |

Two that are **not** walk queries, because they never reach a device:

- `SELECT l_shipdate + interval '1 day' FROM lineitem` — [#168](../tickets.md#t168): the wire has no
  interval, so `attach_recipes` returns `Err` and a walk driving it would panic rather than assert. It
  is a plan-time refusal test, in the same file, with no device.
- A sink column of `Binary`, `Null` or `Float16` — cuDF maps all five of that class to `EMPTY`. No
  corpus column declares one and nothing refuses them today, so the class stays untested by
  construction until a task refuses it at planning time. Named here rather than omitted.

**Named as out of reach for this task, not absent.** `UInt8` and `UInt64` do appear in plans — 55
times at `tp1-single`, as `$count:UInt64` and `__grouping_id:UInt8` — but only above aggregate and
rollup nodes, which are task 2's arms. `ORDER BY` at every mode puts `GpuSort` under a
`GpuAccumulateBatchesAndSort`, also task 2, which is why there is no sort query here despite `sort`
being one of the six. `Timestamp`, `Dictionary`, `Int8`, `Float32` and list-typed columns have no
corpus source at all and are named in the report as gaps, not silently skipped.

### 7. Divergences are catalogued, not adjudicated

A disagreement becomes a `bug_` test asserting the **observed** value, ticket named in a comment,
per `coding-style.md`'s "Building around a bug". It does not matter which side is wrong: this task
does not decide. The task that fixes it decides, with this catalog as its input.

**One test per (divergence class, node kind), not per node instance.** A cause that enters early
survives every node above it: `Utf8View` becomes `Utf8` at the scan and stays that way to the sink, so
a test per node would bury one bug under dozens. The class is what the test names.

**Never a shared table of expected divergences.** A table is a whitelist, and a whitelist is what let
a `Date64` arm ship with no behaviour at all.

### `bug_` tests get their own section in `build-test.md`

This is the first task in the chain that writes any, so it is the one that makes them findable.
`build-test.md`'s Test categories section gains a table of its own, below the categories table: one
row per `bug_` test — the test, what wrong behaviour it asserts, its ticket, and where it runs.

**It is counted separately and excluded from the grand total, and the page says why.** Every other
row counts coverage. A `bug_` test counts a defect, and adding it to a coverage number means the
number goes *up* when a bug is found — which is the reading the number exists to prevent. Its own
total, stated beside the grand total, with one sentence saying the two measure opposite things.

That table is also the answer to "which known-wrong behaviours does this engine still have", which is
a question nothing in the repo answers today. It shrinks as fixes land, and a row that cannot be
deleted when its ticket closes is a fix that did not do what it claimed.

A divergence with no ticket gets one filed. A ticket whose text the catalog contradicts — #187 is
framed as two engines disagreeing when it is a missing argument — gets corrected here, because this
is the first evidence anyone has had.

## API changes

**None.** Everything is in-crate. After `visibility.md`, `wire/mod.rs` and `plan_text/mod.rs` carry no
`pub` items at all, and this task adds none: `Call::output_schema` is `pub(crate)`, the section-B
renderer is `pub(crate)` in `plan_text`, and the tests are `#[cfg(all(test, feature = "gpu"))]`
modules inside `wire/`. `test_support` is untouched, which is what its rule requires — its `pub` API
may not name a type from `wire` or `plan_text`, and the harness signature is nothing but those.

No FFI signature changes. The thin exporter is a new test-only path, not a change to an existing one,
and it needs no new ABI symbol if it reuses the existing export entry point.

## Restriction

**This task fixes nothing.** No cast, no refusal, no correction to a declaration, no change to the
production exporter. Code changes are limited to steps 1 to 6. If a divergence looks like a one-line
fix, that is a ticket: a fix here has to be right about a device nobody has measured yet, and "it is
only one line" is what produces the cascade this task exists to avoid.

No refactor of `attach.rs` or the walk while passing through, no wire change, no C++ beyond the thin
exporter.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt`, all ten | **yes** — the `GpuUnload` line gains `schema=[…]` | step 1 gives the boundary node a schema, and plan text renders every schema it is given |
| `recipe-payloads.txt` section B | **new** | the declared schemas, rendered Rust-side |
| `recipe-payloads.txt` section A, every `sha256=` | **no change** | the wire does not change |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | nothing executes differently |
| `testdata/cost-registry.csv` | **no change** | no cell is enabled or re-ticketed by a catalog |

## Coverage

**This task claims no cells.** The registry scores refusals and this removes none. It is judged on
what it writes down: a declaration per call for six node kinds, measured on a device through an
exporter thin enough that the answer is cuDF's, and a named test for every disagreement.

Its value is to the tasks after it. Three were planned from a single sighting at the sink and each
was planned wrong: one predicted what it could have measured, one chose a C++ mechanism its cuDF
version does not have, one bundled a 35-line fix with a cross-cutting one. A catalog is what stops
the fourth.

## Device workflow

`build-test-shadgpu.sh`, in batches of about five queries as T19 does, at the modes the table names.
Section B's golden is CPU-generated and lands first, so the device run checks a declaration that is
already reviewable. Expect the divergences the tickets predict; record whatever else turns up, which
is the point.


---

<!-- archived from llm-wiki/tasks/declared-schemas-derived.md -->

**Obsolete — a draft derived from `declared-schemas`, never finalized, dropped with it 2026-09-16.**

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


---

<!-- archived from llm-wiki/tasks/sink-divergence-survey.md -->

**Done 2026-09-15 as a prototype — no PR, branch `ENS-sink-divergence-survey` at `cebe1ead` never merged; the product is [`reports/sink-divergence.md`](../reports/sink-divergence.md) and the sink's message, both on master.** Archived 2026-09-16; the chain it closed (`ENS-drop-mode-name`, later B) is gone from the board.

# What the corpus already knows about schema divergence

Kind: prototype

Sixty-odd device cells are disabled against schema causes, and every one of them fails at the sink.
The corpus runs those queries on a device already. They already fail. **The evidence exists and
almost nobody reads it.**

The message is better than it looks. `concat_batches` ends in `RecordBatch::try_new`, whose error is
*"column types must match schema types, expected {declared} but found {exported} at column index
{i}"* — so both types are already there, and the tickets quote them: #187 carries `expected
Decimal128(15, 2) but found Decimal128(38, 2)`, #191 carries `expected Int32 but found Int16 at
column index 0`.

Two things are missing, and they are small: the column **name**, and every column **after the
first** — `try_new` reports one mismatch and stops. A sink carrying a string and a narrow decimal is
two findings, and reporting one is how a rollout concludes that fixing the string was enough.

So this task is mostly the second half: **run the corpus and write down what comes out.** The code
change is a handful of lines. The product is a report.

**`llm-wiki/reports/sink-divergence.md`.** The branch is never merged.

## Why this before anything larger

[`declared-schemas.md`](declared-schemas.md) declares a schema per call and measures it on a device,
and it is the right shape — but it costs a harness, an exporter and a device cycle per query, and it
measures thirteen hand-picked queries. This measures **the whole corpus** for the cost of one error
message and the rollout that was going to happen anyway.

What it cannot see is anything above the sink: `concat_batches` is the only comparison on the device
path, and it happens once, at the end, on the concatenated result. So this survey answers *which
divergences reach the boundary and how often*, and says nothing about where they entered. That is the
question `declared-schemas.md` is for, and this is what tells it which classes are worth the harness.

## The work

### 1. The message names the column, and every column

`executor/gpu_backend/mod.rs:179` wraps `concat_batches`' error, which already carries both types.
Append what it lacks: the column **name**, and the columns after the first.

The decoded batches carry the device's schema — `StreamReader::try_new` parses the IPC schema message
before any batch — so comparing it against `self.schema` costs nothing and needs no prediction.

**Keep the existing sentence as the prefix.** Every ticket quotes it and every rollout greps for it;
this appends, it does not replace.

**Compare types only.** `try_new` does not check nullability, so a nullable-versus-non-nullable
difference never reaches the sink and never disabled a cell. Reporting it here would put a class in
the report that does not occur, which is worse than omitting it — the report's whole use is telling
the next task which classes are real.

### 2. Run the corpus and collect

`build-test-shadgpu.sh`, in batches of about five as T19 does, across the cells disabled against a
schema cause. The existing rollout protocol, unchanged — the only difference is that the failures are
now worth reading.

Collect verbatim. **Do not fix anything, and do not enable a cell.** A cell that now reports a
different cause than its ticket claims is a line in the report, not an edit to the registry.

### 3. The report

`llm-wiki/reports/sink-divergence.md`, and it answers four questions:

- **Which divergence classes actually reach the sink**, by (declared type → exported type), with a
  count of queries for each. This is the table the whole task exists for.
- **Which of them the tickets already name**, and which are new. #183 predicts `Utf8View → Utf8`;
  #187 predicts `Decimal128(p,s) → Decimal128(38,s)`; #191 predicts `Int32 → Int16`. A class nobody
  has filed is the finding worth having.
- **Which cells' failures disagree with the ticket they are disabled against.** The causes are
  ordered, so a cell disabled against one cause may now be reaching another; the registry says one
  thing and the device says another, and nobody has compared them.
- **What the sink cannot see.** Name the classes `declared-schemas.md` intends to catch that produced
  no evidence here, and say whether that is because they do not occur or because they cannot reach
  the boundary. An absent row and an untested row read the same otherwise.

## Restriction

**One site changes.** The error path at `executor/gpu_backend/mod.rs:179` and nothing else. No cast,
no refusal, no registry edit, no ticket closed, no golden regenerated.

**No fix, however obvious.** If the survey makes a one-line fix look irresistible, that is the
strongest possible argument for writing it down and doing it in a task that can be reviewed — the
three parked tasks are what happens when a measurement turns into a change mid-flight.

## Coverage

**No cells.** It enables none and re-tickets none. It is measured by whether the tasks after it are
planned against evidence instead of a sighting.

## What it feeds

- [`declared-schemas.md`](declared-schemas.md) — its thirteen queries were chosen from tickets and
  from reading the type table. This says which classes actually occur and how often, so the list can
  be cut to what matters or extended to what nobody predicted.
- [`walk-drives-every-plan.md`](walk-drives-every-plan.md) — the harness is only worth buying for
  divergences that do not surface at the sink. If everything surfaces here, that task shrinks.
- The rewrite of `casts` and `wire-schema`, both of which are waiting to know whether their ticket's
  account of itself is still true.


---

<!-- archived from llm-wiki/tasks/operator-cases.md -->

**Merged 2026-09-15 as PR #150, squashed to `12e2926a`.**

# Every seq-bearing operator through the harness

Kind: production

**Closes no ticket and fixes nothing.** Cases only: every one is green or a `bug_` test with a
ticket, and the production tree is not touched — not to make a case pass, not to close a ticket
a case happens to reach. The findings are the deliverable; a later task fixes what they name.

Ninth in the chain, after [`operator-harness.md`](operator-harness.md), which is the whole of
its mechanism. This task adds cases and nothing else: no helper, no production change. Each
row below is either a green comparison or a `bug_` test naming a ticket, and only running it
says which.

## Why two tasks

The harness is proven on operators that cannot hide a wrong helper. Once it holds, every
other operator is a script and a node, and a case that goes red is a fact about the engine
rather than about the harness. Keeping the cases out of the harness task also keeps that diff
reviewable: a reviewer reading the comparator should not be reading sixty cases beside it.

## The matrix

One synthetic batch or a few, a hand-built node over `Given` leaves, `run_both`, `assert_same`.
The right column names the tickets a row may land on; a row with none may still find one.

| Operator | Cases | May land on |
|---|---|---|
| `GpuFilter` | predicate on an int, on a string, on a null-yielding column; with its projection; every row passes; no row passes | |
| `GpuProject` | column copy; int, float and decimal arithmetic; a cast; CASE in both forms; LIKE; a scalar function; a typed NULL literal | [#57](../tickets.md#t57), [#198](../tickets.md#t198), [#187](active-tickets.md#t187) |
| `GpuSort` | asc and desc; nulls first and last; two keys; `fetch` | |
| `GpuAccumulateBatchesAndSort` | several batches; one; none; `fetch` | [#173](../tickets.md#t173) |
| `GpuMergeSortedPartitions` | N lanes each sorted; `fetch`; one lane empty; `Done` before any batch | [#173](../tickets.md#t173) |
| `GpuCoalesceAllBatches` | several batches; one; none | [#173](../tickets.md#t173) |
| `GpuAggregate` | each `PlanAgg`, grouped and global; the single-node shortcut carrying a finalize; grouping sets; a decimal sum's scale | [#187](active-tickets.md#t187) |
| `GpuAggregateBatches` | merge with and without its finalize; arrivals sized to cross the compaction threshold — 1 MiB on the cpu, 64 MiB on the device, so the device's is the one to size for; `merge_m2`; a count merging by sum; an average's digits | [#163](../tickets.md#t163), [#187](active-tickets.md#t187) |
| `GpuEmitPartitions` | 4 lanes and 64, each lane compared as a slot; null keys; two keys; a string key; a decimal key; one lane in and four out | [#184](active-tickets.md#t184), [#95](../tickets.md#t95), [#187](active-tickets.md#t187) |
| `GpuHashJoin` | each of the nine types × one probe batch and two × `null_equals_null` both ways × a residual filter where the matrix allows one; with a projection | [#152](../tickets.md#t152), [#159](../tickets.md#t159) |
| `GpuCrossJoin` | two batches; with a projection | |
| `GpuNestedLoopJoin` | Inner and Left with a predicate; with a projection | [#190](active-tickets.md#t190), [#160](../tickets.md#t160) |
| `GpuLoadParquet` | both backends read one parquet the test wrote from a synthetic batch: one batch per row group; a limit; row groups and a limit together | [#186](active-tickets.md#t186), [#188](active-tickets.md#t188) |

### Empty inputs

Every shape below is its own case, named for the shape, never folded into a loop over types:
a red one must say which combination reached the limit. The frozen surface cannot make a table
out of nothing ([#173](../tickets.md#t173)), an empty build side leaves three join types owing
rows ([#175](../tickets.md#t175)), and a global aggregate over nothing owes its identity row
([#199](../tickets.md#t199)) — so several of these are expected to land as `bug_` tests, and
which ones is the finding.

| Operator | Empty shapes |
|---|---|
| `GpuFilter`, `GpuProject`, `GpuSort` | a zero-row batch; a zero-row batch between two with rows |
| `GpuCoalesceAllBatches`, `GpuAccumulateBatchesAndSort` | no batch at all; one zero-row batch; a zero-row batch among others; a `fetch` over zero rows |
| `GpuMergeSortedPartitions` | every lane `Done` with nothing; one lane a zero-row batch beside lanes with rows; every lane a zero-row batch; lane 0 `Done` before lane 1's rows arrive |
| `GpuAggregate` | a zero-row batch, grouped; global (#199); grouping sets over zero rows |
| `GpuAggregateBatches` | no arrival; one zero-row arrival; a zero-row arrival among others; no arrival under a finalize |
| `GpuEmitPartitions` | a zero-row batch in — N zero-row lanes out, and the lane count is the assertion; a batch whose every row carries one key, so N−1 lanes get nothing; a batch of all-null keys; a stream of zero-row, rows, zero-row |
| `GpuHashJoin`, each of the nine types | a zero-row build batch with probe rows (#175 for Right, Full, RightAnti); build rows with one zero-row probe batch; both zero-row; `build: None`, never probed; a zero-row probe batch between two with rows; only zero-row probe batches then the finish — the finish whose probe produced no keys, #173's one refusing site |
| `GpuCrossJoin` | build empty; probe empty; both |
| `GpuNestedLoopJoin`, Inner and Left | build empty; probe empty |
| `GpuLoadParquet` | a parquet of zero rows |

The shapes the planner refuses are out of reach here too: the hash-join recipe arm asks the
node's capability and panics without one, so an outer join with a residual filter ([#153](../tickets.md#t153))
never reaches an executor and has no row.

The source row is the one that needs a file rather than an upload: a scan's input is a path.
The parquet writer is the test's, over `synthetic`, with the row-group size chosen so several
row groups exist.

Two of those tickets are closed by tasks below this one — [#198](../tickets.md#t198) by
`typed-nulls`, [#175](../tickets.md#t175) by `empty-build`. Their `bug_` tests here are the
record those tasks turn red and delete, which is the regression test each would otherwise
have to write.

## Scope

Code expected to change:

- `peacockdb-core/src/tests/gpu_tests/`: six case files — exec, aggregate, accumulate, emit,
  join, source, the aggregate split from exec because it carries its own state fixtures — and
  the kind registry entries. The source file carries the one helper this task adds, a parquet
  writer over `synthetic`, local to it.
- `llm-wiki/tickets.md`: a ticket per new defect; `llm-wiki/build-test.md`: the count.
- Nothing under `cpp/`, `peacockdb-ffi/`, or `peacockdb-core/src/` outside `tests/gpu_tests/`.

Component-level API expected to change: none. A case that would need a production change to
pass is a ticket and a `bug_` test, never the change.

## Constraints

Those of the harness task, and:

- No new mechanism beyond the parquet writer above. A case that needs a helper the harness
  lacks is a finding against the harness task, not a helper added here.
- Every row above exists as a case. The kind guard the harness carries extends to every
  `NodeRef` kind but the three forwarders, and a kind with no case is red.
- Every divergence gets a ticket before it gets a `bug_` test, and a `bug_` test asserts the
  wrong behaviour precisely — the wrong value, the refusal's message — not merely that the two
  sides differ. The ticket is where the fix is designed, later and by another task; nothing
  here repairs, works around, or casts away what a case finds. A ticket a case cannot reproduce
  stays open — it closes when its corpus cells run — and the detail file says which.
- The join scripts follow the capability matrix: build side one batch and always first, probe
  streamed, `without_build` where the build produced nothing.

## Verification bar

- Every row present; each case green or `bug_` with its ticket in the comment above it.
- The kind guard green with only the forwarders excluded.
- New tickets in `tickets.md`, within the fifteen-line cap, one per distinct defect.
- `build-test.md`'s row for the harness carries the new count; the grand total moves with it.
- Green on shad-gpu; the rust-only lib unchanged.

## Completeness signoff

Solved under its constraints: cases only, nothing outside `src/tests/gpu_tests/` and the wiki,
no mechanism beyond the parquet writer, no fix; every row of the matrix and the empty-input
table a case named for its shape; 204 operator cases, 128 green and 76 `bug_` asserting the
wrong answer or the refusal exactly, each with its ticket above it, going red on the fix; seven
new tickets and fifteen amended by what the device showed; the kind guard with only the three
forwarders excluded; exact comparison with no tolerance anywhere. Deviations, none a shortcut:
two fixture corrections (a padded side declared nullable, as the planner does; sorted runs
dealt round-robin so the merge key carries no ties); a seventh empty shape for the finishing
joins, the only route to #173; `nested_cases.rs` beside the spec's six files; cases beyond the
spec's rows; Welford's moments not compared, a mean not being dyadic — a harness finding; the
`avg` shortcut's count declared Int64 so its finalize can be the subject; eight `bug_` helpers
duplicated across files that belong to the harness; two handoffs written on #198 and #175 for
`typed-nulls` (whose premise the second pin shows false) and `empty-build` (whose driver fix
does not reach the harness's `without_build` route).

---

<!-- archived from llm-wiki/tasks/operator-harness.md -->

**Merged 2026-09-15 as PR #149, squashed to `3440aa05`.**

# One call sequence, two backends: the operator harness

Kind: production

**Closes no ticket and fixes nothing.** The task is a harness and the cases that prove it; what
it finds wrong it reports — a ticket and a `bug_` test — and leaves as it found it. Its two
production edits are mechanism the harness needs, not repairs, and neither changes what any
query answers.

Eighth in the chain, after [`sink-divergence-survey.md`](sink-divergence-survey.md) — a prototype
whose branch is never merged, so this one forks off `visibility`. It lands after
[`test-layout.md`](test-layout.md) on purpose: the harness is crate-level, needing both backends
and a device, and `src/tests/gpu_tests/` is the place that task creates for exactly that shape.
Writing it against `tests/*.rs` first would mean writing it twice.

## Why

Nothing today runs one operator on both backends over the same input and compares the two.
`executor_cases.inc` is eleven rows each side proves against a hand-written answer, and
`test_gpu_executors` asserts the device against answers the test wrote. A per-node divergence
therefore surfaces only at the root of a corpus query, where architecture.md's column-indexing
section says the per-node numbers cannot see it. The corpus is also numeric-aggregate heavy
([#195](../tickets.md#t195)), so most operator shapes have no query reaching them at all.

The harness makes the comparison one call: a hand-built node, a script of batches the test
wrote, both backends, the outputs compared exactly. This task builds it and proves it on the
operators whose recipe has no seq, so nothing in the FlatBuffer can hide a wrong helper.
[`operator-cases.md`](operator-cases.md) then runs everything else through it.

## What it is

**An upload.** `peacock_handle_from_arrow(executor, schema, array, out_handle)`: an Arrow C-data
import through `cudf::from_arrow` — the murmur3 hook at `gpu_executor.cpp:340` already does this
— adopted into the live session's registry as a handle, `NodeSession::adopt(TableResult)`. The
registry lives in the session, so `begin_plan` comes first; the seqless three load a plan of one
stub node, which is a shape the C++ already accepts under any forwarder's parent. The symbol is
test-only and says so where it counts: the header comment, no `AbiSymbol` names it, no recipe
can, and the extern sits in `peacockdb-ffi` beside `peacock_spark_partition_ids`, the test-only
hook already there — architecture.md's count of the ABI becomes seventeen, the conformance
group two. The `GpuBatch` a test wraps the handle in is the existing constructor. The fetch
side exists — `GpuExport` is what `GpuUnload` runs.

**A stub leaf, and the recipe code unchanged but for one arm.** `Given` — the leaf the cpu
backend's tests already have, declaring a schema and a layout and nothing else — moves up to
`src/tests/` and is shared. In `wire/attach.rs` a node outside the registry (`try_as_node_ref`
answers `None`, the case that function exists for) emits the writer's stub — the empty
`CudfScan` it already fills a forwarder's parent slot with, made reachable from `attach.rs` —
and no recipe. The stub is what keeps the plan rooted: the writer refuses a plan with no node,
and so does the C++, so a leaf that emitted nothing under an unload or a limit would leave
nothing to load. `attach_recipes(operator over Given leaves)` is then the operator's recipe, at
the last post-order, and its bytes are what `begin_plan` loads. No second recipe writer, no plan.

**A synthetic batch.** `synthetic(rows, seed)`: one fixed schema — Int32, Int64, Float64, Utf8,
Date32, Boolean, a key column with duplicates, a unique id — nulls in every column, floats
dyadic so sums compare exactly, deterministic from the seed. Zero rows is a legal argument and
a case in its own right. Decimals are a second fixture, `decimals(rows, seed)`, and not a column
of the first: the device exports every decimal at precision 38 whatever was declared
([#187](active-tickets.md#t187), open and owned by no task), so a decimal in every batch
would make every case that ticket's `bug_` test instead of the decimal cases alone.

**A comparator.** `assert_same(cpu, gpu, Order)`: slot by slot — one slot per call, and per
lane for the emitter, since output timing is a function of the call sequence on both backends
and a flattened multiset would pass a scatter that put a row in the wrong lane. A slot both
sides left empty — a call that produced no batch, which a limit outside its interval and a
one-call join's finish legitimately do — is equal; one side empty is a named difference, and it
is not the same thing as a zero-row batch. Within a slot, column names and data types, then
values, exact, after sorting by every column; `Order::AsEmitted` for the sorts, whose synthetic
keys carry no ties because neither engine's sort is stable. Nullability is not compared: the
device reports it from the data rather than the declaration, and the engine's own schema check
(`plan/validate.rs`) ignores it for the same reason. `CallStats` are not compared: the byte
formula is shared and scratch is measured. A type divergence fails — it is the
#183/#187/#191 shape, and a `bug_` test is where it belongs, not a cast in the comparator. The
comparator ships with its own red cases: a differing value, a differing type, a differing row
count, each shown to fail.

**One driver per category, generic over `B: Backend`.** `run_both(node, Script) -> Outcome`,
where `Script` names the call sequence in RecordBatches — `Exec(batches)`,
`Accumulate(batches)`, `Lanes(per-lane events)`, `Emit(batches)`, `Join { build, probe }`,
`Unload { batch, range }`, `Source` — and the harness refuses a script whose shape is not the
node's category. Executors come from `Backend::executors_for` with the post-order the tree
gives, so what is exercised is the trait, not a constructor. `Outcome` carries each side's
`Result`: a one-sided `Err` is the failure a `bug_` test then pins by message.

## Cases in this task

The three operators whose recipe carries no seq, plus the helper round trip:

- upload then fetch: whole, a row range, a range past the end, zero rows;
- `GpuUnload` through `executors_for`: whole, ranged, clamped, a zero-row batch, a range over a
  zero-row batch;
- `GpuLimit`: an interval inside one batch, one straddling two, batches entirely outside it,
  skip only, a stream of several batches, a zero-row batch inside a stream, a stream of nothing
  but zero-row batches, and an interval no batch reaches.

Empty inputs are separate cases everywhere, here and in task 9, one per shape — a zero-row
batch, no batch at all, one side or one lane empty — because the frozen surface cannot make a
table out of nothing ([#173](../tickets.md#t173)) and each shape reaches that limit by a
different route.

And a guard: every `NodeRef` kind is named by at least one case, in both directions, with the
three forwarders as the listed exclusions — they have no executor and belong to the driver,
which is tested elsewhere. Each case declares its kind in a registry the guard reads; the guard
never reads source text. Until task 9 fills the registry, the guard also carries the list of
kinds still without a case, checked both ways like the exclusions, and task 9 empties and
deletes it. It is what makes "comprehensive" a red test rather than a claim.

## Scope

Code expected to change:

- `cpp/include/peacock_gpu.h`, `cpp/src/gpu_executor.cpp`: `peacock_handle_from_arrow`;
  `cpp/src/plan_executor.h`, `cpp/src/node_session.cpp`: `NodeSession::adopt`.
- `peacockdb-ffi/src/lib.rs`: the extern, one entry.
- `peacockdb-core/src/wire/attach.rs`: the `try_as_node_ref` arm in `emit`;
  `peacockdb-core/src/wire/writer.rs`: the stub made reachable from that arm.
- `peacockdb-core/src/tests/`: `Given` lifted from `executor/cpu_backend/tests/` (whose own copy
  goes and whose call sites rename), `synthetic` and `decimals`, the comparator with its red
  cases, the kind registry and its guard.
- `peacockdb-core/src/tests/gpu_tests/`: `Device`, `Script`, `Outcome`, `run_both`, and this
  task's cases.
- `llm-wiki/build-test.md`: one row; `llm-wiki/architecture.md`: the ABI count.
- Nothing in `.github/workflows/`, `plan/`, `planner/`, `executor/`, or the wire format.

Component-level API expected to change:

- The C ABI: one additive symbol, test-only. `NodeSession`, the de facto C++ interface: `adopt`.
- `wire::attach_recipes`: accepts a leaf outside the registry, which emits a stub node and no
  recipe. Its signature and every other `wire/mod.rs` item are unchanged; `Writer` gains one
  `pub(super)` method inside `wire/`.
- `crate::tests` and `crate::tests::gpu_tests`, test modules rather than components: the items
  above are new. No component facade gains or loses an item.

## Constraints

- **No fix, anywhere.** Not in an operator, not in the C++, and not in the harness by casting or
  filtering a divergence away. A divergence or a one-sided failure is a ticket — an existing
  one where the defect is the same, a new one otherwise — and a `bug_` test asserting the wrong
  behaviour with the ticket above it. A ticket whose defect a case here no longer reproduces
  stays open: a ticket closes when its corpus cells run, not when a fix is visible in the code
  or a hand-built case passes. The detail file notes it, and nothing else moves.
- Synthetic data only. No sf1, no `testdata/`; the GPU job needs no dataset for any of this.
- Exact comparison. No tolerance argument exists.
- No driver and no forwarder: the harness calls executors, never `run`.
- Production code moves in two places only: the additive test-only symbol, and the one
  `attach.rs` arm with the writer method it calls. No production behaviour changes.
- Rung discipline as `test-layout.md` set it: the leaf, the batch and the comparator compile
  under `rust-only` in `src/tests/`; everything touching a device sits under
  `src/tests/gpu_tests/` behind `feature = "gpu"`. No new CI line — the lib binary's
  `gpu_tests::` filter already reaches it, and `test_ci_coverage` says so or goes red.

## Verification bar

- The comparator's red cases fail for the reason each names.
- Round trip, unload and limit green on shad-gpu; the kind guard green with the three
  exclusions and the pending list, and shown red once with a kind removed from it.
- `cargo test --features rust-only -p peacockdb-core --lib` still compiles and passes: the
  rust-rung modules carry no device type.
- `test_ci_coverage` green without a workflow edit.
- `build-test.md` gains one row for the harness, in its table's terms.

## Completeness signoff

Solved under its constraints: two production edits, both mechanism — the test-only upload
symbol with `adopt` behind it, and the one `attach.rs` arm — and no behaviour change, the recipe
payloads byte-identical; the harness under `src/tests/` with a driver per category over
`Backend`, exact comparison slot by slot with its red cases shown, and a kind guard red both
ways with the three forwarders excluded and thirteen kinds pending for task 9; the round trip,
seven `GpuUnload` and ten `GpuLimit` cases agreeing on both backends, so no ticket and no `bug_`
test. Deviations, none a shortcut: `pub(crate)` for the spec's `pub(super)`, and
`#[cfg(all(test, feature = "gpu"))]` for its `feature = "gpu"`, both the layout test's forms;
the guard in `gpu_tests/`, since `inventory` collects per binary; one CPU gtest beyond the file
list; `Script` allowing dead code for the four variants task 9 constructs; 26 device cases, not 21.

---

<!-- archived from llm-wiki/tasks/visibility.md -->

**Merged 2026-09-15 as PR #148, squashed to `efb41c72`.**

# 6 — the crate's API becomes the CLI's, and the walls go up

Kind: production

Last of six, after [`test-support.md`](test-support.md). That task put the corpus harness behind
the feature, so nothing outside the crate needs an engine type any more. This one takes the
surface down to what the CLI calls, deletes the registers task 2 had to build, and writes the
rules that keep it there. It is also the sweep: three tasks deferred work to "later" without
naming a task, and this is that task.

The two halves are the same thing from two ends. The **surface**: 174 bare `pub` items become
eight. The **walls**: `pub mod` survives only in `lib.rs`, and `coding-style.md`'s Visibility
section stops carrying an exemption at all.

## Where the surface stands entering this task

Measured after task 2, to be re-measured after task 3:

| | count |
|---|--:|
| bare `pub` items in `src/` | 249 |
| of those, in a component or subcomponent `mod.rs` or `lib.rs` | 174 |
| behind the nine exempt `pub mod` paths, which task 4 demotes | 75 (60 excluding the two subcomponent facades) |
| `pub mod` declarations | 15 — six components in `lib.rs`, nine exemptions |

The two rows are disjoint and sum to 249. Task 4 takes the 75, so this task starts from **174** and
ends at **eight** bare `pub` items in three files, six `pub mod`, and no register — 166 demotions,
every one checked by the compiler. Every other item becomes `pub(crate)`, which is all a sibling
component ever needed: components live in one crate, so a component API is `pub(crate)` and only
the CLI's entry points are `pub`. That is the whole of "the crate's API becomes the CLI's".

## What tasks 1-3 leave here

Each of these is stated somewhere as deferred, parked or open, and none has a task. Closing them
is this task's work, not an appendix to it.

**Formatting and wording residues.** The `parquet_meta.rs` rustfmt hunk task 1 deferred to task 2
was never applied and still reports. Three files were left unformatted in task 2's second review
round — `test_cpu_end_to_end.rs` (which task 4 moves to `src/tests/`), `cpu_backend/expr_physical.rs`, and `corpus_gpu.rs` (which task 5 moves to `src/test_support/`).
About twenty comments still use "mode" as a common noun for the thing task 1 retired.

**Guards that under-report.** `no_public_signature_names_a_type_from_a_private_module` matches only
`alias::`/`module::` prefixes, so a bare type imported out of a private module and named in a `pub`
signature passes; that becomes load-bearing here, where the private set grows by 166 items.
`names_the_module`'s reverse half misses `use peacockdb_core::executor::cpu_backend;` because there
is no `::` after the path. The super-climb reader reports one `super::super::x` at depth 0 twice.

**Two tickets to file rather than fix**, because each is a different subject: the murmur gate
re-derives `pmod` and the seed-42 pre-fill locally instead of calling `rows_per_lane`, so one rule
has two copies; and the repo is not rustfmt-clean, has no `rustfmt.toml`, and `pipeline.yml` runs
neither a fmt nor a clippy step. Only the first earns a ticket: `prompts.md` files tickets for
production behaviour and names cosmetics as the case never filed, so the formatting gap is recorded
in `build-test.md`, where CI shape lives, and not in `tickets.md`.

**One contradiction to correct in writing.** `module-layout.md` and
`peacockdb-core/tests/common/memory_limit.rs` both say `test-layout.md` creates `src/test_support/`.
It does not — it hands the feature and the module here. Fix both call sites in the commit that
moves the file, or the next reader trusts the comment over the spec.

**Baseline tooling outlives its task.** `module-layout-baselines/` is described as scaffolding
deleted when that task is archived, but `visibility-dump.py`, `case-inventory.sh` and
`compare-inventory.sh` are checks in tasks 3 and 4. They already live in `scripts/`, moved there by task 2's
completeness commit; `doc-attr-check.py`, `narrow.py` and `external-names.py` are task
2's own and die with it. This task inherits them there and adds nothing.

## The demotions

Task 3 raises the nine subcomponent walls as it moves the tests that forced them, and demotes the
75 items behind them. What is left here is the other 174: everything a component `mod.rs` declares
for its siblings, which needs `pub(crate)` and has been spelled `pub` because components are
`pub mod`.

**`#![warn(unreachable_pub)]` goes on in the first slice, but it is a backstop, not the work
list.** The lint fires on a `pub` item that is not reachable from outside the crate — and every one
of the 174 sits in a component `lib.rs` declares `pub mod`, so it reports **zero today** and would
report zero after a task that demoted nothing. It cannot measure this work.

`scripts/visibility-dump.py` is the work list: `awk '$2=="pub" && $3!="mod"' | wc -l`, 174 falling
to eight, one slice at a time. What the lint buys is the future — once a component's items are
`pub(crate)`, a `pub` written inside a private module is unreachable and the lint says so, which is
the rule enforcing itself after this task rather than during it. It stays at `warn`: the crate's
warning count is already a checked baseline, so a new one fails that check without a second
mechanism.

Each component's `mod.rs` then keeps only what the CLI needs as bare `pub` and demotes the rest:
`plan/mod.rs` (92 → 0), `executor/mod.rs` (48 → 2), `wire/mod.rs` (20 → 0), `planner/mod.rs`
(7 → 4), `plan_text/mod.rs` (3 → 0), `planner/translator/mod.rs` (2 → 0), `lib.rs` (2 → 2). A
component staying `pub mod` while every item in it is `pub(crate)` is the intended shape: the
module is nameable, its contents are not.

The eight the corpus harness forced go the same way once the harness is behind the feature — they
are ordinary component items with an unusual reason for having been `pub`, and after the move
their reason is gone.

### The hoist is not here, and this is why

Task 2 could not put the backend types behind a `mod` wall, because `test_cpu_executors` and
`test_gpu_executors` were separate crates and a separate crate cannot reach a private subcomponent.
It listed three answers — open the walls with `pub mod` (taken), declare the 14 types in
`executor/mod.rs`, or the full hoist of 14 types and 55 inherent methods — and deferred the third
as "the one the rules ask for", to be done "later at leisure".

All three answer one question: how does a **separate crate** reach those types. Task 3 answers it a
fourth way by ending the separation, so the question is gone rather than deferred.

Two things confirm it rather than assume it. Measured on the post-task-2 tree, exactly one reach
into the backend child modules comes from outside their own directory — `wire/tests.rs:831`'s
`CpuJoin`, which task 4 replaces with a two-hop `has_finish_pass` delegation rather than a hoist. `executor/driver`, the consumer that would justify
the other thirteen, names none of them: it goes through the `Backend` trait. And the hoist's
destination works against this task — types declared in `executor/mod.rs` are there to be `pub`,
while this task takes that file to two bare `pub` items, so hoisted types would land as
`pub(crate)` and be no more reachable than they were in `accumulate.rs`.

No ticket either. `coding-style.md` files tickets for production behaviour and never for
cosmetics, and a rearrangement with no consumer is the cosmetic case exactly. This paragraph is the
record, so the next reader meeting `pub(crate)` items in `accumulate.rs` does not re-derive it.

## The facades, after

Four kinds of boundary, and after this task each is exactly one thing.

- **The crate** exposes eight items in three files. That is the CLI's API and the whole of it.
- **A component** — `plan`, `planner`, `executor`, `wire`, `plan_text` — is a directory whose
  `mod.rs` declares its whole API, `pub(crate)`, reachable by sibling components and by nothing
  outside the crate. `common.rs` is a file rather than a directory: what the components share,
  declared in one place, and already at zero bare `pub`.
- **A subcomponent** — `executor/cpu_backend`, `executor/driver`, `planner/translator` and the rest
  — is declared `mod`, so it is reachable only from inside its parent component, and its API is its
  own `mod.rs`.
- **`test_support`** is a component-shaped child of the crate root whose API is bare `pub` behind a
  feature, with signatures free of engine types.

No register, no exemption, no `CROSS_COMPONENT_REACHES`. A `pub mod` below `lib.rs` is a violation
with no sanctioned form, which is what makes the rule readable at last.

## coding-style.md's Visibility section is rewritten

Task 2 wrote the rules and, honestly, the exemption beside them: nine sanctioned `pub mod`, 60
items behind them, a register checked both ways, and a paragraph explaining why the register is a
register rather than a habit. All of that was true of a tree with test crates reaching in. None of
it is true after task 3.

What goes: the exemption section entire, the `CROSS_COMPONENT_REACHES` paragraph, and the
"nine more `pub mod` exist, every one forced by a test crate" clause.

What stays, unchanged: the component and subcomponent rules, `mod` not `pub mod`, no `pub use`,
`mod.rs` bodies of one expression, three-deep nesting where the innermost earns it, absolute
`crate::` paths across a boundary, the length exemptions for `mod.rs` and `common.rs`.

What arrives:

- **The crate's API is the CLI's.** Bare `pub` in `src/` means "the binary calls this". Everything a
  component exposes to its siblings is `pub(crate)`. A new bare `pub` is a claim that the CLI needs
  it, and the layout test asks for the receipt.
- **`#![warn(unreachable_pub)]` is what keeps that true afterwards**, once the components' items
  are `pub(crate)` and a `pub` written inside a private module is genuinely unreachable. It cannot
  measure the task itself — while a component is `pub mod`, every item in it is reachable and the
  lint is silent. That is also why the rule is not merely a convention afterwards: Inside a private module `pub` and `pub(crate)` are identical to rustc —
  the module's own privacy is the wall — so the distinction is for the reader, for the blast radius
  when a module is ever opened, and for `private_interfaces`, which passes silently over a `pub`
  type that nothing can name and fires on the `pub(crate)` one. The lint is what makes the first of
  those three self-enforcing.
- **`pub mod` appears in `lib.rs` and nowhere else.** Six components; no exemption, no register.
- **A test-support signature is free of engine types.** The rule that keeps a facade from being a
  rename.
- **What the compiler enforces and what the layout test has to.** Task 2's honest three-way split
  survives the rewrite — module privacy is rustc's, sibling reach and where a `pub` appears at all
  are the layout test's, and a `pub` type that is unreachable but nominally public defeats
  `private_interfaces`, so that is the layout test's too.

## The surface, after

Bare `pub` appears **eight times in `src/` outside the feature gate, in three files**.

| Item | Declared in |
|---|---|
| `build_session_state`, `register_tables_for` | `lib.rs` |
| `plan`, `PlanKnobs`, `BatchSizing`, `SMALL_TABLE_BYTES` | `planner/mod.rs` |
| `run`, `CpuBackend` | `executor/mod.rs` |

**The eight are not closed under their own signatures, and the table has to say so.** `plan`
returns `Box<dyn GpuNode>` and `MemoryModel`; `run` takes `&dyn GpuNode` and a `B: Backend` and
returns `RunReport` and `RunError`. A `pub` item whose signature names a `pub(crate)` type is a
`private_interfaces` warning on the very item this table keeps — against a warning baseline this
task checks. So the surface is these eight **plus the types they name**, and the first slice
enumerates that closure from the signatures rather than guessing it: walk the eight, collect every
type in their parameters and returns, and keep those `pub` too. If the closure comes out large,
that is the honest size of the CLI's API and the table grows; what must not happen is eight `pub`
items sitting on types nothing outside can name.

`test_support/mod.rs` is a further file carrying bare `pub` and the only one behind a feature. The
guard distinguishes them: eight unconditional items checked by file and name, a feature-gated set
checked by signature.

## Validation

No test case moves in this task and no golden is touched, so both are pinned as invariants rather
than checked as outcomes. What moves is visibility, and a visibility regression compiles.

### Baselines

1. The `pub`/`pub(crate)` item dump with declaring files, from the end of task 3, taken with
   `scripts/visibility-dump.py`.
2. `--list` for all three lib shapes and the remaining binaries, and their leaf-name sets.
3. `sha256sum` over `testdata/goldens/`.
4. The three registers in `test_module_layout.rs` — `PUB_MODULES`, `CROSS_COMPONENT_REACHES`,
   `PUB_OUTSIDE_A_MOD_RS` — with their entry counts.

### The checks

- **The surface lands on exactly eight**, asserted as the table above — file and item, not a count.
  A count passes when one item is dropped and another added. Every other item must appear in the
  dump as `pub(crate)`, not merely as absent: an item deleted and an item demoted look the same to
  a count and different to this.
- **`pub mod` appears six times unconditionally, all in `lib.rs`**, plus `test_support` behind its
  feature — the same unconditional-versus-gated split the surface table makes. `PUB_MODULES` is
  deleted, not emptied: an empty register is an invitation.
- **`unreachable_pub` reports zero, and is known to be armed.** Run the three build shapes and
  confirm zero — which is weak evidence on its own, since it also reported zero before the task. So
  construct the violation: spell one item in a now-private implementation module `pub`, watch it
  warn, revert. That is the check; the count is not.
- **`CROSS_COMPONENT_REACHES` is deleted**, and the reach it named is already gone — task 4
  replaced it with the `has_finish_pass` delegation when it raised the `cpu_backend` wall.
- **The private-type-in-a-public-signature guard is fixed first, then relied on.** It matches only
  path-prefixed types today; with 166 newly private items it is the guard most likely to be needed
  and most likely to miss. Fix it, prove it red on a bare imported type, then run it.
- **Case counts and leaf-name sets identical to task 3's**, on all three shapes. Nothing moves
  tiers here; a count that shifts means a test followed the harness by accident.
- **Goldens byte-identical.** Nothing in this task can reach them; a diff means the harness changed
  behaviour while moving 698 lines of test code.
- **The residues are gone**: `rustfmt --check` is clean on the four files named above, and no
  comment uses "mode" as a common noun for what task 1 retired.

### Slices

Six components, one slice each, every one compiler-checked: turn the lint on first so its warning
count is the work list, then `plan`, `planner`, `executor`, `wire`, `plan_text`, `common`. After
each, the lint's count and the dump's bare-`pub` count both fall; a slice that moves neither moved
the wrong thing. Each slice ends by appending its state to `test-support-detail.md` and handing
back.

The corpus move is not here at all — [`test-support.md`](test-support.md) did it. That is the cut:
698 lines of test code changing compilation unit is a diff a reviewer must read, and ~300 one-word
demotions is a diff a reviewer can only skim, so they are judged separately or the first hides
inside the second.

## Done when

`peacockdb-core` exposes exactly the eight items in the table, every other former `pub` demoted to
`pub(crate)` rather than deleted; `pub mod` appears six times unconditionally and only in `lib.rs`,
with `test_support` gated beside them; `#![warn(unreachable_pub)]` is on and reports zero; both
registers are deleted and the reaches they sanctioned are gone; case counts, leaf-name sets and
goldens are unchanged; `coding-style.md`'s Visibility section carries rules and no register; the
carry-over list above is closed item by item, with the two tickets filed rather than fixed; and CI
is green.

## Completeness signoff

Solved under its constraints, with the surface at its honest size: 46 bare `pub` items and five
fields in five files — nine the CLI names, seven `plan` closes over, thirty `run<B: Backend>`
does, each hop named by the compiler — asserted by file and name both ways; every other former
`pub` is `pub(crate)`; `pub mod` is pinned to `lib.rs`; both registers are gone; the lint is on
and at zero; goldens, counts and leaf names hold but for one layout case renamed for its new
subject. Deviations, none a shortcut: five facade delegates only tests called were deleted and
three moved into test modules; `wire`, `plan_text` and `executor/gpu_backend` have no production
caller and say so with `cfg_attr` allows; the spec's checked-warning-count premise was false, so
`SURFACE` is the gate and the lint the signal; `RunReport.calls` is read by nothing and ~65
`pub` fields on `pub(crate)` structs stay, both under `allow` or unreachable; 66 comments, not twenty.

---

<!-- archived from llm-wiki/tasks/test-support.md -->

**Merged 2026-09-15 as PR #147, squashed to `a76563f3`.**

# 5 — the corpus harness moves behind the feature

Kind: production

Fifth of six, after [`test-layout.md`](test-layout.md) and before
[`visibility.md`](visibility.md). Task 4 created `src/test_support/` for the helpers that had two
audiences and moved most of them; this task moves the last two files, and with them the reason the
final eight items are `pub`.

Those eight are `GpuNode`, `validate`, `RunReport`, `render_run`, `GpuBackend`, `GpuContext`,
`RecipePlan` and `attach_recipes`. They exist for `test_cpu_corpus` and `test_gpu_corpus`, which
deliberately stay external. Once the harness they share is inside the crate, nothing outside names
an engine type — and [`visibility.md`](visibility.md) can then take the whole surface down to what
the CLI calls.

It is a small task with one hard claim, and that is deliberate: 698 lines of test code changing
compilation unit is a diff a reviewer has to read line by line, and it should not be sharing a
branch with three hundred one-word demotions.

## The corpus facade

`corpus.rs` (508 lines) and `corpus_gpu.rs` (190) — 698 lines — join the helpers already in
`src/test_support/`. `mode.rs` and `memory_limit.rs` moved with task 4, which needed them. Inside
the crate these two reach `pub(crate)` items, so the eight stop being `pub`.

The two corpus targets stay because they are the genuine end-to-end tier — SQL in, rows out, against committed
goldens, 456 cases — and because keeping them as two binaries keeps `inventory`'s
one-binary-per-engine property resting on two `--test` targets rather than on the `gpu` feature
producing two compilations. The feature route would work and would make the registry guard depend
on something that reads as unrelated to it.

The two binaries name **none** of the eight. They call functions whose signatures name no component type: `cpu_case(dataset, sf, query, mode, oracle)`, `authoritative_mode`, `gpu_case`.
That is what makes the facade real rather than a rename. `over_cap` travels with `corpus.rs` and
`test_corpus_goldens` reaches it the same way. `golden_text.rs` and `registry.rs` are already in
`test_support` — task 4 moved them, because targets it moved needed them too. `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` name zero crate items and are read only by binaries that stay,
so they stay in `tests/common/` untouched.

## The mechanism is already here

`test-layout.md` declared the `test-support` feature and the self dev-dependency, because the
helpers it moved had two audiences — in-crate tests and the binaries that stayed — and duplicating
one across the boundary guarantees drift. This task adds no mechanism; it adds the last two files
to the module that mechanism created, and the eight items stop being `pub` as a result.

What `#[cfg(test)]` still cannot do is unchanged: the library is compiled without `cfg(test)` when
cargo builds an integration test, which is the whole reason these eight are `pub` today.

## test_support is shaped like a component

`src/test_support/mod.rs` declares the whole API the integration tests may reach. Task 4 put the
harness half there — `Mode`, `MODES`, `MemoryLimit`, the golden-text reader, the registry loader
and the testdata root — and this task adds `cpu_case`, `gpu_case`, `authoritative_mode` and
`over_cap`. Every module below it is private with `pub(crate)` items. Same rules as a component, for a reason beyond symmetry: the signature
check below is a scan of one file only if the API lives in one file.

Being a child of the crate root it sees component facades and not their internals — the same level
as `src/tests/`, and the reason it reaches `pub(crate)` items without any of them becoming `pub`.

**No `pub` in `test_support` names a type from `plan`, `planner`, `executor`, `wire` or
`plan_text`.** That is the rule, stated by what it forbids rather than by a list of what it
allows — the module holds a `PathBuf` root, a golden-text reader and a registry loader, none of
which a list of "strings, `Mode`, `MemoryLimit`" would have permitted. A
signature mentioning `GpuNode` or `RunReport` puts the item straight back on the surface under
another name, and it compiles. This goes in `coding-style.md` beside the visibility rules, and the
layout test enforces it.

## Validation

No test case moves and no golden is touched. What moves is 698 lines between compilation units,
and the failure mode is a harness that changed behaviour while moving.

### Baselines

1. `--list` for all three lib shapes and the seven binaries, and their leaf-name sets.
2. `sha256sum` over `testdata/goldens/`.
3. The `pub`/`pub(crate)` dump from the end of task 4, taken with `scripts/visibility-dump.py`.

### The checks

- **Nothing under `peacockdb-core/tests/` names any of the eight.** Grep the whole directory, not
  the two binaries: the names live in `tests/common/corpus.rs` and `corpus_gpu.rs` today, so a grep
  of the binaries alone is green before the task starts and proves nothing. A hit means
  the facade is a rename, which is the one way this task can look done and not be.
- **No engine type in a `test_support` signature.** Scan every `pub` in `test_support/mod.rs` and
  assert no parameter or return type comes from a component. Then construct the violation, `pub fn tree() -> Box<dyn GpuNode>`, and watch the
  layout test go red. This is the guard the whole facade rests on and the one that would otherwise
  never be exercised.
- **The feature is off in a plain build**, proven by construction: reference `crate::test_support`
  from a non-test path in `lib.rs`, confirm `cargo build` fails with `E0433`, revert. A passing
  build is not evidence — the module simply is not there to break anything.
- **No CI step passes `--features test-support`.** Grep the workflows and assert absence; if one
  does, the self dev-dependency is not doing its job.
- **`inventory` still sees two binaries.** Both corpus targets keep their own registry assertion
  and both must pass — the property the "keep them external" decision exists to protect.
- **Case counts, leaf-name sets and goldens unchanged.** Nothing moves tiers here; a count that
  shifts means a test followed the harness by accident.

## Done when

`corpus.rs` and `corpus_gpu.rs` are in `src/test_support/`; the two corpus binaries reach them
through `cpu_case`, `gpu_case`, `authoritative_mode` and `over_cap` and name none of the eight;
every `pub` in `test_support` has a signature free of engine types and that guard has been seen
red; a plain `cargo build` cannot name `test_support`; no workflow passes the feature; `inventory`
still sees two binaries; case counts, leaf-name sets and goldens are unchanged; and CI is green.

## Completeness signoff

Solved under its constraints: the corpus harness is inside the crate behind the feature, the
three binaries reach it through signatures free of engine types, nothing under `tests/` names
the eight, the guard has been seen red on the spec's probe and on three further spellings, a
plain build cannot name `test_support`, no workflow passes the feature, `inventory` rests on two
`--test` targets, and goldens, leaf-name sets and the 200 bare `pub` outside `test_support` are
byte-identical to the baselines. Deviations, none a shortcut: `corpus_golden.rs` and
`cost_model.rs` moved too, since `corpus.rs` calls both and `src/` cannot see `tests/` — about
1200 lines, not 698; the facade declares 22 items, not four, for the three other suites that read
it; `Mode::knobs` and `Mode::sizing` narrowed to `pub(crate)`, the rule's first findings;
`read_back` deleted with no caller; `tests/common/mod.rs` survives as a six-line re-export shim.

---

<!-- archived from llm-wiki/tasks/test-layout.md -->

**Merged 2026-09-15 as PR #145, squashed to `1a04f096`.**

# 4 — tests down the source tree

Kind: production

Fourth of six, after [`module-layout.md`](module-layout.md) and
[`rmm-pool-budget.md`](rmm-pool-budget.md), and before [`test-support.md`](test-support.md) and
[`visibility.md`](visibility.md).

249 items in `peacockdb-core/src` are bare `pub`, measured after task 2. Most are `pub` for no
reason anyone can name — that is [`visibility.md`](visibility.md)'s subject. What this task ends is
the subset that is `pub` **because `peacockdb-core/tests/*.rs` are separate crates** seeing the
library the way crates.io would: 75 of them behind nine walls, and a hundred-odd more named
directly. This task moves the eleven targets that force them, plus the murmur gate, down
into `src/`, taking the surface from 108 test-driven items to **eight**. The last eight go in
[`test-support.md`](test-support.md).

Three things happen together, because none is worth a separate pass over the same files: the move,
the visibility sweep, and the separation of test code from production code.

## Why the demand is concentrated

Two files in `tests/common/` force 73 of the 108. `injection.rs` (822 lines) and `rebuild.rs` (623)
take a plan tree apart and put it back together, constructing every node kind and every backend
executor on the way. `join_fixture.rs` adds two more.

The other 2,488 lines of `tests/common/` — corpus, registry, golden text, result text, cost model,
mode table — need nine, and no target that keeps them touches the injector. That is what makes the
split clean rather than a judgement call.

Partial moves do not pay: 108 goes to 79 if only `common/` moves, 50 with the two executor tiers,
15 with three more, and 9 only when all eleven targets go. Do it once.

### Where a feature-gated helper pays, and where it does not

`#[cfg(test)]` cannot help an integration test at all: the library is compiled without `cfg(test)`
when cargo builds one, so a `cfg(test) pub` item is unreachable from `tests/`. That is the whole
reason these 108 exist. A **feature** can, and the mechanism costs nothing at the call site:

```toml
[features]
rust-only = ["peacockdb-ffi/rust-only"]
gpu = []                # device tests; not propagated to peacockdb-ffi, which has no device path
```

`gpu` and `rust-only` are mutually exclusive and a `compile_error!` says so. `gpu` does not
propagate to `peacockdb-ffi`: that crate is a C ABI binding with no device-conditional code, and
adding a feature there would be a knob nothing reads.

**The `test-support` feature is declared here, because here is where a helper first has two
audiences.** Nine of the eleven moving targets read `tests/common/` — `golden_text`, `registry`,
`mode`, `memory_limit` and seven `mod.rs` helpers between them — and the seven binaries that stay
read the same files from outside the crate. An in-crate `#[cfg(test)]` module cannot see
`tests/common/`, and duplicating a helper on both sides of the boundary guarantees the drift.

```toml
[features]
test-support = []
[dev-dependencies]
peacockdb-core = { path = ".", features = ["test-support"] }
```

The self dev-dependency turns the feature on for `cargo test` and leaves it off for `cargo build`,
so no CI step passes a flag and a plain build cannot see the module. `src/test_support/` is then
the one place a helper with two audiences lives: `crate::test_support::…` from inside,
`peacockdb_core::test_support::…` from the binaries.

**A helper moves when a moving target needs it, and not before.** `golden_text.rs`, `registry.rs`,
`mode.rs`, `memory_limit.rs` and the shared `mod.rs` helpers move; `corpus_golden.rs`,
`result_text.rs` and `cost_model.rs` are read only by binaries that stay, so they stay too, and
`corpus.rs`/`corpus_gpu.rs` are [`test-support.md`](test-support.md)'s whole subject.

**It does not pay for the injector.** `test_gpu_executors` (46 items) constructs `GpuAccumulator`,
`GpuEmitter` and `GpuJoin` and calls their methods; `test_cpu_executors` (24) does the same on the
other backend; `test_null_analysis` (18) hand-builds plan nodes. A facade over those re-exposes the
same vocabulary under test-local names — indirection, not encapsulation. Those targets move
in-crate instead, which is what takes 108 to single digits.

**It does not pay for the corpus harness either, in this task.** `corpus.rs` and `corpus_gpu.rs`
force the eight items that survive here, and moving them behind a feature is
[`test-support.md`](test-support.md)'s whole subject. They stay in `tests/common/` until then.

## Test code is never in a production file

Added to `coding-style.md`.

- No test code in a file that also carries production code. A module's unit tests are a child
  module of their own in their own file: `validate.rs` with `validate/tests.rs`, which is already
  the pattern in `expr_physical`, `accounting`, `index`, `scheduler`, `single_partition` and
  `expr_writer`.
- A test-only helper in `src/` lives in a file or directory whose name carries `test`, so a reader
  can tell test-only code from production code by the path alone. `driver/mock.rs` and
  `driver/plans.rs` are `#[cfg(test)]` today and do not say so in their names; they become
  `driver/tests/mock.rs` and `driver/tests/plans.rs`.
- Every test module declares the lowest rung it needs, per the ladder below, and its name carries
  that rung — `tests`, `ffi_tests`, `gpu_tests` — so a CI line can select one rung by path.

A dozen or so files hold inline `#[cfg(test)] mod tests { … }` and are split. **Do not work from
a list written here**: task 2 renamed and moved most of them and deleted `config.rs` outright, so
the count and the paths are both stale. Derive the set at the time of the move — the layout test's
own rule names them — and record what you found.

The mod.rs rule from `module-layout.md` applies to components and subcomponents, not to
implementation modules — an implementation module with unit tests is `foo.rs` beside `foo/tests.rs`.
Do not reach for `clippy::mod_module_files` to enforce the mod.rs rule: it cannot be scoped that
way and would reject exactly this pairing. The layout test can scope it, and has to exist anyway.

## The device guard becomes a feature

Today no `cfg` says "needs a device". `rust-only` says "no FFI linked", and `test_gpu_batch` is
`#![cfg(not(feature = "rust-only"))]` while running on a GPU-less runner. What actually keeps device
tests off CPU hosts is which binary CI runs where — and that mechanism disappears the moment those
tests are inside `--lib`.

Add a `gpu` feature. The three build shapes are a **ladder**: each rung adds a capability and
keeps everything below it.

| Build | FFI linked | Device assumed | Runs |
|---|:-:|:-:|---|
| `--features rust-only` | no | no | dataset-matrix, CPU steps |
| default | yes | no | dataset-matrix, FFI steps |
| `--features gpu` | yes | yes | shad-gpu only |

So a test declares the **lowest rung it needs**, and nothing declares what it excludes:

```rust
#[cfg(test)] mod tests;                                       // pure Rust
#[cfg(all(test, not(feature = "rust-only")))] mod ffi_tests;  // needs the FFI linked
#[cfg(all(test, feature = "gpu"))] mod gpu_tests;             // needs a device
```

Two conditions, both positive in meaning, and the sets nest rather than partition: the FFI set is
a subset of what a default build compiles, and a `gpu` build compiles all three. A reader answers
"which builds run this" from one attribute. `rust-only` is the only spelling available for the
middle rung — cargo features are additive and `rust-only` is subtractive, so there is no positive
`ffi` feature to write and inventing one would mean every ordinary build had to ask for the FFI
by name. A `compile_error!` on `all(feature = "gpu", feature = "rust-only")` keeps the ends of
the ladder exclusive.

**A module's name carries its rung whenever the rung is above the floor**, and that is what makes
a rung selectable. Cumulative shapes are the point of the ladder, but a CI line that runs a whole
shape re-runs every rung beneath it: unfiltered, the device host would drag the CPU unit cases
through `--test-threads=1` on the one serial resource in the suite, and the default-features line
would re-run what the `rust-only` line just ran.

| Rung | Module | Selected by |
|---|---|---|
| pure Rust | `tests` | nothing — it is the floor |
| FFI linked | `ffi_tests` | `-- ffi_tests::` |
| device | `gpu_tests` | `-- --test-threads=1 gpu_tests::` |

Those are path filters, not name filters, so they cannot suffer the trap `build-test.md` records
for filters that name a query. The layout test asserts name and gate imply each other in both
directions at both rungs: an `ffi_tests` module carries the `not(rust-only)` gate and a `gpu_tests`
module the `gpu` gate, and no module carries either gate without the matching name. Each CI line
then lists exactly its own rung, which is what makes "one line per rung" an accounting rather than
a slogan.

Two alternatives were considered and are not open. A runtime device check that skips is the shape
`build-test.md` already records shipping a hole — the binaries skip and exit 0, green having
verified nothing, which is why CI asserts sf40's presence itself. And `#[ignore]` already means
"disabled against a ticket" (#182), so reusing it for "needs a device" is one spelling for two
things.

## What moves into src/

Target names are the ones tasks 1 and 2 left behind, not the ones this spec was first written
against. The rung column is the ladder above: `rust` needs nothing, `ffi` needs the FFI linked,
`gpu` needs a device and lands in a `gpu_tests` module.

| Lands in | From | rung | N |
|---|---|:-:|--:|
| `plan/tests/` | `test_layout_injection` | rust | 4 |
| `planner/tests/` | `test_planner_join_capability` | rust | 13 |
| `planner/tests/` | `test_planner_join_refusals` | rust | 10 |
| `planner/tests/` | `test_null_analysis` | rust | 8 |
| `planner/tests/` | `test_plan_goldens` | rust | 19 |
| `wire/gpu_tests/` | `test_gpu_recipe_walk` | **gpu** | 10 |
| `executor/ffi_tests/` | `test_gpu_batch` | **ffi** | 3 |
| `executor/cpu_backend/tests/` | `test_cpu_executors` | rust | 1 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_executors` | **gpu** | 31 |
| `executor/gpu_backend/gpu_tests/` | `test_gpu_abi` | **gpu** | 4 |
| `executor/cpu_backend/gpu_tests/` | `test_murmur_conformance` | **gpu** | 10 |
| `src/tests/` | `test_cpu_end_to_end` | rust | 26 |
| `src/tests/` | `common/{injection,rebuild,join_fixture}.rs` | — | 0 |
| `src/test_support/` | `common/{golden_text,registry,mode,memory_limit}.rs` and the shared `mod.rs` helpers | — | 0 |
| | | | **139** |

`test_gpu_executors` is already a directory of five modules — `accumulate`, `backend`, `contract`,
`exec`, `join` — and moves as one, keeping that shape under `gpu_tests/`. `test_gpu_batch` is the
only occupant of the middle rung, which is why it is the shape proof the slices start with.

55 of the 139 are device cases. The injector and the rebuilder land at crate level rather than in
`plan/` because they construct backend executors as well as plan nodes, so they sit above both.

`src/tests/` is a crate-level `#[cfg(test)] mod tests;` declared in `lib.rs` — a peer of the
components that can see all of them. It holds the end-to-end tier and the shared test support the
component tests reach through `crate::tests::…`. The layout injector and the tree rebuilder live
here, not in `plan/` or `wire/`: they construct plan nodes, wire recipes and backend executors
alike, so they sit above every component rather than inside one.

### What makes test-only actually test-only

Three layers, and only the first is discipline.

- **The path says so.** `src/tests/…`, `component/tests/…`, `module/tests.rs`. A reader can tell
  test code from production code without opening the file.
- **One `#[cfg(test)]` at the root of each test subtree, and the gate is transitive.** `src/tests/`
  is excluded from a non-test build entirely — its files are never compiled, so `mod.rs` and the
  files below it carry no attribute of their own. Production code that references anything inside
  is a hard error in the release build (`E0433: failed to resolve`), not a warning and not a
  lint. That is the guarantee: it is the compiler, not a convention.
- **The layout test checks the shape**, because three things the compiler is happy with would
  still be wrong: a `#[cfg(test)]` attribute anywhere other than on a test-module declaration —
  which is how test code creeps back into a production file one item at a time — `coding-style.md`
  now says why the attribute cannot double as a `dead_code` silencer, and roughly twenty item-level
  uses across seven files answer to that, not the four this spec once named. `planner::translate`,
  `planner::translate_expr`, `executor::physical_expr` and `plan::state_for` — whose caller is
  `planner/translator/schema_tests.rs` — are the documented set, and they are
  **test helpers, not production items**: each exists to hand one test in one other component a
  fact it cannot reach itself. They keep `#[cfg(test)]` and stay in their component's `mod.rs`,
  because visibility pins them there — a helper must name what its own component owns while its
  caller is elsewhere, so the two can never sit together. That is the one carve-out to "a
  test-only path carries `test` in its name", and `coding-style.md` now states it. Each has
  exactly one caller today and each doc comment must name it; `translate`'s says "three of them,
  in two other components" and is already wrong, which is why the layout test reads the comment
  and the callers together rather than trusting either. **The chain still collapses**:
  `planner::translate` calls `Translator::new(..).translate(plan)` directly and
  `translator::translate` — `cfg(test)` too, and called only from here — is deleted, twelve lines
  out. What those fifteen lines buy is `plan_text/tests.rs` entire: 236 lines checking the
  renderer against what the planner emits from real SQL, which neither `plan()` nor a hand-built
  tree can stand in for. A name and a
  gate that disagree at either rung — `ffi_tests` without `not(rust-only)`, `gpu_tests` without `gpu`,
  or either gate on a module named `tests` — since the runs select by path and a mismatch either
  loses a case or drags it onto the wrong host; `driver/partitioned.rs` carries four
  item-level `#[cfg(test)]` attributes today (lines 616, 621, 626, 631); they are the first
  thing the first rule finds, and they move into the module with the tests that use them.

### Testdata paths move with the tests, and #49 is in the way

`tests/common/mod.rs` honours `PEACOCK_TESTDATA_DIR`, overriding the compile-time root "so a binary
built on one host can run on another". Every target that says `mod common` inherits that, including
the ones staged to shad-gpu. Nothing in `src/` does: seven sites bake the path instead —
`env!("CARGO_MANIFEST_DIR").join("../testdata/tpch.minimal")` in `planner/memory_estimation.rs` (×3),
`planner/translator/scan_mapping/parquet_meta.rs`, `plan_text/tests.rs` and
`planner/translator/{tests,schema_tests}.rs` — the list `tickets.md` keeps under #49. That is
the residual [#49](../tickets.md#t49) names.

Moving eleven targets in-crate walks straight into it: they lose `testdata_root()` and land beside
the seven that do it the unportable way, and the device ones then run on shad-gpu **from a binary
built on another host**, which is the case the variable exists for.

So this task closes that residual, and `test_support` is where the root belongs rather than
`src/tests/` — the moved targets, the crate's own unit tests and the seven binaries that stay all
need it, which is the same two-audience argument the feature exists for. `test_support/testdata.rs`, declared in `mod.rs`
honours `PEACOCK_TESTDATA_DIR` with the same compile-time fallback; `tests/common/mod.rs`'s
`testdata_root()` becomes a call to it rather than a second implementation, the seven `src` sites
call it too, and #49 closes with the sweep. Two spellings of one rule is what that ticket is
about, so any second copy re-files it one layer down.

### The exemption expires here, one slice at a time

`coding-style.md` says it outright: nine subcomponent paths under `executor/` are declared
`pub mod` rather than `mod` because test crates reach them, 60 bare `pub` items sit behind those
nine — 75 counting the two subcomponent facades the layout test skips — and "`test-layout.md` moves those test files into `src/`, and the whole exemption expires
with them". This task is where that happens, and it is not a consequence — it is work.

`PUB_MODULES` in `test_module_layout.rs` holds nine entries, each naming the test files that force
it, and **it is checked both ways**: an entry whose named files no longer force it is reported, and
a `pub mod` that is not in the register is a violation. The check is per **forcing file**, so a slice
that moves one edits every `forced_by` list naming it — five entries name `injection.rs` — and
demotes only the entries whose last forcer has now gone. Dropping an entry another target still
forces breaks that target and trips the reverse half of the check. Editing the list and demoting
what it empties are the same commit; leaving either for later is a red build, not a tidy-up.

Which slice closes what follows from the register's own `forced_by` lists: the injector trio takes
`cpu_backend/join` and `cpu_backend/source`, `test_cpu_executors` takes the rest of the
`cpu_backend` group, and `test_gpu_executors` and its child files take all four `gpu_backend`
entries. Task 2 recorded the trade this creates and left it deliberately: if
`test_gpu_executors.rs` alone stops naming `executor/gpu_backend`, the forward half goes red and
the three child-naming files cannot re-justify the entry. That is why the target moves whole.

**The one reach that blocks a wall is answered without a hoist.** `wire/tests.rs` names
`executor::cpu_backend::join::CpuJoin`, so raising the `cpu_backend` wall is an `E0603` on that
line. The register concluded that no delegation could carry it — true of the *type*, and it
stopped there. The test does not want the type: it builds a join per (join type, residual) cell
and asks one question, whether the executor makes a finish pass, to compare against the recipe's
`AtDone` call. So the components declare the question instead:

    // cpu_backend/mod.rs
    pub(crate) fn has_finish_pass(node: &GpuHashJoin, build: &ArrowSchema, probe: &ArrowSchema,
        ctx: Arc<TaskContext>) -> Result<bool, PlanError>

    // executor/mod.rs — the same signature, delegating to the above
    pub(crate) fn has_finish_pass(...) -> Result<bool, PlanError>

**Two delegations, not one**, because the wall this task raises is the reason: once
`cpu_backend/mod.rs` declares `mod join;` privately, `executor/mod.rs` cannot name
`cpu_backend::join::CpuJoin` either. That is the shape the tree already uses — `executor::physical_expr`
delegates to `cpu_backend::physical_expr`, which reaches `expr_physical`. Each body is one line and
drags nothing.

Take it **in the slice that raises the `cpu_backend` wall, before the demotion**, or that slice's
own `cargo test --lib` cannot pass: `wire/tests.rs` still reaches through the wall until the
delegation exists. `CpuJoin::makes_a_finish_pass` is renamed `has_finish_pass` with it, per the
predicate rule in `coding-style.md`. No hoist is needed here or in the tasks after.

`CROSS_COMPONENT_REACHES`'s single entry is that same reach and dies with it. The register itself
is deleted in task 4, not here: this task empties it, and an empty register is still a register.

## What stays a separate binary

| Target | Why it cannot fold in | rung | N |
|---|---|:-:|--:|
| `test_cpu_corpus` | `inventory` collects per linked binary; the registry needs two | rust | 448 |
| `test_gpu_corpus` | the other half of that pair, and it writes env vars | **gpu** | 8 |
| `test_golden_format` | the format reader, over strings | rust | 26 |
| `test_corpus_goldens` | committed sections against their own arithmetic | rust | 20 |
| `test_ci_coverage` | reads the workflow yaml | rust | 6 |
| `test_cost_model` | `.cost.txt` re-derived from `.cpu.txt` | rust | 3 |
| `test_module_layout` | reads the tree, as `test_ci_coverage` reads the yaml | rust | 11 |

`test_module_layout` is task 2's, not this task's to write — **this task extends it** with the
rules below rather than inventing a layout test. Where this spec says "the layout test", it means
that target.

None of the seven names an item that would otherwise have to stay `pub` beyond what `corpus.rs` and
`corpus_gpu.rs` already force. The `inventory` constraint
survives untouched, which is the one that looked fatal: it collects per linked binary and the two
corpus binaries both stay.

## test_ci_coverage shrinks to about 300 lines

It keeps its job and loses most of its subject. From 720 lines:

- The target sweep now covers seven targets, not nineteen, and `INTENTIONALLY_NOT_IN_CI` drops from
  six entries to two.
- The three GPU target lists become one staging list of one binary plus the `--lib --features gpu`
  run.
- Its own matcher unit tests stay whole. They are the reason this guard can go red at all.

It gains assertions, and they are the most important ones in the file, because the ladder puts one
CI line under each rung and nothing else says a rung stopped running. **Four things must exist**:
a `--lib` step under `--features rust-only`, a `--lib -- ffi_tests::` step at default features,
the staged lib binary in the GPU job's array **with `gpu_tests::` reaching it in the run loop**,
and the CLI build. The third is the one that is not a command line — the guard reads the staging
array and the loop, the way it already reads `for t in …` today. Each names its rung's filter, so each
lists exactly its own rung: the device line carries the 55 cases that move into `--lib` in this
task, and the default-features line the three of the middle rung, under `-- ffi_tests::`. Each gets the red-watch below — delete
the line, confirm the guard fails — because each is the only thing standing between a rung and
silence.

## Renames

**`test_inc2_conformance` becomes `test_murmur_conformance`**, and that closes a documented
exception. `coding-style.md`'s Names section opens by admitting the name breaks its own rule —
"named after an increment, which the second bullet forbids" — and justifies keeping it because
renaming would move the staging array, the exemption list and two pages. This task moves all three
anyway. Delete that paragraph; the rule no longer needs an apology beside it.

Sixteen references in ten files: `scripts/build-test.sh` (5), `pipeline.yml` (2),
`build-test.md` (2), and one each in `build-test-shadgpu.sh`, `test_cpu_executors.rs`,
`test_ci_coverage.rs`, `executor_cases.inc`, `coding-style.md`, `architecture.md` and
`cpp/tests/gpu/test_cudf.cpp`.

It is also the lowest-level test in the suite and moves in-crate with the rest: it names nothing
from `peacockdb_core`, driving comet's `create_murmur3_hashes` and one FFI symbol over raw arrays.
It lands beside `spark_partitioning.rs`, the CPU half of the invariant it protects.

That placement surfaces a gap worth a ticket, not a fix here: the test re-derives `pmod` and the
seed-42 pre-fill locally rather than calling `rows_per_lane`, so it proves the kernel matches comet
while the code that actually places rows is only transitively covered. One rule, two copies.

One of the five references in `build-test.sh` is not a comment: line 309 is a literal
`peacockdb-core:test_inc2_conformance` in a hand-maintained list, and the comments around it record
that this is the one test file the suite derivation does not catch by pattern — it was silently
skipped once. **Delete that literal rather than renaming it.** The target stops existing: it becomes
an in-crate `gpu_tests` module, and this task's own list leaves `RUST_TESTS` with one entry. A
renamed literal would name a `--test` target that is not there, which is the failure
[#176](../tickets.md#t176) describes — cargo errors late, inside the cuDF leg. The ten lines of
explanation go with it, as below. Re-count the sixteen references before editing: tasks 1 and 2
moved several of these files.

`driver/mock.rs` and `driver/plans.rs` become `driver/tests/mock.rs` and `driver/tests/plans.rs`.
`translate/schema_tests.rs` already carries the word and stays.

## build-test.md

The test table is restructured in this task, not a later one, and the axis changes. Today one
table carries every language and every kind of test; it becomes two.

**The first table is the Rust tests against production code** — `peacockdb-core` and
`peacockdb-ffi` — **blocked by rung**, because the rung is what decides where a case can run and
each block's total is one CI line's case count. A bolded header row opens each block; within a
block the rows are grouped by tier, then by category; and the Why, Examples and N columns survive
as they are.

    **cpu — `--features rust-only`: no FFI, no device**
      crate integration, external
      crate integration, internal
      component · subcomponent · module unit
    **ffi — default features: FFI linked, no device**
      component
    **gpu — `--features gpu`: shad-gpu only**
      crate integration, external · component · subcomponent

The `Runs` column goes. It said which CI job runs a row, and once the repo guards move to the
second table there is no variation left inside a rung: cpu and ffi are dataset-matrix, gpu is
shad-gpu, and the block header says so once instead of every row repeating it.

**The ffi block has two rows and five cases** — `test_gpu_batch`'s three and `peacockdb-ffi`'s
`test_ffi` — and the table should show that rather than pad it. A thin rung is a fact about this
engine: almost nothing needs the FFI linked and no device.

**The second table is everything else**: the C++ suites, the Python prototype and validators,
`cost-report`, and the repo guards — `test_ci_coverage`, which reads the workflow yaml,
`test_module_layout`, which reads the source tree, and `test_golden_format`, which tests the
harness's own format reader. None of the three runs engine code, and grouping them by what they
guard is more use than filing them by a rung they do not have.

`test_corpus_goldens` stays in the *first* table, cpu rung, crate integration external. It runs no
engine code either, but its subject is the engine's output and it goes red when the rendering
drifts, which is the distinction that matters.

Assign each file by where its test module is declared, not by eye: `driver/accounting/tests.rs` is
a unit test of an implementation module, `driver/tests/` is the subcomponent's, and `nodes/tests/`
is `plan`'s. The cross-checks are one per rung plus the binaries, and **every figure in this table
is pre-task-2**: it predates `test_module_layout`'s cases and five target renames, so rebuild it
from the baselines. The arithmetic that must hold is the page's, not this spec's — the two tables
add to the headline figure.

The two tables still have to add to the page's headline figure, which is how the page is checked.
Task 2 reported a four-case discrepancy there and fixed it in its completeness commit, so the
arithmetic is sound entering this task; keep it sound rather than re-deriving it.

Two facts the new shape makes visible that the current table cannot. The gpu block is what
shad-gpu runs, entire — **55** inside `--lib` selected by `gpu_tests::`, plus **8** in one binary —
so the cost of the serial host is one number a reader can find. And the coverage distribution
survives inside the cpu block, where `executor/driver` and `plan` are the heaviest subcomponent
and component rows and the corpus's 448 is one target rather than a tier.

## The three GPU target lists, and the two scripts

Today five GPU targets are named in three places that `test_ci_coverage` asserts agree:
`build-test.sh`'s `gpu_runtime_targets()`, `build-test-shadgpu.sh`'s `RUST_TESTS` at line 30, and
`pipeline.yml`'s staging loop. After this task the list is **one target plus a lib build**, which is
a bigger change to those scripts than to the lists.

**`build-test-shadgpu.sh`**

- `RUST_TESTS=(test_gpu_corpus)` — one entry.
- `stage_cargo_test_binary` resolves a built binary by matching `target.name` against a `--test`
  name in cargo's json. It needs a second form for the lib test target, whose json entry has
  `kind: ["lib"]` and `test: true` and whose `target.name` is the crate name. Stage it under an
  explicit filename — `peacockdb_core_gpu_lib` — because the run loop globs
  `cpp/install/rust-tests/*` and a bare crate name reads as ambiguous beside the target binaries.
- The build must pass `--features gpu`, and **the lib binary alone takes the `gpu_tests::`
  argument**: under the ladder it holds every rung, so an unfiltered run would put the CPU unit
  cases through `--test-threads=1` on the one serial host. The loop passes arguments to every
  staged binary alike, so this is a per-binary argument the loop does not have today.
- **The zero-test guard becomes load-bearing, and the two runners disagree about it.** A path
  filter that matches nothing runs no cases and exits 0, so a rename of the `gpu_tests` convention
  would be invisible without it. `pipeline.yml` arms it unconditionally — it reads `running 0
  tests` from the log and fails. `build-test-shadgpu.sh` suppresses it exactly when a filter is
  set (`elif [ "$rzero" -eq 1 ] && [ -z $filter_q ]`), on the reasonable ground that a
  `PCK_TEST_FILTER` legitimately matches nothing. **The lib binary's `gpu_tests::` argument must
  therefore not travel as `PCK_TEST_FILTER`**: it is part of what the binary is, not a
  developer's selection, and the guard must stay armed for it. Keep the two distinct in the script
  or the backstop is off in the one place a developer runs the suite by hand.

**`build-test.sh`**

- `gpu_runtime_targets()` shrinks to `test_gpu_corpus` plus the lib entry.
- **Ten lines of comment above it become obsolete and should go.** They explain that
  `test_inc2_conformance` is the only file gating per item rather than per file, which is why the
  membership test could not see it and why the list is written out rather than derived. After the
  move it is an in-crate `mod tests` gated on `feature = "gpu"` like every other device test, and
  the special case it documents no longer exists. Delete the explanation with the exception.
- `needs_cmake_targets()` derives from a file-level `#![cfg(not(feature = "rust-only"))]`, which
  still works and now finds one target. `--lib` is not a `--test` target, so nothing derives it —
  it must be named, in both the `--gpu` and the `--rust-only` paths, the way `pipeline.yml` names
  it today.
- The `--rust-only` path builds `--lib` without `gpu`; the `--gpu` path builds `--lib --features
  gpu`. Under the ladder these are not disjoint — the second is a superset — and the filter is what
  makes the runs disjoint. Say so where the lists are written, or the next reader reads the
  supersetting as a bug.
- The "derived suite must not be EMPTY" guard stays and gets closer to firing. With one derived
  target left it is one move away from being the thing that catches a mistake, so leave it.

**`pipeline.yml`** takes the same three-list change, plus one new step and one changed loop.
**shad-gpu never runs cargo** — the job stages prebuilt binaries and the remote loop executes
`$REMOTE_DIR/cpp/install/rust-tests/*` with `--nocapture --test-threads=1`. So the device rung is
a staged binary, not a command line: the staging array gains the lib target and the run loop
passes `gpu_tests::` to that one binary and to no other. The new step is the middle rung: `cargo test -p peacockdb-core --lib -- ffi_tests::` at default features on
dataset-matrix, which
**replaces the `--test test_gpu_batch` step it retires** — the job already compiles that feature
shape for `test_gpu_batch` and `peacockdb-ffi --test test_ffi`, so this is a swap, not a second
compile of the DataFusion stack, and the cache-thrash rule in `build-test.md` is not in play.
**`test_ci_coverage`** then compares three one-entry lists and asserts the four lines above.

## The surface, after this task

**Two counts, and only one of them is this task's.** Bare `pub` in `src/` is a raw number in the
hundreds — 249 measured after task 2. This task takes the 75 behind the nine walls, because a
`pub` item in a module it has just made private is what `unreachable_pub` is for and what the
demotion of that wall means; the other 174 are `pub` for no reason anyone can name and are
[`visibility.md`](visibility.md)'s subject.
What this task ends is `pub` **that an external consumer forces**, and that count reaches eight.

Sixteen items are `pub` for a reason at the end of this task, in six files: the CLI's eight, plus `GpuNode` and `validate`
(`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`, `GpuBackend` and
`GpuContext` (`executor/mod.rs`), and `render_run` (`plan_text/mod.rs`). All eight of those are
forced by `tests/common/corpus.rs` and `corpus_gpu.rs`, and [`test-support.md`](test-support.md)
removes them.

Five of them were lifted out of a subcomponent by task 2 and must stay lifted: `run` and
`RunReport` from `executor/driver`, `CpuBackend` from `executor/cpu_backend`, `GpuBackend` and
`GpuContext` from `executor/gpu_backend`, all declared in `executor/mod.rs` today. That is the rule
doing its work — the wall forces whatever must be public upward — and this task must not push any
of them back down.

## The wiki this moves

- **`build-test.md`'s test table is restructured here**, per the section above: the Rust tests
  against production code in one table blocked by rung — cpu, ffi, gpu — grouped by tier and then
  category inside each block, with Why, Examples and N kept and `Runs` dropped; the C++ suites,
  the Python sets, `cost-report` and the three repo guards in a second. The two must add to the
  headline figure, and the four-case discrepancy that sum has today is closed here.
- **`coding-style.md` gains the test-code rules** — no test code in a production file, a module's
  unit tests in a child module of their own, `test` in every test-only path, and the rung ladder:
  a test module declares the lowest build shape it needs and its name says which — `tests`,
  `ffi_tests`, `gpu_tests` — with name and gate implying each other at both rungs above the floor. It also loses the `test_inc2_conformance` exception paragraph that opens its Names
  section, since the rename closes it.
- **`coding-style.md`'s visibility section is amended, not rewritten**: the previous task states
  the rules, this one records what this task settled — the exemption register empty, `pub mod` down
  to six, and eight items still `pub` because a test crate forces them, which
  [`test-support.md`](test-support.md) unforces and [`visibility.md`](visibility.md) then removes
  along with the rest of the raw count. The
  `test_support` signature rule belongs to [`test-support.md`](test-support.md), which is where
  the corpus facade makes it load-bearing; the feature itself arrives here.
- **`architecture.md`** needs one line, not four: task 2 already corrected the driver and
  accountant paths, the wire-format writer paths and the `spark_partitioning.rs` pointer. What is
  left is the conformance gate's name at `architecture.md:971`, which this task's rename moves.

## Validation

This task moves 8,450 lines of test code between compilation units. Nothing it touches may change
what the engine computes, and no case may be lost — the two risks are opposite in kind and are
checked differently.

### Baselines

1. `--list` for `--lib` and every integration target, reduced to **leaf names** — strip module
   paths, because that is the set the move must preserve while the paths necessarily change.
2. `sha256sum` over `testdata/goldens/`. No golden may move at all in this task.
3. The `pub`/`pub(crate)` item dump from the end of task 2 — **it is a script, not a memory**:
   `visibility-dump.py`, whose output at the end of that task is `visibility-final.txt`.
   Re-run it here rather than inventing a second count, and state which of its rows the ladder
   below counts: bare `pub` only, test-gated items excluded. `case-inventory.sh` and
   `compare-inventory.sh` beside it are the leaf-name tooling for (1). All three already live in
   `scripts/`: task 2's completeness commit moved them there, because they are checks in this task
   and the two after it.
4. Warning counts from clean builds in all three feature shapes.

**Every figure in this spec predates task 2** — which renamed five targets, added
`test_module_layout` and its 35 cases, and moved the tree. Take the baselines first and work from
them. Where a measured number and a number written here disagree, the measurement is right and the
sentence is stale; say so in the detail file rather than bending the move to fit the page.

### The invariant: the case set is preserved, the paths are not

A case that disappears here is silent. Nothing goes red — a target runs 30 tests instead of 31, and
no assertion knows it was meant to run 31.

- **Leaf-name set equality.** The union across the three lib shapes and the seven binaries must
  equal the baseline union exactly. Not the count — the set, so a case deleted and another
  duplicated cannot cancel out. The lib shapes nest, so the union is taken after dedup; that is a
  consequence of the ladder and not a smell.
- **The rungs are what the counts must show**, each a separate assertion: `--lib` under
  `rust-only` lists the pure-Rust set; `--lib` at default features filtered by `ffi_tests::` lists
  **exactly the three** middle-rung cases; `--lib --features gpu` filtered by `gpu_tests::` lists
  the device set **and nothing else**. If either filtered run selects a case from a lower rung, a
  name and a gate disagree and the case is about to run on the wrong host.
- **The total is unchanged.** This task moves tests; it deletes none. Take the figure from the
  baseline rather than from this spec, which was written before task 2 added 35 cases.

### Move in slices

**A slice is a dispatch, not just a commit.** Each one ends by appending its state to
`test-layout-detail.md` — what moved, what the ladder number is now, what is unproven — and handing
back; the next slice starts in a fresh window from the baselines and that file. Task 2 was the
lesson: the same shape, five components with a commit each, run as one dispatch that spent eight
hours and died on a context limit with the work unreported.

**Slice 0 comes before any move**, because nothing can move until it exists: the `gpu` feature with
its `compile_error!`, the `test-support` feature with `test_support/testdata.rs`, declared in `mod.rs` and the seven
existing sites converted to it, and
`test_module_layout` extended with the rules above. It moves no test and its proof is that the
three build shapes still compile and the suite is unchanged.

Then one commit per target, ascending by how much it forces: `test_gpu_batch` (3 cases, 2 items)
first as the shape proof — it is also the only middle-rung target, so it proves the rung and its CI
swap together — then the four injector consumers with `injection`/`rebuild`/`join_fixture`, then the
executor tiers, then the rest. After each, the leaf-name set for that target must have moved from
its binary's list into the right module's list and nowhere else.

**Two ladders, and every slice moves one of them.** The surface as it falls — the spec's figures
were 108 → 79 → 50 → 15 → 8 and are re-derived from the baselines — and the register beside it,
**9 entries → 0**, with the `pub mod` count falling 15 → 6 as each is demoted. A slice that moves
neither moved the wrong thing.

### The device gate

- `cargo build --features gpu` and `--features rust-only` both succeed; the `compile_error!` fires
  when both are passed together, and that is checked by trying it.
- **Name and gate agree in both directions.** The layout test asserts it by reading the tree:
  construct a `gpu_tests` module without the gate, and a `gpu`-gated module not called `gpu_tests`,
  and watch each go red.
- On shad-gpu the staged lib binary, run with `--test-threads=1 gpu_tests::`, covers the device set
  in roughly what the five staged binaries took. Materially longer means the filter is selecting
  more than it should; `running 0 tests` means it is selecting nothing, and the guard must say so.

### Test code is separated

- `git grep -n '#\[cfg(test)\]' -- peacockdb-core/src` returns only test-module declarations —
  `mod tests`, `mod ffi_tests`, `mod gpu_tests` — and the cross-component entry points the
  carve-out permits in a component's `mod.rs`, each with a doc naming its caller. The tree has 21
  in eight files, including gated `use` lines above those entry points, which count as part of the
  declaration they serve. Anything else is test code in a production file, which is what
  this task exists to end, and `driver/partitioned.rs` lines 670-685 are where it starts.
- `git grep -n '#\[test\]' -- peacockdb-core/src` returns only paths containing `test`.
- The transitive gate is checked by construction: add a line in `src/tests/` referencing a private
  item, confirm `cargo build --release` still succeeds; add a line in `plan/` referencing
  `crate::tests::`, confirm it fails with `E0433`. Revert both.

### CI

`test_ci_coverage` shrinks here, so it is the one guard that must be shown red rather than merely
green. Delete each of the four asserted lines from `pipeline.yml` in turn and confirm the guard
fails on each: the device line, the default-features `--lib` line, the `rust-only` `--lib` line and
the CLI build. One rung per line, and the assertion is all that stands between a rung and silently
not running.

The two scripts change with it, per the section above; verify `build-test.sh --gpu` and
`--rust-only` both still produce a non-empty derived suite, since that guard is now one move from
firing.

### Then build-test.md

Edited last, from the measured numbers rather than from this spec. Its two tables must add to the
headline figure, and that arithmetic is how the page is checked. A spec number and a measured number
that disagree mean the move is wrong, not the page.

## Done when

exactly eight items in `peacockdb-core` are `pub` because a test crate forces them — `GpuNode` and
`validate` (`plan/mod.rs`), `RecipePlan` and `attach_recipes` (`wire/mod.rs`), `RunReport`,
`GpuBackend` and `GpuContext` (`executor/mod.rs`) and `render_run` (`plan_text/mod.rs`), all eight
forced by `corpus.rs` and `corpus_gpu.rs` and removed by [`test-support.md`](test-support.md);
`PUB_MODULES` is empty and `pub mod` is down from 15 to six, counted by `visibility-dump.py`; the
raw bare-`pub` count is whatever task 4 inherits and is not this task's claim. No
production file contains a `#[test]`; no `#[cfg(test)]` sits anywhere but on a test-module declaration or a
carve-out entry point in a component's `mod.rs`; every test module declares the lowest rung it needs and is named for it, with name
and gate implying each other at both rungs above the floor; every test-only path in `src/` carries `test` in its name;
`crate::test_support::testdata_root()` is the only testdata root anywhere, called by the crate's
unit tests, the moved targets and `tests/common/mod.rs` alike, and [#49](../tickets.md#t49) closes
with it; a plain `cargo build` cannot name `test_support`; `test_ci_coverage` is near 300 lines and asserts one CI
line per rung plus the CLI build; `build-test.md`'s two tables add to the headline; and the leaf-name
set is the one the baselines recorded — this task moves tests, it does not delete any.

## Signoff

Solved under its constraints: the twelve targets are in `src/` at the rung the table names, the
seven binaries stay, every baseline leaf survives in all three shapes and no golden moved; the
register is empty, `pub mod` is seven (`test_support` postdates the count of six), and the eight
items a test crate still forces are the corpus harness's. Two readings closed with no blocking
finding; the three important ones — the `test-support` feature undocumented, two private-field
readers admitted by a second carve-out case, `end_to_end.rs` over the length rule — are fixed.
Shortcuts: `build-test.sh`'s lib entry has run end to end only in `--rust-only` (verda was down;
the `--gpu` and default modes are rendered, syntax-checked and their resolver run against real
cargo json); the murmur gate's three CPU-runnable cases now run on shad-gpu only; the spec's
`crate::tests::`-from-`plan/` E0433 probe was not run, every cold production build standing in.

---

<!-- archived from llm-wiki/tasks/rmm-pool-budget.md -->

**Merged 2026-09-15 as PR #144 (merge commit `72fb23f8`).**

# 3 — the pool reserves what a binary needs

Kind: production

Inserted before [`test-layout.md`](test-layout.md), and independent of it: it touches `cpp/` and
one wiki page, and nothing in the layout refactor reads either.

Closes [#178](../tickets.md#t178) tentatively. Every gtest binary that installs a pool reserves 85%
of free VRAM and caps at 95, so two processes on the card cannot both have what they asked for and
the second dies in `pool_memory_resource` with `std::bad_alloc`. Measured once: two jobs
overlapping by under two minutes, three sf40 tests down, at a 14.38 GiB peak on a 139.7 GiB device.
Not a full device — two pools.

**CI no longer collides with itself**: `gpu-tests` carries `concurrency: {group: shad-gpu,
cancel-in-progress: false}`, which is the fix the ticket proposed and it has since landed, so runs
queue rather than overlap. What remains is the card being shared with work outside this repo, which
no group of ours can serialise — and a binary that asks for 85% of a device it does not own is the
part we can fix.

## The pool is not only in sf40 tests

The premise this task started from was that only the sf40 suites take a pool. They do not. Six
binaries call `peacock::install_rmm_pool()` from `main()`:

| Binary | sf40 | Runs |
|---|:-:|---|
| `test_tpch.cpp`, `test_tpchv.cpp` | yes | every gpu-tests job |
| `test_cudf_nodes.cpp`, `test_tpch_streamed.cpp` | yes | manual |
| `test_cudf.cpp` — the GPU smoke and the murmur3 kernel | **no** | every gpu-tests job |
| `test_plan_executor.cpp` — hand-built plans over `tpch.minimal`, 19 MB | **no** | every gpu-tests job |

So an ordinary CI run puts four processes on the host, each asking for 85% of what it finds free,
one of them for a dataset of nineteen megabytes. That is the shape of the collision, and sizing
only the sf40 pair would leave it in place.

`multi_gpu.cpp` is a seventh caller with its own per-device installation. It is manual, needs two
GPUs, and never runs in CI, so it is not part of this and **keeps the percentage constants**, which
stay in the header for it alone with a comment saying so.

## The change

`install_rmm_pool()` takes an explicit byte budget. No percentage, and **no clamp**: a binary asks
for what it needs and a host that cannot supply it fails to build the pool, which is already
`RmmPoolStatus::Unavailable` — the binary carries on with rmm's default resource, correct but
unpooled, and a caller taking timings asserts as it does today. A clamp would silently hand back a
smaller pool than was asked for, and the number a benchmark reports would stop meaning what it says.

Each of the six declares its own budget as a named constant beside its `main()`, with the
measurement that justifies it in the comment. **Take the numbers, do not choose them**: the pool
already carries a `statistics_resource_adaptor`, so run each binary and read its peak. Round up to
something a reader can defend, not to a percentage.

The FFI's `peacock_install_rmm_pool` gains the same argument, because it exists so a Rust caller
gets the allocator the gtest binaries have and that is no longer a fixed thing. What it must not do
is start reading `gpu_memory_limit`, which is stored and ignored — that is [#148](../tickets.md#t148)
and a decision about the product.

## Validation

- Every GPU tier stays byte-identical. This changes how much memory is reserved, never what is
  computed.
- **Two of each in parallel.** On shad-gpu, run `peacock_tpch_tests` twice at once and confirm both
  pass; that is the failure this task exists to make unreachable, and it has never been run
  deliberately.
- Each binary's declared budget is at least its measured peak, and the four that share a CI job sum
  to well under the device.

## The ticket stays open, tentatively closed

`#178` is not deleted and not archived. It is marked **tentatively closed** with the reasoning:
the pool no longer sizes itself against the device, so two runs fit; but the host is shared with
work outside this repo, so a third party can still exhaust it and this cannot be proven closed
from here.

The instruction that goes with it is for whoever meets it next, and it is deliberately narrow: **if
a GPU tier fails with `std::bad_alloc` in `pool_memory_resource`, add a dated line to #178 saying
which run and which binary, re-run the job once, and do not debug it.** The evidence accumulates on
the ticket until there is enough of it to say whether the sizing was wrong or the neighbour was
greedy. A coordinator that stops to diagnose this spends a dispatch on a machine it does not own.

`llm-wiki/prompts.md` carries the same line in the Coordinator section, because a coordinator does
not read `tickets.md` and would otherwise never see it.

## Done when

Six binaries declare explicit byte budgets with their measured peaks recorded; `install_rmm_pool`
takes bytes and does not clamp; the percentage constants remain only for `multi_gpu.cpp` and say
so; two `peacock_tpch_tests` run concurrently on shad-gpu and both pass; every GPU tier is
byte-identical; #178 is marked tentatively closed carrying the retry-don't-debug instruction; and
`prompts.md`'s Coordinator section carries it too.

## Completeness signoff

Solved under its constraints, with one Done-when item unmet and four things named rather than
hidden. Unmet: two `peacock_tpch_tests` at once never ran — a tenant outside this repo held 53 GiB
of the card for the whole task, and 69+69 needs an idle one. Two `peacock_tpchv_tests` did run
concurrently and both took their declared budget from very different free-memory readings, which
proves the rule and not the number. Shortcuts and deviations: the spec's premise for no-clamp is
false — nothing acts on `Unavailable`, and an unpooled sf40 run loses tests rather than running
slowly — and the carry-on behaviour was kept as specified, with the failure made legible instead;
the integrated sizing regime was deleted rather than replaced, so every budget is an H200 number
([#148](../tickets.md#t148) carries the open question); `PEACOCK_RMM_POOL_BYTES` was added past the
spec to restore what the deleted override gave the swept binaries; and 24 GiB for the 100M-row node
sweep is a first try, not a bisection.

---

<!-- archived from llm-wiki/tasks/module-layout.md -->

**Merged 2026-09-10 as `beb7455e`, squashed onto master by hand; PR #143 closed unmerged as a result.**

# 2 — a component's API is its mod.rs

Kind: production

Second of four, after [`drop-mode-name.md`](drop-mode-name.md) and before
[`test-layout.md`](test-layout.md) and [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

`peacockdb-core/src/batch_partitioned/**` moves to `peacockdb-core/src/`, laid out as components
whose whole API is declared in `mod.rs` with implementation behind private modules. The previous
task deliberately left this directory alone so that every file moves once, here, rather than twice.

Today 170 top-level items are `pub` and eight of them are named by another crate. This task does not
change that — it changes where they are declared and what may reach past them.
`plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver` becomes
`executor::run` as their modules acquire those names; `peacockdb/src/main.rs` moves with them.

### Five components

The IR is neither planner nor executor: it is what passes between them, and eleven non-test
consumers span both halves. Giving it to either makes that half's internals a dependency of the
other.

```
src/lib.rs
src/common.rs              the row-byte formula, today memory.rs
src/plan/                  what a plan is
src/wire/                  what crosses the FFI
src/planner/               making both
src/executor/              running them
src/plan_text/             rendering any of it
```

`wire/` is separate from `plan/` because they are two contracts, not one. A plan is the tree the
planner builds and the driver walks; a recipe is the menu of parameterized kernels that crosses to
the C++ side, and `architecture.md` already treats it as its own subject under that name.

**It holds everything that knows what a flat buffer looks like** — the vocabulary (`Recipe`, `Seq`,
`FbKind`, `Call`, `CallPattern`, `Input`, `ProjectRole`, `AbiSymbol`, `RecipePlan`), the writers
that are `recipe/` today, the reader, the two renderers that are `plan_text/fb_text.rs` and
`plan_text/recipes.rs` today, and the generated module.

That last one forces the shape. Eleven files name `crate::generated` — nine in `recipe/`, plus both
of those renderers — and they use **73 distinct generated types** between them, so flatc's output
cannot be walled off unless its callers are inside the same wall. `plan_text/recipes.rs` belongs
with them because it parses the buffer itself (`flatbuffers::root_with_opts::<fb::GpuPlan>`); it
takes `render_plan_recipes` and `Payloads` with it, and `plan_text/mod.rs` asks `wire` for the
`--- recipes ---` section. The memory section is not the same case: `MemoryModel` is a plain struct
the renderer can walk without knowing the wire format.

With all eleven inside, `wire/mod.rs` exposes `Recipe`, `Seq`, `attach_recipes`, `payload_text` and
`render_plan_recipes`, and the 7,336 lines flatc emits are private to one component.

**Keep the `#[allow(unused_imports, dead_code, clippy::all)]`** that sits on the module today, and
say why in a comment. It is cosmetic while the module is `pub` — everything is externally reachable,
so `dead_code` cannot fire. Private, every generated type the crate does not name becomes dead code,
and there are hundreds.

`planner` therefore has no `recipes` subcomponent: writing a recipe is a fact about the wire format,
not about planning, and `attach_recipes` walks a finished tree.

`plan_text` is a peer because it renders a plan tree, a run report, the memory model and the
recipes, and because it holds the one real edge into the driver — `run_text.rs` reads `RunReport`
and `PlanIndex`. It sits above both.

### Where everything goes

| Today | Goes to | Because |
|---|---|---|
| `nodes/` | `plan/` | eleven non-test consumers across both halves |
| `node.rs` | `plan/mod.rs` | `GpuNode`, `RowInterval` — the trait the nodes implement |
| `layout.rs`, `schema.rs`, `expr.rs`, `aggregates.rs` | `plan/`, as implementation modules | one vocabulary, not four. As subcomponents every other component would have to reach through their walls to name `Expr` or `Schema`; declared in `plan/mod.rs` they are one facade |
| `validate.rs` | `plan/` | structural properties of a tree, not of planning; also removes the one executor-to-planner edge, `driver/partitioned.rs:28` |
| `error.rs` | split | `PlanError` to `plan/mod.rs`, `RunError` and `When` to `executor/mod.rs` |
| `RowGroupMeta`, `Batching`, `ScanMetadata` | `plan/` | `GpuLoadParquet` stores the mapping verbatim |
| `recipe/` entire, `plan_text/{fb_text,recipes}.rs`, and `lib.rs`'s `generated` | `wire/` | the eleven files that name `crate::generated` use 73 of its types; it cannot be private unless they are inside with it |
| `partitioner.rs`, `parquet_meta.rs`, `gpu_rowgroup_prune.rs` | `planner/translator/scan_mapping/` | all three entry points have one caller, and all three calls are inside `Translator::source`, a 20-line function (`translate/mod.rs:449,451,458`). As a peer of `translator` it would be the design's only sibling-subcomponent edge |
| `MemoryModel` | `planner/mod.rs` | already half of what `plan()` returns |
| `RunReport`, `PlanIndex`, `ROOT` | `executor/mod.rs` | what running a plan produces, plus the post-order addressing the recipes and the FFI share — not private driver bookkeeping |
| `expr_translate.rs` | `planner/translator/` | sole consumer is `translate/` |
| `nulls.rs` | `planner/` | a refusal the planner makes; sole caller `plan.rs` |
| `estimator.rs` | `planner/memory_estimation/` | as proposed |
| `executor.rs`, `backend.rs` | `executor/mod.rs` | the seven category traits and `Backend`; their cycle disappears when they share a file |
| `batch.rs`, `cpu_batch.rs`, `gpu_batch.rs` | `executor/` | `Batch` and the two implementations |
| the three `#[cfg(not(feature = "rust-only"))]` declarations in `batch_partitioned/mod.rs` | `executor/mod.rs` | `gpu_backend`, `gpu_batch` and `GpuBatch` — see below |
| `forwarder.rs` | `executor/` | routing with no backend and no executor — the driver owns it |
| `expr_physical.rs`, `spark_partitioning.rs` | `executor/cpu_backend/` | one consumer each |
| `driver/accounting.rs` | stays under `executor/driver/` | see below |
| `memory.rs` | `src/common.rs` | four consumers across planner, executor and both backends |
| `config.rs` | deleted | see below |

Result:

```
plan/mod.rs           GpuNode, RowInterval, PlanError, the eighteen nodes,
                      Expr, Schema, NodeKind, PartitionLayout, AggFunc, PlanAgg,
                      RowGroupMeta, Batching, ScanMetadata,
                      NodeRef, ExecutorCategory, category_of, node_name
plan/common.rs        check_column_refs, check_merge_keys, input_layout,
                      input_schema, rebase_through_projection
plan/{exec_ops,accumulators,aggregate,join,partition_ops,source,union,unload}.rs
plan/{expr,layout,aggregates}.rs
plan/validate.rs

wire/mod.rs           Recipe, Seq, FbKind, Call, CallPattern, Input, ProjectRole,
                      AbiSymbol, RecipePlan, Payloads, attach_recipes,
                      render_plan_recipes, payload_text, node_at, depth,
                      check_seq_kinds
wire/generated.rs     private — the include!, 7,336 flatc lines, 73 named types
wire/{node_writer,join,aggregate_writer,expr_writer,writer,read,
      fb_text,recipes}.rs

planner/mod.rs        plan(), PlanKnobs, BatchSizing, SMALL_TABLE_BYTES, MemoryModel
planner/nulls.rs
planner/{translator,memory_estimation}/
planner/translator/scan_mapping/

executor/mod.rs       Backend, the seven traits, Batch, CpuBatch, GpuBatch,
                      RunError, When, CallStats, RowRange, BatchForwarder,
                      RunReport, PlanIndex, ROOT
executor/{cpu_batch,gpu_batch,forwarder}.rs
executor/{driver,cpu_backend,gpu_backend}/

plan_text/
```

`plan/mod.rs` lands near 980 lines, measured: 561 of declarations with their doc comments, 120 for
the registry and its four exhaustive 18-arm matches, 272 of one-line delegations over 32 inherent
blocks, and the module header. It is the crate's central index and the one file a newcomer should
read, and it holds no logic — the one-expression rule is what keeps it a header rather than a
module. `wire/mod.rs` is about 300 — the vocabulary plus five delegating entry points. Nothing else
passes 400.

### A component whose API is conditionally present

`executor/` is the one component that does not have a fixed surface: `gpu_backend`, `gpu_batch` and
`GpuBatch` are `#[cfg(not(feature = "rust-only"))]` today, and the declarations move with them. So
`executor/mod.rs` carries the cfg on the declarations themselves, not only on the `mod` lines —
`GpuBatch`, `GpuBackend` and `GpuContext` exist in two of the three feature shapes and not the
third.

Two consequences. The layout test must read the cfg rather than the item, or it will report a
missing declaration under `rust-only` and a surplus one otherwise. And the `use common;` line at the
end of `executor/mod.rs` must not pull in anything device-only, since `common.rs` is compiled in
every shape — the rust-only tier boundary is exactly what a shared `common.rs` is able to breach.

### Why accounting is not its own subcomponent

`ResidentAccountant` is a field on the partitioned driver and is passed by mutable reference into
the lane state machine at four sites in `single_partition.rs`. `Held<T>` is the driver's in-flight
batch representation, `Slot` an index into its executor slots, `Trip` a `StepError` variant. Four of
the five types are driver internals; only `Underestimate` faces outward, through `RunReport`.

A sibling subcomponent would have to declare all four as API. As `executor/driver/accounting.rs`,
a private implementation module, they stay `pub(crate)` behind the driver's wall and nothing outside
`driver` can name them.

`ResidentAccountant` is the name of the thing. Retire "the enforcer" and "resident enforcer" as
second spellings for it, in the wiki and anywhere else — the type both accounts and enforces, and
enforcement is the smaller half.

### config.rs is dismantled

`MemoryLimit` moves to `src/test_support/`, which [`test-layout.md`](test-layout.md) creates for the
corpus harness that reads it; until that task lands it sits in `tests/common/` beside `bp_mode.rs`. `TargetPartitions`, `TARGET_PARTITIONS`
and `BATCH_STRESS_BUDGET` are named by nothing outside the file's own unit test and go. The module
doc describes `tp8-standard` device labels, which the legacy-mode drop retired.

## The visibility rules

For `coding-style.md`, replacing nothing that is there today.

- A component or subcomponent is a directory with `mod.rs`. Its whole API — structs, enums, traits,
  functions, constants — is declared there. Nowhere else in the component carries `pub` or
  `pub mod`.
- Implementation modules are declared `mod x;` and their items are `pub(crate)`. The module's own
  privacy is the boundary: a path through a private module is refused whatever the item says, so
  `plan::exec_ops` cannot be named from outside `plan` and `pub(super)` is not needed.
- **A subcomponent is declared `mod`, not `pub mod`** — `mod recipes;` in `planner/mod.rs`, never
  `pub mod recipes;`. `pub mod` would make `planner::recipes::Recipe` nameable crate-wide and the
  subcomponent wall would exist only on paper. What a sibling component needs is declared in the
  component's own `mod.rs`; that is what the four type moves in the table above are for.
- `lib.rs` declares the components `pub mod`, and they are the only `pub mod` in the crate.
- **Nesting may go three deep** where the innermost earns it: `planner/translator/scan_mapping/` is
  720 lines behind three entry points. The same rule applies at each level — `mod`, not `pub mod`.
  A directory with a one-item facade and a hundred lines behind it is an implementation module
  wearing a directory; the test is whether the body justifies the wall.
- `pub use` is not allowed. Inline the declaration into `mod.rs`, or into `common.rs` for what the
  implementation modules share. A child reaches into its parent; a parent never re-exports a child.
- A body in `mod.rs` is one expression. Declarations, and delegations of exactly one line.
- A struct keeps its inherent `impl`, and that block lives in `mod.rs` with one-line bodies. A trait
  is for two or more implementors. A trait per struct would also break every `const fn` and
  associated const, which trait items cannot be.
- An implementation module may implement any trait for a type declared in its own component's
  `mod.rs`, and may define free functions the `mod.rs` delegates to. It may not declare types or
  traits that form the component's API.
- Absolute `crate::` paths across a component boundary, `super::` only within one.
- `mod.rs` and `common.rs` have no length limit for this task. A limit is set afterwards from what
  they weigh.

### What that buys, exactly

Three claims, and only the first two are the compiler's.

- **A component is reachable only through its `mod.rs`.** Enforced: every implementation module is
  private, so naming one from outside is `E0603: module is private`.
- **A subcomponent is reachable only through its own `mod.rs`, and only from inside its parent
  component.** Enforced by the same mechanism, once the declaration is `mod` rather than `pub mod`.
- **Only the parent component's own code may use a subcomponent.** Enforced *across* components.
  Not enforced *within* one: Rust's rule is "the module and its descendants", and sibling
  subcomponents are descendants of the parent. There is no visibility level meaning "my parent but
  not my siblings" — `pub(super)`, `pub(crate)` and `pub(in path)` all give the same set.

**The layout as placed has no sibling-subcomponent edge left.** A sweep for calls from one
subcomponent into another found exactly one, `translator` into `scan_mapping`, and the answer was
that `scan_mapping` was misplaced rather than that the rule needed an exception. `driver` never
calls a backend — it is generic over `Backend` — and `memory_estimation` takes only types.

So two things fall to the layout test: sibling reach between implementation modules, which is the
same gap one level down, and where a `pub` appears at all, which nothing in rustc checks. Both are
readable from the tree, in the idiom `test_ci_coverage.rs` already uses. Without that test none of
these rules can go red.

## What the visibility sweep finds

170 top-level `pub` items in `src`, plus 142 `pub` methods and associated consts.

- **8** are named by another crate, and all eight by one file — `peacockdb/src/main.rs`, the CLI,
  which is the only workspace member that depends on `peacockdb-core` at all. They are
  `build_session_state`, `register_tables_for`, `plan_batch_partitioned`, `PlanKnobs`,
  `BatchSizing`, `SMALL_TABLE_BYTES`, `CpuBackend` and `batch_partitioned_driver`. That is the
  crate's real API: register tables, plan a query, run it on a backend.
- **108** are `pub` only because `peacockdb-core/tests/*.rs` are separate crates.
- **54** are named by nothing outside the crate and lose `pub` — among them `Batching`, `ColumnRef`,
  `SortOrder`, `UnaryOp`, `Translator`, `MemoryModel`, `JoinCapability`, `Forwarder`,
  `logical_size_from_schema`, `estimate`, `partition`, `translate_expr`. Eleven of the 54 are used
  only inside their own file and lose `pub` entirely: `Decomposition`, `EmittedBatch`,
  `SourceEstimate`, `StateFunc`, `all_row_groups`, `position_of`, `wire_nodes`, and the four
  `config.rs` items that are being deleted.
- 85 `pub(super)` become `pub(crate)` once the enclosing `pub mod` loses its `pub`.
- 30 `pub use` are inlined.

One leak the item count does not show, because it is a module rather than an item:
`pub mod generated` in `lib.rs` exports the whole flatc surface — 7,336 lines, 73 types named
internally and every other one reachable. It becomes `mod generated;` private to `wire/`.

The 108 stay `pub` **in this task**, and [`test-layout.md`](test-layout.md) removes them by moving
the eleven targets that force them down into `src/`. Doing that here would mean one diff in which a
layout mistake and a coverage regression look alike, so it waits — and by then every one of those
imports has already been rewritten, which is most of the work.

## Renames that fall out

- **The node `GpuJoin` becomes `GpuHashJoin`.** It is the equi-join, it serializes to
  `CudfHashJoin`, and it sits beside `GpuCrossJoin` and `GpuNestedLoopJoin` — one of three
  unqualified for no reason. The executors keep the category names `CpuJoin` and `GpuJoin`, which is
  the scheme every sibling follows and is accurate: both run all three join nodes.
  13,246 golden lines carry the old name. It is display text, so the recipe digests do not move;
  sed, then regenerate to confirm the sed rather than to author it.
- **`memory.rs` becomes `common.rs`.** It is a byte formula, not memory management, and it collides
  with `plan_text/memory.rs`, which renders the `--- memory ---` section.
- **`gpu_rowgroup_prune` loses its `gpu_`.** It runs on the CPU and serves both backends.

**`CpuUnload` and `GpuExport` stay as they are.** They are one thing under two names, differing by
backend, which the style guide's "the same thing carries the same name everywhere" reads against —
but neither collides with anything, and any fix costs more than it buys: `GpuUnload` is taken by the
plan node, and renaming `CpuUnload` to match `GpuExport` moves a name nobody is confused by. Noted
as a considered keep so the next reader does not re-derive it.

## Tests

The four tiers already exist and none of them moves.

- **Module unit** — `#[cfg(test)] mod tests { … }` inline in an implementation module. 13 sites,
  unaffected.
- **Component and subcomponent** — `#[cfg(test)] mod tests;` declared beside the implementation
  modules, files under `component/tests/`. 11 sites. Being a descendant of the component they see
  its private items but not an implementation module's, which is the boundary respected.
  `nodes/tests/`, `cpu_backend/tests/`, `driver/tests/`, `translate/tests.rs` and `recipe/tests.rs`
  are already this.
- **Crate integration** — `tests/*.rs`, held to the component API. Eleven files name an
  implementation module today: `cpu_backend::{accumulate,emit,join,source,backend}`,
  `gpu_backend::{accumulate,emit,join,backend}`, `nodes::{aggregate,join}`. Every type they reach is
  legitimately component API, so this is an import rewrite — `cpu_backend::CpuAccumulator` — and no
  test relocates.

`test_cpu_executors` and `test_gpu_executors` construct backend executors directly, so the executor
constructors stay public for tests. That is the executor contract and is fine; list them in the
component `mod.rs` deliberately rather than by accident.

Eleven of the eighteen targets then move down into `src/` in [`test-layout.md`](test-layout.md),
which is also where test code stops sharing a file with production code. Nothing here should be
written to make that harder — in particular, do not fold a test helper into a production module to
shorten an import.

## The wiki this moves

`architecture.md` is not prose about the code — it is prose *anchored to* the code, and the anchors
move. 29 of its lines carry a Rust path or module name: 37 path references in total, some as
markdown links to `../peacockdb-core/src/batch_partitioned/recipe/*.rs`, most as bare
`node_writer.rs` / `join.rs` / `scheduler.rs` in running text. Seven are in the wire-format section
alone, which is also where the largest move lands.

Correcting them is this task's, not a later cleanup's — `prompts.md` makes keeping those two pages
true the same commit's duty as the change. The work is four kinds, and only the first is mechanical.

- **Paths in links and backticks**: rewrite to the new component. `batch_partitioned/recipe/` becomes
  `wire/`, `batch_partitioned/translate/` becomes `planner/translator/`, `batch_partitioned/nodes/`
  becomes `plan/`, and the crate-root files land where the placement table says.
- **Directory names in prose**, which read as facts rather than links: "the recipe writer
  (`batch_partitioned/recipe/`)", "the types are in `batch_partitioned/`", "`driver/`,
  `partitioned.rs` owns the tree". These do not grep the same way as the links and have to be read
  for.
- **Sentences the reorganization falsifies**, which carry no path at all. "A `Gpu` name with no
  `Cudf` is one of this mode's own plan nodes" survives; the Execution section's "the types are in
  `batch_partitioned/` and the code is what they are" needs the new home; and the whole framing of
  the mode as *a* mode rather than *the* engine is the phase-1 rewrite, 185 hits across `llm-wiki`.
- **`coding-style.md` gains the visibility rules** — the section drafted above goes in whole, and
  its Small-files bullet's examples (`batch_partitioned/nodes/`, `batch_partitioned/cpu_backend/`)
  become the new paths. Its Names section loses the `test_inc2_conformance` exception paragraph in
  the next task, not this one.

`build-test.md` carries 19 such lines; correct the paths here and leave its test table alone —
[`test-layout.md`](test-layout.md) restructures it, and doing it twice means doing it wrong once.

## Validation

Nothing here may change what the engine computes, so the bar is the opposite of the previous task's:
**no golden may move at all**, and the one exception is quarantined in its own commit.

### Baselines, before the first move

1. `sha256sum` over `testdata/goldens/`.
2. `cargo test -p peacockdb-core --lib -- --list` and one `--list` per integration target. Tests do
   not move in this task, so every one of these must come back byte-identical at the end.
3. A dump of every `pub` and `pub(crate)` item with its declaring file — the visibility baseline the
   sweep is compared against.
4. Warning counts from clean builds in all three feature shapes.

### The `GpuHashJoin` commit is quarantined

It goes first, alone, and it is the only commit in the task whose diff touches `testdata/goldens/`.
Sed the 13,246 lines, then regenerate and require an empty diff — the regeneration confirms the sed
rather than authoring it. **Plain `UPDATE_CANONICAL=1`, never with `PEACOCK_REWRITE_RECIPE_BYTES`:**
without the second variable `test_plan_goldens` compares the committed payload digests against the
bytes it just built and fails naming the file; with it, it rewrites them, and the digest agrees
with itself having proved nothing. **Every later commit must show zero golden changes in `git diff --stat`,**
and that is the single most valuable check in the task: a golden that moves after this point means
the layout changed behaviour.

### One commit per component

In this order: `plan_text` and `executor/driver` first, because they are already close to the target
shape and prove the pattern cheaply; then `wire`, which is the largest single move and the one that
makes `generated` private; then `plan`; then `planner`; then the backends. After each, run the lib
unit tests plus `test_plan_goldens` — the cheap tier — so a break is localized to the
component that caused it rather than found at the end across a 138-file diff.

### Per-commit checks

- **The case inventory is byte-identical.** Tests do not move here, so any `--list` difference is a
  test that stopped compiling into its target — the most likely silent failure in the whole task.
- **Three builds, not one**: `--features rust-only`, default, and the C++-linked build. The
  rust-only tier boundary is the invariant most easily broken by a move, because pulling one type
  into a shared `mod.rs` or `common.rs` can drag an FFI type into a rust-only path, and it fails at
  link rather than at review. `executor/` is the component to watch: its API is conditionally
  present, so its `common.rs` compiles in every shape while `GpuBatch` does not.
- **The visibility sweep is mechanical, not a reading.** A script enumerates every `pub` and
  `pub(crate)` with its declaring file and asserts: no bare `pub` or `pub mod` outside a `mod.rs`;
  no `pub use`; no `pub(super)`; subcomponents declared `mod`, not `pub mod`; and the item set
  unchanged from the baseline, since this task moves declarations and does not remove any. Run it
  at every component commit.
- **Re-run task 1's strip-and-rematch after each slice.** Its residue gate excludes by line, not
  by match, so a survivor spelling anywhere on a line hides real residue sharing it. That was
  latent when task 1 closed; this task moves the files those 170 lines live in, which is exactly
  the motion that turns it live.
- **Drop the gate's `':!peacockdb-core/src'` exclusion once the directory is gone.** It exists only
  to spare `src/batch_partitioned/`, and this task removes that name. Left in place it hides the
  whole crate: task 1's completeness pass found four residues inside that tree precisely because
  nothing read it, and after this move the exclusion would blind the gate to everything the task
  touched. Run the gate without it, and expect the three mapping sites plus whatever `README.md`
  and `source.py` still say about `ParquetBatchPartitioner`.
- **Every grep in this task takes `--untracked`.** `git grep` does not see untracked files, so a
  sweep run before staging is blind to exactly the files being moved — in task 1 a gate reported
  clean while residue sat in four renamed files. This task moves every file in the crate, so the
  blindness is total until each slice is staged. Run the sweeps after `git add`, or with
  `--untracked`, and never before a move.
- **`git diff -M --summary` reports renames**, not delete-plus-add. A file reported as both changed
  more than half its content, which a path rewrite and an import fix should not do.

### The layout test must be seen red

For each rule it claims — a `pub` outside a `mod.rs`, a `pub mod` subcomponent, a `pub use`, a
sibling implementation module reaching another, a `crate::`-less cross-component path — construct
the violation, watch it fail, revert. A guard nobody has seen fail is a guard nobody knows is wired
up, and `test_ci_coverage.rs` is the worked example of doing this properly in this repo.

### Then the full suite, once

Per `coding-style.md` a behaviour-preserving refactor is verified with a representative case per
mode per binary plus the golden and meta tier; the full corpus runs once, at the end, on verda.
Check what a package-wide command actually sweeps before running it — `--features rust-only` selects
a build, not a tier.

### If a golden moves

It is not a golden to regenerate. It means the move changed behaviour, and the diff names where: a
plan line is the planner, a `--- memory ---` figure is the estimator, a payload digest is the recipe
writer. Bisect by component commit — that is what the one-commit-per-component rule buys.

## Done when

The crate builds clean under all three feature shapes with no new warnings against the recorded
count; every golden after the `GpuHashJoin` commit is untouched; the case inventory is
byte-identical; bare `pub` and `pub mod` appear only in `mod.rs` files, with no `pub use` and no
`pub(super)` anywhere in `src`; the layout test exists and has been seen red on each rule; the
visibility rules are in `coding-style.md` and `architecture.md`'s paths are correct; and CI is green.

## Completeness signoff

Solved under its constraints, with three named shortcuts and no bandaids.

1. "bare `pub` and `pub mod` appear only in `mod.rs` files" is not met as written. Nine `pub mod`
   sit outside `lib.rs` with 60 bare `pub` items behind them, every one forced by a separate test
   crate. Each is registered in `test_module_layout.rs` with the files that force it, checked in
   both directions so an entry outliving its reason goes red, and the whole exemption expires in
   task 3. `pub use` and `pub(super)` are genuinely zero.
2. The case inventory is not byte-identical. `config.rs`'s two unit tests moved to
   `test_golden_format` at net zero, which dismantling `config.rs` authorizes, and the layout test
   this task delivers gained an eleventh case. `TargetPartitions`' label round-trip is gone with
   the type; `MemoryLimit` coverage is preserved. No golden moved after the quarantined rename.
3. The device evidence transfers by argument, not by a run on the head: every `src/` change above
   the GPU-green head is import order, a doc comment moved onto the right struct, and two comment
   lines. The 170 goldens carry recipe payload digests and are byte-identical, so the wire format
   the device consumes did not move.

Two known blind spots are stated where a developer meets them rather than only here: the
cross-component reader misses whitespace before `::`, and `forced_by`'s reverse half misses a
plain module import. The spec's promised follow-up — a length limit for `mod.rs` set from what
they weigh — is not set, and `coding-style.md` defers it with no owner.

---

<!-- archived from llm-wiki/tasks/drop-mode-name.md -->

**Merged 2026-09-10 as PR #141.**

# 1 — the mode has no name any more

Kind: production

First of four. The legacy modes are gone, so "batch partitioned" and its `bp` abbreviation
distinguish nothing; every occurrence is a qualifier against an alternative that no longer exists.

This task changes **names only**. It renames identifiers, mode labels, golden filenames, one Python
module, one ticket page and the prose that carries them. It moves no Rust file and changes no
behaviour, which is what makes its validation absolute: every derived artifact must reproduce byte
for byte.

**`peacockdb-core/src/batch_partitioned/` is the one name that survives**, deliberately.
[`module-layout.md`](module-layout.md) places its contents into components, and doing the flattening
here would move every file twice for one outcome. Task 2 is where the last path goes.

The four tasks in order: this one, [`module-layout.md`](module-layout.md),
[`test-layout.md`](test-layout.md), [`test-support.md`](test-support.md). None may run beside the
four in [`tasks.md`](tasks.md) — rebasing across them is a whole-tree conflict.

## What changes

138 files carry the name in a path; 964 content lines outside the goldens and 169 inside. (The
gate below also lands on 138 — a coincidence, not the same 138.)

`BpMode`, `BP_MODES` and `bp_mode.rs` become `Mode`, `MODES` and `mode.rs`. The lowercase gate
catches the filename but not the two identifiers, and a `BpMode` in a tree with no `bp` anywhere
else is the qualifier-against-nothing this task exists to remove.

**Mode labels lose the prefix**: `bp-tp4-sized` becomes `tp4-sized`. One table owns them, `MODES`
in `tests/common/mode.rs` (`BP_MODES` in `bp_mode.rs` before the rename above), and `ident()` derives the macro spelling by replacing hyphens — so the
table plus a sed over `corpus_cases.inc` covers the Rust side. Then `cost-registry.csv`'s fifteen
`bp_*` column headers, `testdata/fixtures/two-row-registry.csv`'s identical headers, and the
twenty-one sites in `cost-report/src/main.rs`.

**Golden files move rather than regenerate.**

- Only `bp-mini.result.txt` carries a mode inside it — the `mode=` line, 34 sections in tpch and 60
  in tpcds. `.plans.txt`, `.cpu.txt` and `.cost.txt` carry none, so those are a `git mv` and nothing
  else.
- `bp-recipe-payloads.txt` keeps its digests. They are over the flat-buffer bytes and no mode label
  crosses the wire, so a digest that moves here means something other than a rename happened. Do
  not set `PEACOCK_REWRITE_RECIPE_BYTES`.

**Entry points**: `plan_batch_partitioned` becomes `planner::plan` and `batch_partitioned_driver`
becomes `executor::run` in task 2, when the modules they live in acquire those names. Here they
keep their names; renaming a function whose module is about to move is one edit made twice.

**`llm-wiki/tasks/bp-tickets.md` becomes `active-tickets.md`**, staying in `tasks/`. 34 references
in eleven files — `archived-tasks.md` (11), `cost-report/src/main.rs` (6), `build-test.md` (3),
`test_cpu_end_to_end.rs` (2), `tasks.md` (2), `casts.md` (2), and one each in
`peacockdb/src/main.rs`, `tickets.md`, `wire-schema.md`, `refcounted-tables.md`. Its fourteen
`<a id="tNN">` anchors keep their ids, so every `#tNN` link still resolves; only the filename moves.
`cost-report`'s six are load-bearing — the widget resolves ticket links against that path.

**In the archives, update the link paths and leave the prose.** `archived-tasks.md` and
`archived-tickets.md` are a record of what happened; rewriting their sentences to say "the engine"
would falsify the history they exist to hold. Paths must resolve; wording stays.

**The Python prototype renames a module.** `scripts/exec_model/batch_partitioned_driver.py` becomes
`partitioned_driver.py`, and ten files import it by name. Python has no compiler to catch a miss —
the failure is an `ImportError` at collection time, so run the prototype suite before committing.

**Test targets** rename, each to what `build-test.md` already calls its tier:

| was | becomes | the tier it is |
|---|---|---|
| `test_batch_partitioned_injection` | `test_layout_injection` | Layout injection mechanism |
| `test_batch_partitioned_plans` | `test_plan_goldens` | Plan goldens — and it sits beside `test_corpus_goldens` |
| `test_cpu_batch_partitioned` | `test_cpu_end_to_end` | end to end: SQL in, rows out; its macro is already `end_to_end!` |
| `test_cpu_bp_corpus` | `test_cpu_corpus` | |
| `test_gpu_bp_corpus` | `test_gpu_corpus` | |

`test_inc2_conformance` is not renamed here — [`test-layout.md`](test-layout.md) makes it
`test_murmur_conformance` when it moves in-crate, and renaming it twice is one edit made twice.

This reaches CI twice: `.github/workflows/pipeline.yml` and
`test_ci_coverage.rs`, whose exemption table and three GPU target lists name the binaries. That
guard fails on a miss, which is the check. `exec-model-corpus.yml` carries the phrase in a comment;
`pipeline.yml` is not the only workflow to sweep.

**Four traps.**

- **`batch` alone is a domain word.** `BatchSizing`, `Batching`, `batch_rows`, `AggregateBatches`,
  `CudfCoalesceBatches` all stay. Only the two-word phrase goes.
- **The two words are not always adjacent.** `batch_single_partition_driver.py` carries the same
  qualifier with `single_` between them, so a `batch.partition` regex misses it; it becomes
  `single_partition_driver.py`, with its function and class. Left alone it would have been the only
  `batch` qualifier surviving in `scripts/`.
- **`batch→partition` is not the mode name.** Four sites — `gpu_plan.fbs:312` and `:346`,
  `node_session.cpp:220`, `gpu_rowgroup_prune.rs:151` — describe the row-group→batch→partition
  *mapping*, which is a real three-level structure and stays. A regex with `.` between the words
  matches the arrow, so a careless sweep mangles them.
- **In `llm-wiki` the phrase is the mode's name, not a qualifier** — 185 hits, plus roughly ten in
  `cpp/` and `gpu_plan.fbs`. Those sentences want rewriting to say the engine; a mechanical strip
  leaves them ungrammatical and, worse, still wrong.

## Validation

The rename is inert by construction, so the bar is that every derived artifact reproduces exactly.
"It compiles and the tests pass" proves nothing here — the tests would pass over a golden that
quietly changed.

### Baselines, before the first edit

1. `sha256sum` over every file under `testdata/goldens/`, saved outside the tree.
2. `cargo test -p peacockdb-core --lib -- --list` plus one `--list` per integration target, as the
   case-name inventory. 437 lib cases, eighteen targets.
3. The warning count from a clean `cargo build` and `cargo build --features rust-only`.
4. `git rev-parse HEAD`, so a bisect has a floor.

### The checks

- **The golden hashes move in exactly two files.** After the `git mv` and the two seds, the sha256
  list must differ from the baseline only in the two `bp-mini.result.txt` files, and there only on
  `mode=` lines. Any other hash change means the rename touched content it should not have.
- **Then regenerate anyway, and require an empty diff.** `UPDATE_CANONICAL=1` over
  `test_plan_goldens` and the corpus cpu tier rewrites every golden from a live run;
  `git diff` after it must be empty. The hash check says the files did not move; this says the
  engine still produces them.
- **The refusal message is the one exception, and it is quarantined.** `error.rs:20` renders
  `"unsupported in batch-partitioned mode: {what}"` and `translate/mod.rs:221` says "do not plan in
  batch-partitioned mode (#143)" — and those strings land in **75 lines across the ten
  `.plans.txt` goldens**. Reword them in their own commit, last: everything before it must
  regenerate to an empty diff, and that commit's regeneration must produce a diff of exactly those
  75 lines and nothing else. Same shape as task 2's `GpuHashJoin` quarantine, and the same reason —
  it separates the strong check from the one known change. The wording is the developer's, under
  two constraints: it must not say "mode", and the two sites must agree.
- **The payload digests are the sharpest instrument, and the plain regen is what reads them.**
  Under `UPDATE_CANONICAL=1` alone, `test_plan_goldens` compares the committed digests against the
  bytes it just built and fails naming the file. Setting `PEACOCK_REWRITE_RECIPE_BYTES=1` makes it
  rewrite instead of compare, which is the instrument switched off — so never set it here. A
  digest that moves during a rename means the run stops, not that the new digest gets committed.
  (`module-layout.md`'s quarantine reads this same paragraph.)
- **The case inventory maps under one transformation.** Every `--list` name must map to a baseline
  name by removing a `bp_` or `bp-` prefix, and per-target counts must match. A vanished case is a
  `#[test]` lost to a bad sed; a new one is a duplicated module.
- **`cost-report`'s own tests are the registry guard.** They read both `cost-registry.csv` and
  `testdata/fixtures/two-row-registry.csv`, so a header renamed in one file and not the other fails
  there rather than in a later task.
- **The exec-model suite runs before the commit**, not after. Its 216 cases are the only check on
  the Python module rename, and an `ImportError` there is silent until collection.
- **The prose-and-labels gate.** The naive form cannot be empty, because this task deliberately
  keeps `src/batch_partitioned/` and deliberately does not rename `plan_batch_partitioned` or
  `batch_partitioned_driver` — 132 lines outside `peacockdb-core/src` name the module path (128 in
  `peacockdb-core/tests/**`, 4 in `peacockdb/src/main.rs`) and 42 more name those two functions. All
  of it is the residue task 2 removes. So the gate excludes the four spellings that survive:

  ```
  git grep -inE --untracked 'batch.?partition' -- ':!llm-wiki' ':!peacockdb-core/src' \
    | grep -vE 'mod batch_partitioned|batch_partitioned/|batch_partitioned::|::batch_partitioned|plan_batch_partitioned|batch_partitioned_driver'
  ```

  **The pattern is `batch.?partition`, case-insensitive, and both halves of that are load-bearing.**
  `.?` rather than `.` catches `BatchPartitionedDriver`, which a separator-requiring pattern cannot
  see at all — that is how a class survived its own module's rename. And the exclusions are the
  four surviving spellings written out rather than a bare `batch_partitioned`, which would swallow
  every underscore residue including `--test test_batch_partitioned_plans` and
  `mod test_batch_partitioned_injection` — the target-rename miss the gate exists to catch.

  **Run it in a UTF-8 locale.** `→` is three bytes, and under `LC_ALL=C` a `.` matches one byte, so
  the pattern cannot span the arrow and the three mapping sites drop out — the gate reports three
  and reads as cleaner rather than as half-blind. The residue half is unaffected either way, since
  CamelCase needs zero characters and every separator form is one byte, so a C-locale run is safe
  but its count is not the documented one. `build-test.md` sends people to `LC_ALL=C` for
  cross-host comparison, so this is a real way to meet it.

  The goldens are deliberately **not** excluded. They carry no hits once the refusal is reworded,
  so excluding them buys nothing today — and they are exactly where that wording lands, so a gate
  blind to them could not catch it coming back.

  **The exclusion is line-scoped, which is the one hole left in it.** `grep -vE` drops the whole
  line, so a deliberate survivor anywhere on a line shields real residue sharing it —
  `mod batch_partitioned; // renamed from test_batch_partitioned_plans` is invisible to the gate.
  Latent rather than live in what the gate reads: stripping the six survivor spellings from every
  excluded line and re-matching returns nothing there. But 170 lines carry a survivor spelling, and
  the later tasks move the tree those lines live in, so re-run that strip-and-rematch after each
  slice rather than assuming it still holds.

  **The `peacockdb-core/src` exclusion is the larger hole, and it was live.** The pathspec drops the
  mode's own module, so the gate never reads the tree this task is named after, and the completeness
  pass found four hits inside it: `error.rs`'s two `RunError` display strings, `mod.rs`'s module doc,
  and a temp-dir name in `parquet_meta.rs`. The same pattern scoped to `peacockdb-core/src`, with the
  survivor spellings stripped per line, must land on one — `gpu_rowgroup_prune.rs:151`'s mapping
  site. Task 2 removes the directory that forces the exclusion; until then that scoped form is the
  only thing that reads inside it.

  At the finish it lands on **six**, all deliberate: the three `batch→partition` mapping sites,
  `README.md:371` and `source.py:3` naming `ParquetBatchPartitioner` (the same structure, not the
  mode), and `test_ci_coverage.rs:431`, which is task 2 residue and goes when the module does.

  Then `git grep -nE --untracked '\bbp[-_]' -- ':!peacockdb-core/src' ':!llm-wiki'` and
  `git grep -n --untracked 'bp-tickets' -- ':!llm-wiki'`, both empty. `llm-wiki` is excluded whole
  because it is rewritten by hand rather than swept, and two parts of it keep `bp` on purpose: the
  archive's 21 mode labels, which record what the modes were called at the time and would be
  falsified by an edit, and these four task specs, which have to quote both spellings to say what
  becomes what.

  **`--untracked` is not optional, and it is the trap that would have shipped residue.** `git grep`
  does not see untracked files, so every gate is blind to exactly the 42 files this task renames —
  they are `??` until staged. Run without it and the gate goes **green over the renamed files
  themselves**: the first pass here left the phrase in `mode.rs`'s module doc and `mode_named`
  panic, and in two renamed targets' module docs, with gate 1 reporting clean. The same shape bit a
  `git grep -l` sed list, and that one at least went red in the Python suite. This one would not
  have.
- **`test_ci_coverage` passes**, so a missed target rename fails there rather than silently
  un-gating a tier.

### What a device run does and does not add

A renamed golden that `mode.rs` computes a different path for fails **rust-only, before any
device**: `test_corpus_goldens` opens `cpu_golden()`, `cost_golden()` and `result_golden()` by
computed path with `.expect(...)`, and `test_cost_model` sweeps the same directory. So the device
tier is not the first reader of those filenames, and a run that claims to be proving them is
claiming the wrong thing.

Nor is "the sections resolve" the whole of it: `test_corpus_goldens` already checks committed
sections it did not write, against their own arithmetic, with no run at all. What the device run
uniquely adds is those sections checked against **a second engine's actual run** — plan shape,
`in_rows`, the per-batch lists, the bytes. That is the claim that stays true in tasks 2 and 3, and
"only the device can see this" is an easy thing to say about a filename when it is true only of a
comparison between engines.

### Done when

Every golden regenerates to an empty diff and the payload digests are byte-identical; the case
inventory maps by prefix removal with no count changing; `cost-report` and the exec-model suite are
green; the grep gates are empty; and CI is green with no new warnings against the recorded count.

## Completeness signoff

Solved under its constraints. Every derived artifact reproduces byte for byte: the 33 golden
renames differ from their baselines only on the 94 `mode=` lines and the 75 quarantined refusal
lines, the payload digests never moved, the case inventory maps 1:1 under prefix removal, and a
final `UPDATE_CANONICAL=1` regen of the plan goldens and the cpu corpus left an empty diff.

Two bandaids, both deliberate and both named above. The prose gate excludes `peacockdb-core/src`,
which hid four residues until the completeness pass; the scoped form that finds them is written
into the gate section, and task 2 removes the directory that forces the exclusion. And the spec
was not frozen — six commits rewrote it, so "the gate lands where the spec says" is partly
self-fulfilling; each of the six survivors was re-derived independently instead.

---

<!-- archived from llm-wiki/tasks/empty-answers.md -->

**Obsolete — approach rejected, never merged; PR #138 closed 2026-09-10.** Superseded by `empty-build.md`, which showed the capability it proposed is one the engine does not need.

# Answering with nothing: the table no call can build

Kind: production

Two refusals, one wall. A node owes rows it did not receive, and every entry point on the surface
loads a table by reading one — so there is no call to make.

- [#173](../tickets.md#t173) — a collapse of no handles, a merge of no runs, and a finish whose
  probe produced no keys.
- [#175](../tickets.md#t175) — a join whose build side produced no batch, where `Right`, `Full` and
  `RightAnti` owe their probe side. #175 says it itself: *"the same wall as #173, reached from the
  join instead of the accumulator."*

**Both close here.** They are one task because they are one missing capability seen from two nodes,
and separating them would mean building it twice.

**Depends on [`wire-schema.md`](wire-schema.md), and that dependency is the whole reason this is
small.** Both tickets say "unfreezing buys a make-empty-of-schema call and the refusals go", written
when nothing on the wire carried a node's schema. Once `PlanNode.output_schema` is populated, the
call is not needed at all: `execute_node` already holds the node
(`node_session.cpp:190`), so it can answer with an empty table rather than throw. **No new ABI
symbol.** The C++ said so before either of us did — `node_session.cpp:257`:

> *"A collapse of nothing has no schema to answer with: **the node's own `output_schema` is absent on
> a recipe plan**, and concatenating no views gives a table of no columns, which is not a batch
> anything above can read."*

## 1. #173 — an empty table of the declared schema

Three sites refuse, and all three want the same thing:

| site | what owes rows |
|---|---|
| `cpp/src/node_session.cpp:261-266` | a collapse with no input handles |
| `gpu_backend/accumulate.rs:331` | a merge of no runs — "the collapse of nothing under another name" |
| `gpu_backend/join.rs:236` | a finish whose probe was empty, so it has no keys to join against |

In `execute_node`, build the answer from `node->output_schema()`: one `cudf::make_empty_column` per
field (`cudf/column/column_factories.hpp:43`), assembled into a table with the declared names. The
two Rust refusals then stop being refusals — the call they could not make is a call that now
answers.

**The global aggregate is the exception and must stay one.** #173: *"a global aggregate owes its
identity row whatever arrived"* — `count` is 0, not absent. An empty-of-schema answer there would
drop a row DataFusion produces, so the aggregate arm keeps its own path and this task must not
collapse the two into "empty input, empty output".

**The CPU backend already emits nothing in the same places**, deliberately, so the two engines agree.
Check each site against its CPU counterpart as it changes: the point is not that the device stops
refusing, it is that both engines answer the same thing.

## 2. #175 — the probe side, padded or not

`empty_build_answers_nothing` (`nodes/join.rs:563`) is already the right decision in the right place;
what is missing is what to do when it returns false. Its own doc names the split:

> *"what they owe is the probe side, **padded or not**"*

- **`RightAnti` owes the probe side unpadded.** Its output is the probe columns alone, and an empty
  build side makes every probe row unmatched — so the answer *is* the probe batch. **Route it, do
  not call.** No pad, no kernel, no handle beyond the one the driver holds.
- **`Right` and `Full` owe it padded**, with typed NULLs in the build columns. That is the mirror of
  the pad that already exists: `ProjectRole::NullPad { nulls }` with `pad_project` and
  `padded_columns` (`recipe/join.rs:114`, `:261`, `:423`) appends one NULL per **probe** column a
  build-preserving join's projection keeps. This needs the same shape counting **build** columns.

So the recipe gains one role, not a mechanism. Whether that is a second `ProjectRole` variant or a
side on the existing one is the author's call — but the name must say which side is being padded,
since a reader who assumes the existing direction gets a plan that type-checks and pads the wrong
columns.

**Two corpus queries are waiting on it**, both found by T19: `tpch/q21` at `tp4-single`, and
`tpcds/q77`, whose `Right` outer at four lanes gets no build side. q77 is currently out of the
end-to-end list with `tpch/q2` carrying its claim, because writing the CPU pad alone would make the
oracle answer a query the device refuses. **Put q77 back on that list as part of this**, or the
reason it was removed outlives the reason.

## 3. Tests

### Unit, Rust

- **the empty-build decision by type** — `empty_build_answers_nothing` returns true for the six that
  end the lane and false for `Right`, `Full`, `RightAnti`. It exists; assert it names all nine so a
  tenth type cannot be added silently.
- **`RightAnti` routes rather than calls** — a lane with an empty build side and a probe batch emits
  that batch and makes no ABI call. The absence of the call is the assertion, not the rows.
- **the build-side pad counts build columns** — the mirror of
  `the_pad_project_appends_one_null_per_probe_column_the_projection_keeps`, written the same way and
  next to it, so the two directions are read together. A projection keeping no build column pads
  nothing.
- **a pad in the wrong direction is caught** — assert the emitted NULL count against a join whose
  build and probe widths **differ**. With equal widths both directions pass, which is how this ships
  wrong.

### Unit, gtest

- **a collapse of no handles answers an empty table of the declared schema** — column count, names
  and types from `output_schema`, zero rows. The test that pins the fix.
- **the types are the declared ones, not defaults** — a schema with a `Decimal128(15,2)` and a
  string, asserting both come back as themselves. An empty table of the wrong types is the failure
  this cannot afford, since nothing downstream has rows to notice with.
- **a global aggregate with no input still emits its identity row** — the exception, asserted rather
  than assumed, because the natural implementation of everything above deletes it.

### Recipe walk

- **a query with an empty lane runs end to end** — the existing harness with a shape that leaves one
  lane with no build side, driven at `target_partitions` > 1. Today it refuses; this is the first
  test where an empty lane reaches the unload.

## 4. Goldens and the device workflow

| golden | moves | why |
|---|---|---|
| `*.plans.txt` | **yes**, where a Right/Full join gains a pad | a new recipe call renders in the node's recipe line |
| `recipe-payloads.txt` | **yes** | the pad project is a payload |
| `testdata/cost-registry.csv` | **yes** | cells move off #173 and #175 |
| `<mode>-<tier>.cpu.txt`, `.cost.txt` | **yes**, for the queries that newly run | new sections, not changed ones |

Device work, in batches of about five on `shad-gpu` with `build-test-shadgpu.sh`, as T19 does: the
two known sightings first, then any cell whose ticket names #173 or #175. Expect the freed cells to
land on whatever refuses next rather than going green — the causes are ordered, and #152 and #183 sit
in front of most of the corpus. **Close #173 and #175 when their cells are gone from the registry**,
not when the code lands.

## 5. Out of scope

The other frozen-surface refusals. This task buys exactly two capabilities — an empty table of a
declared schema, and a probe side passed through padded or bare — and every other "the surface
cannot express this" stays where it is. If a third refusal looks like it would fall out for free,
that is a ticket, not an addition.

---

<!-- archived from llm-wiki/tasks/wire-schema.md -->

**Obsolete — approach rejected, never merged; PR #137 closed 2026-09-10.** The divergence it removed on the wire is now to be found by the operator harness and fixed at its source, not carried as a per-column width.

# The declared schema on the wire, and the precision the export is never told

Kind: production

**This task closes [#187](active-tickets.md#t187)** — the device widens a decimal the plan declared
narrow — by giving the export the precision it currently defaults to 38 for, rather than by casting
the result back.

Sibling of [`casts.md`](casts.md), which predicts the divergence and deliberately does not fix this
half. **Do that task first**: it writes down what the device returns for a declared type, and this
one changes the answer for decimals.

## Why it happens

cuDF's decimal type carries **scale but not precision**. `export_table_to_ipc` builds its metadata as
`col_meta.push_back({name})` — name only — so `decimals_to_arrow` falls back to
`metadata.precision.value_or(max_precision<__int128_t>())`, which is
`floor(128 · ln2 / ln10) = 38`. Every decimal exports at precision 38 with scale preserved. That is
why `tpch/filter-project` declares `(15,2)` and receives `(38,2)`, and why `q6` never hit it: its
sums declare `Decimal128(38,4)` already.

**This is a missing argument, not two engine rules disagreeing.** #187's current text frames it as
the CPU's `widened_decimal` and the device's concat reaching different verdicts on the same bytes.
That framing is wrong and the ticket should be corrected as it closes.

The Rust side already sends the value: `Field.decimal_precision` exists in `gpu_plan.fbs` and
`serialize_schema` fills it from `Decimal128(p, s)`. It is dropped on the C++ side, after arrival —
`TableResult` (`plan_executor.h:15`) is `table` + `column_names`, and once a column is in one, the
precision exists nowhere.

## The work

### 1. Write `output_schema` on the wire

`PlanNode.output_schema: Schema` already exists in `gpu_plan.fbs` and is documented "Output schema of
this node". The writer never fills it: `fb::PlanNode::create` appears once on this path, in
`Writer::push` (`recipe/writer.rs:97`), with `output_schema: None`. That is the whole change — every
node goes through that funnel, `GpuNode::schema()` is node-local with nothing to derive, and
`serialize_schema` already fills `decimal_precision`/`decimal_scale` from `Decimal128(p, s)`.

Write it for **every** node, not only where a decimal appears: `fb_text.rs`'s own header warns that
"not set" and "set to zero" are different instructions to the executor, and a conditional wire format
is a worse thing to own than a slightly larger buffer.

The C++ then reads it where `TableResult` is built (`plan_executor.h:15`, today `table` +
`column_names` only), carries a per-column precision alongside the name, and
`export_table_to_ipc` sets `column_metadata.precision` instead of leaving it empty. `operators/
union.cpp:35` already reads an `output_schema` for the neighbouring problem — branches landing
different fixed_point scales — so this is an existing mechanism reaching one more node kind rather
than a new one.

With precision on the wire, `export_type_for`'s `Decimal128(p,s) → Decimal128(38,s)` row stops being
true: the divergence is removed rather than absorbed. Update the row in
[`casts.md`](casts.md) and the prediction test that records it — the cast list never gains a decimal
arm, because pinning a fixable omission into a golden would make it look inherent.

**The C++ half was the open risk and it is closed.** The worry was that
`node_session.cpp:517` does `result.column_names = input.column_names` — names propagating from
*inputs* rather than from each node's declared output — which would mean precision had to be
threaded from an origin. It does not. `execute_node` holds the node it is executing:

```cpp
const fb::PlanNode* node = impl_->post_order[seq];   // node_session.cpp:190
```

and every `TableResult` built inside it (`:267` collapse, `:355`/`:399` repartition) is in that
scope, so `node->output_schema()` is directly readable once step 1 populates it. Line `:517` is
`NodeSession::slice_handle`, which has no node because it slices an existing handle — it copies
`input.column_names` from a `TableResult` that already exists, and precision rides along the same
way. **No threading from an origin is required, and no signature changes.**

### 2. `schema_text` renders precision and scale

`fb_text.rs:229` formats fields as `{}:{:?}` over `f.data_type()`, so `recipe-payloads.txt` prints
bare `Decimal128` while expressions on the same page print `Decimal128(23, 2)`. Two fields that are
on the wire are invisible to the golden whose job is to pin the wire — a change to either, including
one that broke step 1, would not move it. Render them for `Decimal128` fields.

### 3. Unit tests

Same idiom as [`casts.md`](casts.md) — build the node, run its recipe fn, `writer.finish()`,
`flatbuffers::root::<fb::GpuPlan>`, `node_at(seq)`, assert on payload fields.

- **precision reaches the payload** — assert `output_schema().fields()[i].decimal_precision()` is
  the declared 15 rather than 0. Reuses the `(name, precision, scale)` schema helper `casts.md`
  adds. **Red before step 1 lands**, which is the order `coding-style.md` asks for.
- **every node carries a schema, not only the ones with decimals** — a node of plain `Int64`
  columns still has `output_schema` set, since a conditional wire format is the thing step 1
  declines to own.

## Restriction

**Code and test changes are limited to what is written above.** No refactor of `node_session.cpp`
beyond carrying one field alongside `column_names`, no generalizing the export metadata past
precision, no cleanup of `TableResult`'s neighbours. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `recipe-payloads.txt` | **bytes** change on every node; text changes on decimal fields | step 1 adds `output_schema` everywhere, step 2 renders precision |
| `*.plans.txt` | **no change** | plan text renders the Rust tree, which already knew the precision |
| `<mode>-<tier>.cpu.txt`, `.cost.txt`, `.result.txt` | **no change** | values and byte pricing are unaffected; only a declared type moves |
| `testdata/cost-registry.csv` | device cells move off #187 | see below |

The payload golden is the one to review rather than accept: its bytes move for every node in every
plan, and step 2 is what makes that diff legible instead of opaque.

## Device workflow

1. Run the **10 queries carrying #187** on `shad-gpu` with `build-test-shadgpu.sh`, in batches of
   about five, as T19 does: `tpcds` q16 q33 q61 q77 q90 q94 q95, `tpch` q2, `filter-project`,
   `hash-join`.
2. For each, **either enable the device cells or update the ticket**. The causes are ordered, so a
   cell that stops failing on #187 lands on whatever refuses next rather than going green — expect
   [#152](../tickets.md#t152). A cell whose cause changed is a ticket edit, not a cell that stays
   where it was.
3. Close #187 only when its cells are gone from the registry, and correct its text as it closes.

---

<!-- archived from llm-wiki/tasks/casts.md -->

**Obsolete — approach rejected, never merged; PR #136 closed 2026-09-10.** Predicting the export type at plan time and casting at the unload builds the divergence into the plan; the operator harness reports it instead.

# Export types: predicted at plan time, carried on the wire

Kind: production

Two device cells fail on the same class of thing — the device hands back a column whose Arrow type
is not the one the plan declared — and each is currently discovered at the boundary rather than
predicted before it. This task writes the prediction down where it can be checked, and closes both.

**This task closes [#183](active-tickets.md#t183)** — the device exports `Utf8` where the sink declares
`Utf8View` — by predicting the export type at plan time and casting the one divergence that is
inherent.

Its sibling [`wire-schema.md`](wire-schema.md) closes [#187](active-tickets.md#t187) with the same
derivation and the C++ half this task deliberately leaves out. Do this one first: it is Rust-only,
and the prediction it writes down is what the other one then makes true for decimals.


## Why they happen

`unload` concatenates the decoded IPC batches against the sink's declared schema
(`gpu_backend.rs:166`), and `concat_batches` requires exact type equality. Neither side is
misbehaving:

- cuDF has exactly one string type. `expr.cpp:74` maps the `Utf8View` tag to `type_id::STRING` and
  `cudf::to_arrow_schema` maps that back to `arrow::utf8()`. The divergence is inherent and the
  cast is the only place to absorb it.
- cuDF's decimal type carries **scale but not precision**. `export_table_to_ipc` builds its metadata
  as `col_meta.push_back({name})` — name only — so `decimals_to_arrow` falls back to
  `metadata.precision.value_or(max_precision<__int128_t>())`, which is 38. Every decimal exports at
  precision 38 with scale preserved. That is why `tpch/filter-project` declares `(15,2)` and receives
  `(38,2)`, and why `q6` never hit it: its sums declare `Decimal128(38,4)` already.

The second is a missing argument, not a disagreement between two engine rules. #187's current text
frames it as the CPU's `widened_decimal` and the device's concat disagreeing about the same bytes;
that framing is wrong and the ticket should be corrected when it is closed.

## The work

### 1. `export_type_for` — the composition, written down once

The round trip Arrow → fb → cuDF → Arrow lives in three files and two languages today, so nobody can
answer "what type will the device hand back for this column?" without reading `expr.cpp`. Make it a
Rust function. It is total over the declared type for every case below.

| declared | fb tag | cuDF | exported | |
|---|---|---|---|---|
| `Boolean`, `Int8`–`Int64`, `UInt8`–`UInt64`, `Float32/64` | direct | direct | same | identity |
| `Utf8` | `Utf8` | `STRING` | `Utf8` | identity |
| `Date32` | `Date32` | `TIMESTAMP_DAYS` | `Date32` | identity, via `to_arrow_schema`'s `default:` arm |
| `Utf8View` | `Utf8View` | `STRING` | `Utf8` | **#183** |
| `LargeUtf8` | `LargeUtf8` | `STRING` | `Utf8` | same shape, no corpus query reaches it |
| `Date64` | `Date64` | `TIMESTAMP_MILLISECONDS` | `Timestamp(ms, None)` | no corpus query declares it |
| `Decimal128(p,s)` | `Decimal128` | `DECIMAL128` | `Decimal128(38,s)` | **#187** — predicted here, **not cast here**; [`wire-schema.md`](wire-schema.md) removes the divergence instead |
| `Null`, `Float16`, `Binary`, `LargeBinary`, `BinaryView` | mapped | **`EMPTY`** | — | **refuse** |

The last row is not a cast. `convert_data_type` serializes all five and `fb_to_type_id` has no case
for any of them, so they reach the device as a typeless column. `export_type_for` returns `Err` and
the plan is refused at planning time, which is what happens to them today only by accident.

### 2. Derive `exports` inside `attach_recipes`

Not a separate pass and not in the recipe writers, which are the wire codec rather than a
planning phase. The recipe walk already delivers the input:

```rust
fn unload(
    _node: &GpuUnload,
    _inputs: &[&Schema],     // the sink's declared schema, already here, unused
    _writer: &mut Writer,
) -> Result<Option<Recipe>, PlanError>
```

`_inputs` is exactly what `export_type_for` needs, handed to the one function already positioned at
the sink. Deriving it there costs no second traversal and creates no second place that has to agree
with the first — the hazard `driver/index.rs:151` exists to guard for node numbering.

The result is a per-column identity-or-cast list, carried to `GpuSink::new`
(`gpu_backend.rs:124`) alongside the schema the driver already passes it.

**The cast at `unload` must be narrow.** `concat_batches` against the declared schema is today the
only check that the device produces what the plan says it produces; it is what surfaced #187. A
blanket cast-to-schema fixes #183 and destroys that. Only the arms the table marks as divergent are
cast; every other mismatch still fails, as `export_table_to_ipc`'s `DECIMAL32/64 → DECIMAL128`
widening is a named normalization rather than a general one.

Two review points, both of which lose a check if missed:

- `self.schema.fields().zip(batch.columns())` truncates silently on a column-count mismatch, which
  `concat_batches` catches today. Check the length explicitly.
- Non-string divergences would fail at `RecordBatch::try_new` rather than at `concat_batches`, so
  `"the exported stream is not the sink's rows: {error}"` moves with them or #187-class failures lose
  the message the tickets quote.

### 3. Unit tests

`recipe/tests.rs` already asserts on the buffer where the recipe cannot answer, with the idiom at
`a_finalize_project_emits_the_group_keys_the_finalize_list_leaves_out`: build the node, run its
recipe fn, `writer.finish()`, `flatbuffers::root::<fb::GpuPlan>`, `node_at(seq)`, assert on payload
fields. No GPU, no session, no plan load.

- **exports at the unload** — pure function of the input schema, so it needs neither the buffer nor
  `finish()`. A `GpuUnload` over `Utf8View` + `Decimal128(15,2)` + `Int64` derives a list naming the
  divergent columns and omitting the identity ones.
- **the `EMPTY` class refuses** — `Binary`, `Null`, `Float16` return `Err` rather than passing
  silently. This is the arm most likely to rot, since no corpus query reaches it.
- **a decimal predicts but does not cast** — `export_type_for` reports `Decimal128(15,2) →
  Decimal128(38,s)` and the unload's cast list omits it, so the prediction is recorded before its
  fix exists. `columns_of` hardcodes `DataType::Int64`, so this needs a sibling taking
  `(name, precision, scale)` — which [`wire-schema.md`](wire-schema.md) reuses.

`recipe/tests.rs` is at 887 lines against the 1000-line cap. These land around 930 — under, but the
next addition to that file forces the split rather than this one.

## Restriction

**Code and test changes are limited to what is written above.** No refactor of the surrounding
writer, no generalizing `export_type_for` beyond the table, no second cast site, no cleanup of
the recipe writers while passing through them. Anything else found on the way is a ticket.

## Goldens, and how each moves

| golden | how it moves | why |
|---|---|---|
| `*.plans.txt` (10 files) | every `GpuUnload` line gains `exports=` | new field; `GpuUnload` renders bare today |
| `<mode>-<tier>.cpu.txt` | **no change** | `memory.rs:43` prices `Utf8View` and `Utf8` identically at `(rows+1)*4`, and content size is Σ value lengths for both |
| `<mode>-<tier>.cost.txt` | **no change** | derived from the `.cpu.txt` sections, which do not move |
| `<tier>.result.txt` | **no change** | values are unaffected; only types were ever in question |
| `testdata/cost-registry.csv` | device cells move off #183/#187 | see the device workflow below |

`exports=` renders on every unload, including `exports=none` where nothing diverges. Omitting the
attribute would make "nothing diverges" and "the list was never computed" identical in the golden,
which is the invisible-absence shape `coding-style.md` records twice.

`exports=` is a Rust-side prediction that the C++ never receives: the unload writes no payload, so
nothing on the wire describes the sink's columns. The golden is documentation and a tripwire, and
the runtime check at `unload` is the only thing that enforces it. [`wire-schema.md`](wire-schema.md)
is what makes it checkable against the buffer.

## Device workflow

The cast and the precision are not proved by a green CPU tier — every cell they exist for is a device
cell that is currently disabled. After the code lands:

1. Run the affected corpus queries on `shad-gpu` with `build-test-shadgpu.sh`, in batches of about
   five, as T19 does. **59 queries carry #183** — every query with a string in its sink schema,
   which is derivable from `tp1-single.plans.txt` without running anything, and was: no query
   without a string in its sink carries #183, over 82 checked, with no exceptions.
2. For each, **either enable the device cells or update the ticket**. A cell that now reaches a
   different cause is a cell whose ticket changes, not a cell that stays where it was — and the
   causes are ordered, so fixing #183 will move cells onto whatever refuses next rather than turning
   them all green. Expect #152 to be the common landing place.
3. Close #183 only when its cells are gone from the registry, not when the code lands. A ticket
   whose cells are still disabled against it is not closed.

---

<!-- archived from llm-wiki/tasks/drop-legacy-modes.md -->

# Drop the legacy execution modes

Delete the six legacy execution modes — CPU full-table at tp1 and tp8, CPU partitioned,
GPU all-at-once, GPU full-table, GPU partitioned — with the planning they rest on, the
tests, the goldens, the benchmark records and the widget columns that describe them. What
stays is the batch-partitioned planner and its two backends.

## What goes

- **Rust**: `executors/` (five mode classes, the node-by-node driver, the streaming
  driver, the resident enforcer), `operators/` (the 16 `Gpu*Exec` wrappers and their
  serializers), `gpu_rule.rs` (both physical optimizer rules), `plan_serializer.rs`,
  `resident.rs`, `cpu_executor.rs`, `gpu_executor.rs`, `node_executor.rs`, and
  `CpuExecutor` in `lib.rs`.
- **C++**: `execute_plan.cpp` and the `peacock_execute` ABI entry point. `execute_node`
  stops being a recursive driver and becomes what it always was on the node path: the
  resolver that hands an operator its next already-resident input.
- **Tests**: the fourteen legacy targets and the harness that only they used
  (`common/exec_mode.rs`, `common/benchmark.rs`, `common/gpu_cases.inc`).
- **Goldens**: every `.plan.txt`, every `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`
  and `plan_bytes.sha256`; `testdata/benchmark-results/` with the harness that wrote it.
- **Widget**: the legacy table, the six legacy registry columns, and the second PR
  comment the four tables needed.

## What is kept, and where it moved

The batch-partitioned side reached into the legacy tree in four places, so each moves
rather than dies: the per-node DataFusion runner (`cpu_backend/single_node.rs`), the three
Arrow-to-wire helpers the recipe writers share (`recipe/wire.rs`), `parquet_table_name`
(`parquet_meta.rs`), and the small-table threshold, which two test files spelled
separately and is now `plan::SMALL_TABLE_BYTES`.

## Coverage this removes and does not replace

Two guards were defined as "the recipe writer against the legacy writer" and cannot
survive it: `every_field_the_legacy_writer_sets_is_set_here_or_declared_a_difference` and
the seven expression cases in `recipe/expr_writer/tests.rs`. What they added over the rest
was a second, independent producer of the same bytes; the payload digest still pins what
this writer writes. Say so in the PR rather than quietly.

The C++ operator gtests keep their coverage: `test_plan_executor.cpp` drives its
hand-built plans through `NodeSession` node by node, the way the driver does.

## Verification bar

CPU: the rust-only suite and `ctest -L cpu` locally, `cargo test -p cost-report`.
GPU: the five staged targets on shad-gpu. Both green before the PR.

**Merged 2026-09-08 as PR #140**, three commits against master. What the review added beyond
the spec: thirteen loose root files had ridden into the deletion commit and the repo root is
now ignored by extension; the free `execute_node` became `take_input`, since it executes
nothing and shared a name with the one that does; the CLI was rewritten onto the planner and
nothing compiled it, so CI builds it and `test_ci_coverage` guards that. The GPU job's rust
loop was found printing nothing and guarding nothing — its `cat` and `grep` segfaulted under
an exported glibc — which is where the per-command library path, the crash detector and the
single-GPU concurrency group came from.

---

<!-- archived from llm-wiki/tasks/batch_partitioned_executor.md -->

# batch-partitioned executor: the implementation plan (T0–T22)

**Closed 2026-09-08, and archived whole.** Every task here is done except two: T20 became
[#195](../tickets.md#t195) and T22 is obsolete as written (see its entry). It is kept verbatim
because commits, reviews and the archived specs below name these numbers, and this is where a
reader resolves them; what the tasks built is described in
[`architecture.md`](../architecture.md).

Tasks in dependency order, and the numbers now ascend with it. T13 is the one that does not:
it landed early, because both drivers over a mock backend needed none of T9–T12, and it keeps
its number because commits and reviews already name it. T21 sits out of order for the same
reason — it was split off T14 after it had been narrowed — and the tail of the list runs
T20, T22 because 21 is spent. T11 and T12 were retired in the same
renumbering — their work is T15 and T16 — so a number is never reused and an older reference
still resolves. Each task is one developer hand-off with its own proving tests.
Legacy tests stay green throughout — every task that touches shared code runs the
affected legacy subsets (one query per mode/tier per binary plus the rust-only tier, per
build-test.md).

~~**T0 — Python prototype of the whole execution model**~~ (done). All node types and both drivers
in Python, operators built with pandas, plans hand-built (no DataFusion, no planner) — an
emulation of tree execution whose purpose is to settle the push model before any Rust
exists. Lives in [`scripts/exec_model/`](../../scripts/exec_model/README.md); its tests run
in CI (cost-report, plus the TPC-H set in dataset-matrix, which has the generated sf1).

Done — struck through, and folded into this document where it changed a decision:

- ~~the trait set, both drivers, and the memory enforcer with the accounting formula~~;
- ~~the scheduling rule~~ — height, order, min-height-first with leftmost ties, every lane
  of the chosen node; the Drivers section is rewritten from it;
- ~~the backpressure rules~~ — a join in its build phase holds its whole probe subtree; a
  satisfied limit holds its whole subtree for good. Both were findings, not designs;
- ~~queues need no cap~~ and ~~`Pending` does not exist~~ — the draft's two flow-control
  mechanisms, both dropped, both because runnability is a predicate evaluated before the
  call;
- ~~pandas-backed operators~~ — filter, project, sort, the aggregate sequence with its
  partial/final decomposition, the accumulators, the hash scatter, the join capability
  matrix, and the T2 row-group partitioning policy, each written against the pandas/cuDF
  intersection with the divergences named;
- ~~every query checked against a single-shot oracle at five partitioning configs~~, the
  prototype's version of two-engine correctness;
- ~~both limit lowerings~~, as the limit rule now states them. One finding survives here:
  the tests must assert on the *calls*, since only those distinguish a limit from a filter
  applied after the transfer;
- ~~the stress surface~~ — a plan rewriter (`operators/injection.py`) rather than
  hand-written variants: one plan re-run at every partitioning, batch size, empty-lane and
  hash-placement preset, with `GpuCoalesceBatches[target]` injected above every source
  (#139's node, proving the drivers tolerate it anywhere) and sources emitting zero-row
  batches at a set probability. It carries one rule the planner's tests should quote: a
  join may be re-partitioned only when both sides are hash-partitioned on the join keys,
  since otherwise its lane count is load-bearing and splitting it joins matching slices;
- ~~empty partitions, empty batches, skewed hashes, the flow-and-backpressure surface,
  determinism (two runs, identical batch traces)~~;
- ~~validation scope~~ — partitioning and `SingleBatch` constraints in scope, schema checks
  not.

- ~~the hand-built plan corpus~~ — 22 TPC-H and 71 TPC-DS query texts rather than the
  3–4 and ~10 the plan asked for, each at three layouts and on both join backends. It was
  the piece most likely to find something and it did: [what the corpus rollout
  measured](#what-the-corpus-rollout-measured) is the section it produced, and every item
  there is a property of the design rather than of the prototype.

Closed without the **estimator** (`estimated_max_resident_size`, `target_batch_bytes`).
The prototype models scratch per executor and never derived batch sizes from a budget,
and T6 derives both in Rust directly — a prototype estimator would be a second model to
keep true against the one that ships. The corpus is what T6 will calibrate against.

~~**T1 — flatbuffer operation-name refactor**~~ (done). Nine of the fifteen legacy node-kind names
(`GpuFilter`, `GpuProject`, `GpuSort`, `GpuAggregate`, `GpuCrossJoin`,
`GpuNestedLoopJoin`, `GpuUnion`, `GpuLimit`, `GpuCoalesceBatches`) collide with the new
mode's node names. Rename the fbs tables and `PlanNodeKind` variants to a `Cudf` prefix
(`CudfScan`, `CudfFilter`, …) so the two vocabularies are visually distinct everywhere —
schema, generated code, the C++ `node_type()` switches and serializer identifiers on the
Rust side. A pure rename: FlatBuffers wire bytes carry no table names and enum ordinals
do not move, so the proof is `plan_bytes.sha256` staying byte-identical with no
regeneration, plus green legacy subsets. The same commit sweeps the llm-wiki references
(architecture.md's fb names, affected tickets, and the recipe-plan table in this spec).

Landed on master as PR #120, with `plan_bytes.sha256` byte-identical and no golden
regenerated, which is the proof the rename asked for.

~~**T2 — ParquetBatchPartitioner.**~~ The pure policy class and its unit tests: fewer
survivors than N; N=3; single row group over target; batching off ⇒ one batch per chunk;
empty survivors (explicit error — the fbs "empty map means legacy single partition"
convention must not leak in); the balance bound on uniform row groups (max−min partition
rows ≤ one row group);
fixed-output determinism case. No planner integration yet.

~~**T3 — node and trait skeleton.**~~ `GpuNode`, `PartitionLayout` (with the two-valued
`SortOrder`), `Schema` with semantics annotations, `Batch`/`CpuBatch`/`GpuBatch` shells
with the move/`!Clone`/`Drop` rules, executor trait definitions with `CallStats`,
`Backend`. Traits in their own files per coding-style. Compiles under rust-only
with the GPU side gated. Unit tests: `SortOrder` canonicalization, layout equality.
**First, before anything else in this task**, compile a skeleton: the `Backend` trait with
all seven associated types, two impls whose `Batch` types differ, `NodeExecutors<B>`, and a
generic function driving one build→probe→finish transition and one source step. It
compiles with no `dyn` anywhere (verified), and it is what pins the static-dispatch
property the GPU path depends on — the mock backend the driver tests need is then a third
impl, not a special case.

~~**T4 — translation layer, single-partition shapes.**~~ DataFusion physical plan (tp1) →
`GpuNode` tree for chains: load, filter, project, sort (+fetch), limit (root-adjacent ⇒
no node, `skip`/`fetch` set on `GpuUnload`; otherwise a `GpuLimit` node over a
planner-inserted `GpuMergePartitions` — never a coalesce), coalesce-all,
single/final aggregates, cross/nested-loop joins. Per-node-kind conscious mapping;
unrecognized node ⇒ plan-time error naming it; window ⇒ the #143 refusal. Unit tests
assert emitted constructs for simple queries.

~~**T5 — translation layer, partitioned shapes.**~~ tp4: shuffle points → Merge+Emit, the
aggregate sequence with its shortcuts and the gid rule, join side normalization (type
remap + column-order-restoring project) and build-side coalesce insertion per the
capability matrix, union/interleave with explicit branch-cast projects. The
`hashKeys ⊆ group columns` structure is produced here (validated in T8). Unit tests per
construct in tp1 and tp4, including side-swap cases.

~~**T6 — estimator pass and plan goldens.**~~ `estimated_max_resident_size` per node
(rows × width vocabulary, N-lane charging), `target_batch_bytes` derivation feeding T2's
partitioner, integration as `plan_batch_partitioned()`. Canonize all four
`<mode>.plans.txt`, memory sections included, for TPC-H and TPC-DS (minus #23's four and
window queries, which appear as refusals).

~~**T7 — schema registry.**~~ (done). The `Schema` carried in `NodeKind` populated on all
nodes, with column semantics annotations. Unit tests: hand-crafted plans produce expected
types and annotations; decimal precision/scale fidelity through project/aggregate/union-cast
paths.

The type and the annotations landed with T3-T6; the tests are PR #126. They assert on the tree
rather than on rendered text, since both engines derive their per-node bytes from the same
declared schema and a wrong type moves no golden byte — `agg_state` at the init, the per-lane
merge and the finalizing merge, and `avg`'s state columns typed by what they hold rather than
by position, which is the case the task existed for.

~~**T8 — validation.**~~ (done). `validate_schemas_and_partitions()` on every node type: partition
topology, key-distribution subset rule, sortedness requirements (merge requires
`BatchSorted`; a limit after a sort requires its input to be `is_stream_sorted()`, checked
on whichever node carries the interval — the `GpuLimit` mid-plan, the `GpuUnload`
root-adjacent), `SingleBatch`
expectations (join build, cross/nlj inputs), captured-index checks. Unit tests: manually
constructed wrong combinations error, right ones pass; then run validation over every
canonized corpus plan from T6.

Node-local validation landed with T4/T5 and is called from `plan_batch_partitioned`; the
generic pass is `batch_partitioned/validate.rs`, PR #126. It runs over every canonized plan
because the planner calls it, so a rejection renders as a `refused:` section and fails both the
golden compare and the registry cross-check. Three planner defects were found this way and
fixed there rather than ticketed: `NestedLoopJoinExec`'s dropped projection, `GpuJoin` minting
a key distribution instead of carrying one, and a non-exhaustive match that dropped the claim a
mark join earns.

~~**T13 — drivers and enforcer.**~~ (done). Both drivers over a mock `Backend` impl — the
third instantiation, alongside CPU and GPU — with the schedule and the accountant as units of
their own, and the accounting formula with its pre/post checks. What the task settled is in
[Drivers](#drivers) and [Memory accounting](#memory-accounting); what it left for T14 is every
real executor, since nothing here computes a row.

~~**T9 — additive ABI.**~~ (done). The three approved symbols in `gpu_executor.cpp` + `peacock_gpu.h`,
signatures as [GPU execution](#gpu-execution-through-the-frozen-ffi) gives them; any
*further* surface change goes through a proposal to the human, per the constraint section.
Rust bindings for all three; `GpuBatch` handle plumbing (session ref, `Drop` release,
`ManuallyDrop` consume boundary). Tests: a C++ gtest in the plan-executor suite reading
disjoint row-group subsets and asserting union == whole-scan; a gtest exporting ranges of
one handle and asserting the concatenation equals the whole, plus the empty range and the
past-the-end range; the same for slicing, plus that the input handle is released and
double-slicing it fails; Rust FFI smoke on shad-gpu. The range plumbing reaches
`UnloadExecutor::unload(batch, rows)`, so the trait's second argument lands here rather
than in T10.

Landed with two shapes worth knowing. The row range is one function, `clamp_row_range`, that
the export and the slice share, so the two cannot disagree about an overrun; and the row-group
override reaches `execute_scan` as a `cudf::host_span`, so the node's own vector and a caller's
array take one path. `RowRange` and `unload(batch, rows)` were already in from T13, so the
trait needed nothing. One test moved tier against the list above: two IPC streams do not
concatenate, so "the ranges are the whole" is asserted in `test_gpu_abi`, where arrow-rs decodes
them, and the gtest holds the contract edges instead.

~~**T14 — recipe-plan serialization.**~~ The `GpuNode` → fb-seq mapping implemented, canonized and
unit-tested. `attach_recipes()` runs after the plan is complete and hangs a recipe on each node
that drives the GPU ABI; a node that makes no ABI call gets none, which is a fact about the node
and so is worth reading off the plan. One function per node kind produces that node's recipe from
**that node alone** — no child, no parent, no tree walk. The mapping table is a per-node statement
and a function that can reach a child would let it stop being one, so the restriction is the
design rather than an economy.

A recipe is a sequence of ABI calls, each carrying the built FlatBuffers node it addresses — the
payload, not a reference to where the fields live. Two renderings, one function taking an enum:
without payloads it is a section in every `<mode>.plans.txt`, between the plan tree and
`--- memory ---`, keeping the tree shape and repeating nothing the tree already shows except the
lane count; with payloads it is a golden of its own, holding the recipes alone — no plan tree, no
memory — for a subset of queries chosen to reach every fb kind and every call shape longer than
one call. A digest of the serialized bytes rides beside the payload text, since text and bytes can
disagree and `plan_bytes.sha256` is the precedent for pinning the wire form rather than a
description of it. Unit tests cover the kinds whose recipe is more than one call, `GpuJoin` first:
the seq set and call pattern per join type, against the
[capability matrix](#join-capability-matrix).

Dense seqs are impossible, and that is a property of the fbs rather than a choice. Children are
nested (`input`, `left`/`right`, `inputs`) and `CudfScan` is the only leaf table, so a set of
addressed nodes whose arities exceed its own edge count has to be padded with stub scans — and
every stub takes a post-order slot, so it moves the seqs above it. Three rules follow: stubs
rather than a re-hung child, since a shared offset is a DAG and gets indexed twice; a call whose
input is a runtime handle hangs off the previous fb node of its own recipe, or a stub where there
is none; and a forwarder's unconsumed branch is gathered by a structural `CudfUnion`, because an
orphan is never indexed and its shift has no visible cause. The pass and the serializer are
therefore one walk: a seq is the post-order index of what was built, so it cannot be counted
before the building.

The `Expr` -> `fb::Expr` writer is its own file and is where the unit tests concentrate: every
expression variant and operator, nesting, and each scalar kind the corpus produces — decimals
with their precision and scale first, since a wrong write there is invisible in plan text and
wrong on a device. `plan_serializer.rs` serializes a DataFusion plan and keeps that one job;
`serialize_scalar_value` and `serialize_schema` are reused, `serialize_expr` cannot be, since it
downcasts `PhysicalExpr` and this IR is our own.

Post-order is the agreement to assert rather than assume: `begin_plan` indexes post-order, so the
emitted tree has to number exactly as the recipes say, and a plan simple enough to check by its
answer would answer correctly while addressing the wrong node.

Nothing executes here and nothing new runs on a GPU. `scripts/exec_model/operators/recipe.py` and
`recipe_join.py` are the starting point for the join sequences — a model, not a spec, and the fbs
and `cpp/src/operators/join.cpp` are what settle a disagreement.

The proving set is the new unit tests and the golden target, and no legacy subsets — the human
scoped it that way because the change is additive: a recipe is attached to a plan nothing reads
yet, so the only tests whose result can move are the ones that read it.

~~**T21 — a recipe plan on a live GPU, driven by hand.**~~ It needs T14 and nothing else: no driver,
no executors, no scheduling. A new test file on the shad-gpu tier plans a query over TPC-H sf1,
loads the recipe plan T14 already built, and makes exactly the calls the recipes name, threading
each call's output handle into the next one's input and exporting at the root. One helper does the
whole walk; one test per query calls it, so a failure names the query rather than a stage.

`begin_plan`'s `out_node_count` is asserted equal to the number of fb nodes the writer created —
not to the `GpuNode` count, which is a different number: stubs, structural unions and any node
with more than one call all separate the two, in both directions and in most plans. It is the
first thing this task can settle that nothing before it can: every seq indexes a post-order the
C++ builds in `index_post_order`, and until a device has parsed a buffer we wrote, our agreement
with that walk rests on two child-order functions having been read side by side. One assertion, at
the first call, in the first place both numbers exist at once.

Shapes, chosen so each call is unambiguous. Everything but the aggregates plans one partition and
one batch, which makes every recipe a single call per node and the walk a straight line: a bare
scan, a filter, a project over a filter, and the joins — inner, and one build-preserving type,
whose single probe batch takes the legacy one-call form. The aggregates plan one batch and **two**
partitions, because a merge is the operator this mode adds and one partition never performs one:
two lanes each merge their own state, the cross-lane merge folds them, and the finalize project
runs once. That is the first time `AggregateMode::Merge` and the finalize expression meet a
device.

`avg` is the case worth a test of its own. Its finalize divides a decimal by a count, and cuDF
derives a divide's result scale from its operands where arrow takes it from the declared output
type — so a wrong cast is invisible on a CPU host and wrong on a GPU, in a column whose type reads
correctly either way. Assert the digits, not the type.

The oracle is DataFusion on the same SQL — `data_fusion_exact`, the CPU tier's own vocabulary —
and deliberately neither a result golden nor our CPU executor. A golden records what the first run
produced, so a finalize whose scale is wrong from the start is pinned rather than caught; and our
CPU executor evaluates the same finalize expression the device is sent, so it agrees with a wrong
one. DataFusion computes `avg` without a Welford triple, a merge mode or cuDF's divide-scale rule,
which is what makes agreement with it evidence. Joins compare as sorted multisets, since a GPU
join's output order is not deterministic. What it deliberately leaves out is everything the
driver decides — batching, backpressure, arrival order — since every shape here is one batch;
those arrive with the executors, and the driven end-to-end over every layout is T17's.

What has a device behind it. Ten fb kinds have run on one; every other is refused by name in a
`match` over every `FbKind`, so a variant added later stops the file compiling rather than going
quietly unclassified. Still unproven on hardware: [#136](../tickets.md#t136)'s finish pass — probe
keys per batch, the concat at done, the finish join, the pad project — the whole Right family,
cross and nested-loop joins, both sort nodes, `slice_handle` and a ranged export.

~~**T10, T15 and T16 — the executors, as one task.**~~ All three land together on one branch, because
they are one question asked of three node families: what does an executor do when the recipe
already says which calls to make. Ordered inside the task as T10 then T15 then T16, since the
accumulators and the joins are the Exec executors' shapes with state added.

~~**T10 — Exec executors.**~~ Filter, project, per-batch sort, aggregate (partial/single), unload
(`GpuBatch → CpuBatch`, honouring the row range). The **GPU executor runs the recipe attached to
its node** — the calls, in order, with the handles threaded — and reuses no legacy operator code:
the recipe is the instruction set, and reaching into legacy operator internals would be a second
path to the same kernels. The **CPU executor relays to DataFusion**, where reuse with legacy is
expected rather than avoided, since both are asking DataFusion for the same operator.

~~**T15 — accumulators.**~~ `GpuCoalesceAllBatches`, `GpuAggregateBatches` (merge-only and finalizing),
`GpuAccumulateBatchesAndSort`, `GpuMergeSortedPartitions`, and the mid-plan `GpuLimit`. Edge cases:
zero batches, one batch, ties for the merge (partition-major stability), fetch interaction, large
batch counts, gid-carrying aggregate merges.

~~**T16 — partition ops and joins.**~~ `GpuEmitPartitions` (per-batch scatter at a small N and a
large one, empty outputs for skewed hashes, and the lane each key lands in, asserted on both
backends — co-partitioning is what every partitioned join rests on). `GpuMergePartitions` is not
here: its mapping is `Forwarder`'s, from T13, and its service order is the driver's. `GpuJoin` with
`set_build`/`probe_and_fetch`/`finish_and_fetch`, plus cross and nested-loop joins on the same
trait. The [capability matrix](#join-capability-matrix) is emulated as a test table — per
(type × layout): stream-vs-refuse, correctness against a hand-built oracle, the GPU finish pass via
key accumulation ([#136](../tickets.md#t136)), `null_equals_null` on the finish join. That finish
pass is the one shape this mode invented with no device behind it after T21, which is why the
matrix is emulated here rather than assumed.

**What a copy costs decides the matrix, and the copy does not exist yet.** Every handle is erased
by its reader, and the frozen surface has no copy symbol, so a shape whose recipe names
`BuildSideCopy` meets a second probe batch with a dead handle. The question T16 had to settle was
whether to keep claiming those shapes stream and refuse until [#145](../tickets.md#t145), or make
a single-batch probe the matrix's permanent rule. Over the 37 hash joins in the
partitioned-tp8-standard goldens a copy would cost 0.08 of the probe stream at the median and more
than it for 12, so a permanent single-batch rule would price every join at the worst one: the
claims stand and the device refuses, naming [#152](../tickets.md#t152), with a test on the
refusal.

Left and Full outer go further and have **no device path at all** until then, which is #152's
second row rather than its first: their key project and their per-call join read the same probe
batch, so no ordering of the two leaves both an input. The finish pass's pad is therefore proved
on the CPU alone, and the device test asserts the refusal.

**How everything here is tested.** Small synthetic data, never the corpus; plans hand-constructed
rather than planned, so a test names the shape it means instead of hoping a query produces it;
`attach_recipes()` is fair game, since the recipe is what a GPU executor consumes. The oracle is
hand-constructed too: an expected result written down, not derived by the code under test. CPU and
GPU tests in separate targets so CI hosts split them. A device test writes its own parquet, which
is the ABI's doing rather than an exception: the four entry points load a table only by reading
one, so a device test's input is a scan or nothing. What the rule excludes is tpch.minimal and the
generated sf1, whose values nobody chose.

**What this task does not do.** No driver: nothing here is hooked into the schedule, and every
assertion is about one executor answering one call. That defers the whole class of claims that
read as call counts and pull counts — a limit holding nothing whatever the offset, at most two
batches sliced per query, the scan stopping — to T17, which is where a driver exists to make them.
`PlaceholderRowExec` ([#158](../tickets.md#t158)) waits for the same reason: it is a source, and a
source proves itself by what the driver pulls from it. T17 then found it cannot be discharged at
all while the surface is frozen — see the sixth entry under
[What the frozen surface costs](#what-the-frozen-surface-costs-and-what-unfreezing-would-buy).

~~**T17 — the whole path, under injection.**~~ The first task in which SQL goes in and rows come out:
planning, the recipes, the executors and both drivers running together, rather than each proved
against a fixture of the last one's shape. Every test starts from a query's text and ends at its
results, so what is under test is the join between the pieces — which is the only part four tasks
of separate proofs cannot reach.

The oracle is DataFusion on the same SQL. Not the legacy CPU executor, as this entry said before
T21: a second engine of our own agrees with us wherever we are consistently wrong, and by the time
this task runs, the finalize expression it evaluates is the one we also send to the device. The
one independent implementation in reach is the one that decomposed the aggregate differently.

Queries chosen to be interesting rather than representative, over the sf1 corpus text, and
between them covering the [join capability matrix](#join-capability-matrix): every join type this
mode claims, crossed with the layouts that make each one stream or refuse. The matrix is emulated
on synthetic data in the executors task, where each type is one executor answering one call; here
it is planned from SQL and run through the drivers, which is the first time a type's claim is
tested as the thing a user gets rather than as the thing an operator returns.

Four shapes are not join cells, so no join cover reaches them, and each is named by the query that
carries it:

| Shape | Query | Why that one |
|---|---|---|
| union lowered to an interleave | tpcds q33, q56, q60 or q66 | the claim is output lane p from lane p of each branch, so it needs four lanes; q14 also interleaves and is the trap, since its is `lanes=1` |
| union that cannot interleave | tpcds q77 | its branches disagree on lane count — 4+1+4, and the golden says `lanes=9` — which is the case [Node set](#node-set) argues in prose and nothing executes |
| both row-interval lowerings | tpch nested-limits | the root-adjacent interval becomes `GpuUnload`'s skip/fetch and the mid-plan one a `GpuLimit` over the scan; the only `OFFSET`s in either corpus, and it has no `.cpu.txt`, so this is its first execution |
| a merge with state worth merging | tpch shuffle-stddev | `GpuAggregateBatches` rides in most of the join queries as a sum; this is the Welford init, both merges and the finalize project |

Nested-loop Left is the one matrix cell no corpus query reaches, and its shape — a single-batch
probe, since #136's finish trick accumulates keys and a predicate join has none — is reachable
from no other row, so this task writes the query. The other uncovered cells stand: an Inner with
`null_equals_null` (the flag rides an INTERSECT lowering, and every corpus INTERSECT lands as a
semi form) and the three plan-time refusals, which a corpus query cannot provoke by construction.

Each query is re-run under injection, several modes rather than one, with the same answer demanded
every time. The prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) is
where to look for modes worth having — layouts re-planned rather than edited, a rebatcher above
every source, sources emitting zero-row batches at a set probability — and it is a model rather
than a specification, so a mode it lacks and this path needs is a mode to add. Rebuild rather than
edit, for the reason the prototype records: a node's partitioning is not a field, so a rewrite
re-runs the planner at a chosen `(target_partitions, batching, small_table_bytes)` and the shapes
come out consistent.

Two rules the injector carries and this one must too: a join may be re-partitioned only when both
sides are hash-partitioned on the join keys, since otherwise its lane count is load-bearing and
splitting it joins matching slices; and a degenerate hash — every key into one lane — is a legal
hash, because a shuffle's contract is co-location and nothing above it may depend on how evenly
the lanes were loaded.

It also inherits what the executors task could not assert without a driver: a limit holding
nothing whatever the offset, at most two batches sliced per query, the scan stopping — each a call
or pull count — and `PlaceholderRowExec` ([#158](../tickets.md#t158)), which is a source and so
proves itself by what a driver pulls from it.

`PlanIndex` gets the unit tests it has never had, and they belong here because this is the first
task whose failures would be read through it. Nothing tests it directly today: `PlanIndex::build`
has one caller, and the scheduler tests derive their own subtree ranges from a parents array rather
than taking the index's. Assert what the derivation decides rather than what a plan happens to
produce — pre-order numbering and the contiguous subtree range that every hold rests on, `parent`
and the snapshotted children, and the three counts a category changes: `ready_lanes` against
`lanes` for a cross-lane accumulator and an emitter, `input_lanes` for the `Done` events a
partition accumulator owes, and `slot_base` where it is lane-scoped against where it is not. Each
of those is wrong far from where it shows.

**Two numberings meet in the driver, and one walk should compute both.** `PlanIndex` is
pre-order — a subtree is a contiguous range, which is what every hold rests on — and a recipe is
keyed by post-order, because that is how the FFI addresses a node. So the index records each
node's post-order position beside its pre-order one, from the walk it already makes, and
`executors_for` takes it. What must not happen is a third derivation: `attach_recipes` numbers at
plan time and the index numbers at run time, so a test asserts the two agree over the corpus —
[#134](../tickets.md#t134) is the same pair one boundary over, and it is unchecked there.

A source executor is this task's to write, and so is the answer to a lane with no build batch: a
`set_build` that never happens because `GpuCoalesceAllBatches` emitted nothing is a driver
decision, not an executor one, and T16 left it here deliberately (the finish's own zero-key answer
is already settled).

The row range the driver hands an unload is asserted before the call. `clamp_row_range` absorbs
an offset past the end and a length past it, because a C ABI has to be total — but
`RowInterval::range_of` cannot produce either, so the tolerance can only be reached by a driver
whose `rows_seen` has drifted, and what that looks like is a `LIMIT` quietly returning short.
Assert non-empty and within the batch where the driver builds the range, so the arithmetic names
itself rather than being absorbed.

The mock backend gets a handful of its own for the same reason one level up. Every assertion in
`driver/tests/` is measured against it, so a mock that miscounts is 1255 lines of tests agreeing
with the wrong answer and staying green. Pin what a script says against what the mock does — the
scripted batch counts and sizes per source and lane, the skew pattern an emitter fills its lanes
by, and an accumulator emitting where the script says it emits. A few cases, not a suite: what is
being checked is that the instrument reads what it was set to.

It starts by wiring the executors to their traits. `Backend` names seven associated types, so no
earlier task can implement it — T16 finishes the last of them and none of them owns a source — and
`Executor`'s `resident_bytes`/`scratch_bytes` are the memory accounting this task adds. So the
executors arrive as inherent methods in the trait's shapes, and the first commit here is the one
that makes the compiler check that.

**Defects found here are fixed here.** Every task before this one proved its own layer against a
fixture; the first thing to run all of them at once will find things about their joins, and
parking those behind tickets would leave the path unproven in exactly the way this task exists to
end. T21 is the precedent: it was meant to be one test file and it found four defects, each a
rule held by a doc comment with nothing reading it, and one guard that could not go red.

~~**T17a — layout injection over corpus queries.**~~ (done). T17 runs seventeen queries at five modes and
calls it injection; it is not. Each of the five is a plan the planner would have chosen anyway —
`(target_partitions, sizing)` re-planned, with `small_table_bytes` and the budget constant. The
prototype's [`LayoutInjector`](../../scripts/exec_model/operators/injection.py) does a different
thing: it takes one plan and rewrites it into layouts no planner would emit — lanes deliberately
drained, a degenerate hash, a rebatcher above every source cutting against the grain, sources
emitting zero-row batches at a probability. Four dimensions, none of them reachable from SQL, and
none of them exercised by any real query today. They live in `driver/tests/stress.rs` over a mock
whose answer is a script.

**Why T17 could not do it, and what this task has to build.** `GpuNode` exposes `children()` and
nothing that reconstructs a node — no `with_children`, no `rebuild` — which is why T17 re-planned
instead. But the rewrite is writable without touching production: `as_node_ref` is a public
exhaustive `NodeRef` over all eighteen kinds, each node's steering fields are `pub`, and each kind
has a public constructor. So a test-side `rebuild(node, new_children)` is eighteen arms that read
the fields and call the constructor, and because the match is exhaustive a nineteenth kind fails to
compile rather than being silently un-rewritten — the same guard `node_kind()` and `driven` already
rest on.

**Prove the rewrite before using it, and a unit test is the right size.** A rebuild that drops a
field is a plan that differs from the one under test for a reason nobody chose, and every result
after it is then about a different query. The case is small: hand-built plans rebuilt with their own
children, identical in **debug output** — not in the rendered plan. The renderer is what a golden
reads and it does not print everything: a loader's survivors and `can_be_null`, and an aggregate's
intermediate schema, reach no plan line, so a rebuild that drops `can_be_null` renders identically
and passes. Those are exactly the fields a corpus plan never varies, which is what would have made
the rendering-only form a guard that cannot go red. Debug prints every field including the private
ones and is the identity. Nothing in this case reads a committed file: the comparison is a plan
against its own rebuild, so it holds without goldens and moves when neither of them does.

**Make the field cover exhaustive rather than claimed.** Fixtures that populate every optional
field are a hand-maintained list, and the failure it misses is the one that will happen: a field
added to a node next quarter, no fixture varying it, the arm free to drop it, the test green.
Three levels, and the third is the one worth building:

- *arms* — the match over `NodeRef` is exhaustive, so a nineteenth kind fails to compile;
- *kinds* — the fixture set is asserted to reach every variant, so a kind with no fixture is red;
- *fields* — derive them from the debug output rather than listing them. Every field name a node's
  debug prints must take **at least two distinct values across the fixture set**, because a field
  constant everywhere is a field whose loss no fixture can detect. It goes red naming the field,
  and it goes red the day the field is added rather than the day something depends on it.

That last one is what makes the case exhaustive without reflection and without a maintained list.
It is also a guard that can go red on its own, which on this chain has been the exception. Not through
`driver/plans.rs` — it is `#[cfg(test)] mod plans`, so an integration test cannot see it, and its
builders leave every optional field `None`, which is the half the property needs. The fixtures
live beside `rebuild` and give each node a value in every optional field: `Filter` with a
projection as well as without, `Limit` with a fetch, `Join` with a residual. A corpus plan covers
only the combinations its queries happen to produce, and a fixture set of all-`None` nodes would
let a dropped field through in silence.

Cover the arms the way `every_node_kind_builds_the_executor_its_category_names` covers its own:
assert the fixtures reach every `NodeRef` variant, so a kind added later is a compile error in the
match and a red count here. It goes red by dropping one field from one arm. Nothing is injected
until it passes, and no corpus query is needed to prove it.

**Two mechanisms, two dimensions each, because the dimensions are not the same kind of thing.** A rebatcher is a node —
it changes the tree, and it must be placeable anywhere a batch flows, not only above a source. The
other three are behaviour a node does not carry: this engine's hash is Spark-murmur3 fixed in the
emitter, and neither an empty lane nor a zero-row batch is a field of `GpuLoadParquet`.

| Dimension | Mechanism | Why not the other one |
|---|---|---|
| a rebatcher, at any edge that still validates | tree rewrite: insert a node above the chosen child. One direction only — nothing below the loader splits a batch ([#142](../tickets.md#t142)), so the node is `GpuCoalesceAllBatches` merging a lane to one, and the finer direction is the mode axis already. A coalesce clears the sort order by construction, so an edge under a `GpuMergeSortedPartitions` or a limit-after-sort is refused at validation: assert the refusals are exactly that class rather than skipping those edges quietly, since a skipped edge is a hole nothing reports | wrapping an executor cannot do it — an exec is one batch in, one out, and a merging rebatch must hold batches across calls, which is an accumulator's job. A wrapper that held them would lie to the accountant and break the queue bound the driver guarantees |
| a drained lane | tree rewrite: move that lane's row groups into another lane | **not** a wrap — a source producing nothing for a lane loses its rows, so every oracle comparison fails and the dimension is untestable. `partition_groups` is the mapping, so moving groups keeps the lane count and every row. Safe at every plan this mode produces: `co_partitioned()` requires one lane or a ByHash distribution on the join keys, so no join here rests on scan lane alignment — the shape the prototype needed `_is_shuffled_join` for and this mode does not have |
| zero-row batches at a probability | wrap the source executor: emit an empty batch instead of advancing | not a node field; the driver already carries empty batches, so this exercises a path the operators have and this one does not |
| a degenerate hash | wrap the emit executor: route every key to one lane. **Not compatible with a plan carrying Right, Full or RightAnti**: one lane holding every key leaves the others with an empty build side, and those three owe their probe side against one, which is [#175](../tickets.md#t175)'s refusal. Drop the dimension for those plans and pin the refusal with a case, so the drop is a rule a reader meets rather than an abort they rediscover | the emitter carries key *expressions*, not a hash function — ours is fixed |

Lane counts are deliberately absent: `target_partitions` already varies them through the planner,
and a lane count no planner would choose is the one dimension whose correctness rule the prototype
had to encode (`_is_shuffled_join`, `_LaneOrigin`). Buying that rule to vary something the mode
axis already varies is not worth it here.

**Three phases, and the middle one is the point.** Plans first, selection second, execution third,
because 10 queries × 5 modes × the injection crossing is more runs than a CI tier can hold, and
choosing *which* to run is a claim that has to be visible rather than a `take(30)`.

1. **Plan.** For each enabled query, plan at all five modes. Five plans, no injection yet, and a
   plan that fails to build is a failure rather than a skip.
2. **Select.** Derive the candidate set — each plan crossed with the injection settings — and
   choose at most **30 per query**. Representative means the selection covers each dimension at its
   boundaries and each mode at least once, not a sample: the rebatcher in both directions, the
   empty-batch probability at zero and at its high setting, a drained lane where the mode has more
   than one, the degenerate hash where the plan shuffles. The chosen set is **asserted, not
   trusted** — a test over the selector alone, with no queries, that a known candidate set yields
   a cover, and that dropping a dimension from the settings makes it go red. Deterministic and
   seeded: two runs choose the same 30, or a failure is not reproducible.
3. **Run.** Each selected plan through the driver, every answer against the same oracle.

**Render the oracle once per query.** Comparing renders both sides, so an oracle rendered per
variant makes a query's cost grow with the number of variants for no reason — at thirty variants
that is most of the tier. Render once, hold the text, compare every variant against it.

**The oracle does not change and must not.** It is DataFusion on the same SQL, planned and
collected once per query at `target_partitions=1`, compared against every variant. That is what
makes injection meaningful: the answer is fixed by something that never saw the layout, so a
layout that changes the answer is a defect rather than a disagreement between two of our own
shapes. One oracle per query, not per variant — it is the expensive half and it is invariant.

**Eleven queries, and the cost axis is not the obvious one.** Injection multiplies runs, so it goes
on the cheap end of the list — but *cheap* here is the size of the result, not of the scan. A run's
cost is dominated by the oracle comparison, so `anti-join`, a `SELECT *` over 1.2M rows, is the
most expensive member of a set picked by rows scanned. Scanned rows at sf1, which is the proxy the
set was first chosen by and is kept here so the correction is legible:

| Query | rows scanned | what it carries |
|---|---:|---|
| tpch `nested-loop-join` | 40,000 | nested-loop Inner |
| tpch `nested-loop-left-join` | 40,000 | nested-loop Left, single-batch probe |
| tpch `nested-limits` | 220,000 | both row-interval lowerings, cross join |
| tpcds q45 | 899,384 | LeftMark |
| tpch `anti-join` | 1,600,000 | RightAnti |
| tpcds q8 | 3,060,404 | Inner multi-key, LeftSemi with `null_equals_null` |
| tpcds q16 | 3,087,163 | LeftAnti, LeftSemi with a filter, a mid-plan limit |
| tpcds q93 | 3,187,918 | Right outer, multi-key |
| tpcds q97 | 4,361,952 | Full outer |
| tpcds q2 | 4,401,864 | the union that cannot interleave |
| tpcds q33 | 5,281,336 | the four-lane interleave — eleventh on cost, first on merit |

q33 is in on merit, and it happens to cost little: eleventh of the seventeen, six heavier ones
left out. It is the only four-lane interleave in the
list, and an interleave is the one operator whose correctness *is* a lane correspondence — output
lane p from lane p of each branch. Excluding the one shape a perturbed lane could break, to save
0.9M rows over the tenth, would be picking the cheap set over the point of the exercise.

The six left out are the heavy end — q38, q87, `shuffle-stddev`, q20, `left-join`, and q21 at
19.5M rows, three `lineitem` scans in one query. If the budget turns out to allow more, `left-join`
is the next one worth having: a Left outer's finish pass accumulates probe keys, so its residency
is a function of how the batches arrive.

**Shape of the change.** One new test library file carrying the wrappers, the settings, the
candidate derivation, `rebuild` and the selector — `peacockdb-core/tests/common/injection.rs`,
split into a second file only if it passes the 1000-line bar. The **tests** go in
`test_cpu_batch_partitioned.rs`, not beside the mechanism: a `#[test]` under `tests/common/`
compiles into every one of the 22 binaries that declare `mod common`, so it would run 22 times and
be counted 22 times. A target of its own would avoid that and costs a `pipeline.yml` step and a
`test_ci_coverage` entry, which is not worth it for two cases. The eleven
fixtures are declared as one list: `injected_queries!` expands it into both the `INJECTED` const
and the fixtures, so a query leaves the set only by leaving the list, and everything else is
unchanged. One thing in `peacockdb-core/src` changes, and only its visibility: `validate()` becomes `pub`.
The driver validates nothing — `check_canonical_form` is limit positions and no more — so an
injected tree would have run unchecked, and a rewrite that broke a node's requirements would have
answered rather than been refused. That is the failure this task must not have: an injector
quietly generating plans the planner would never emit. It is also what makes the rebatcher's
refusal a demonstration rather than a prediction, since `validate()` is what names the node and
the order it broke.

The cases split by dataset rather than by subject. Four need none — the identity case, the
selector's cover, the rebatcher refusal and the degenerate-hash emit — and go in
`tests/test_batch_partitioned_injection.rs` with its own `pipeline.yml` step and
`test_ci_coverage` entry, so they run in seconds on any host. The two that need a query — the dimension demonstration, which plans
`nested-loop-join` over sf1, and the injected corpus set — stay in the end-to-end file. A mechanism proof buried in a tier that takes minutes is one nobody runs
while iterating.

**Measure before capping, one run at a time.** T17's seventeen queries at five modes are 85 runs in
4m39s at four threads. Eleven queries at up to 30 is 330, on the cheap end of the corpus — a number
to measure, not to assume.

The measuring pass runs **serially**, `--test-threads=1`, and times each run on its own. Four
threads contending for the same host is what the correctness tier wants and the opposite of what a
timing wants: a number taken under contention cannot be compared with another taken under different
contention, so a table built from a parallel run would rank the wrong things. Time each
(query, mode, injection setting) individually and report every one — not a total, since a total
cannot say which row to cut.

The table is the deliverable of that pass and belongs in `llm-wiki/reports/`, since it is a
measurement of a host rather than a fact about the code: one row per query, the per-setting times
across it, its total, and the grand total, with the host and thread count at the top the way the
benchmark records carry `build_profile`. Then the cap is chosen against it.

**What may be trimmed, and what may not.** If it does not fit the tier, cut runs and not cover.
The selection rule already guarantees each dimension at its boundaries and each mode at least once,
so a smaller cap is still a cover — that is what the rule is for. What must survive any trim is one
carrier per injection dimension and the shapes that only one query has: q33's interleave, the two
nested-loop forms, `nested-limits`' two row-interval lowerings. Trimming to the cheapest eleven
minus q33 would be the obvious cut and the wrong one, for the reason q33 is in the list at all.
Where a query is dropped entirely, say which dimension lost a carrier and why the remaining ones
cover it.

~~**T18 — infra for running corpus queries.**~~ (done). T17 proved the path on seventeen queries and T17a
injected layouts into eleven; the corpus is 39 tpch and 99 tpcds, each at five modes on two
engines. What stands between the two is not more test cases but a way to declare one. This task
builds that. It was split out of T20, which carried both this and the new query shapes, once it
was clear the rollout needed the infra before it needed either.

**One declaration per query.** `corpus_query!(dataset, sf, query, cpu_modes, gpu_modes,
cpu_oracle, gpu_oracle)`, where the two mode arguments are a bitwise or over the five planning
modes the plan goldens already carry. A query's whole coverage is then one line that can be read
and diffed, rather than up to ten macro invocations that can disagree with each other. It expands
to one test case per (query, enabled mode) on each engine, so `cargo test <query>` names exactly
the runs that query has — which is what makes the filtered regeneration below possible at all.

The CPU and GPU cases go in **different binaries**, as the legacy pair does: the GPU targets are
staged to shad-gpu by the gpu-tests job, and `test_ci_coverage` verifies that job's array names
them. One macro writing into two binaries is the reason the mode arguments are separate — a query
can be enabled on the CPU at four modes and on a device at one.

**The oracle keywords already exist and are taken unchanged.** `cpu_oracle` is
`data_fusion_exact | data_fusion_approximate | data_fusion_subset`: one oracle, plain DataFusion
at `target_partitions = 1`, asked three ways — the whole answer, the whole answer at a 1e-12
relative tolerance where the sole divergence is float summation reassociation, and the count and
containment alone where the SQL does not determine which rows (see the unordered `LIMIT` below).
The third is new; the first two are legacy's, unchanged. `gpu_oracle` is
`golden_exact | golden_approx | golden_approx_std | live_cpu | skip`: the frozen result compared
exactly, at 1e-12, at 1e-11 where cuDF's variance diverges further than the convention allows,
against a live CPU run where the result is too large to commit, and not at all for a query whose
row order is undetermined. A second vocabulary for the same choice is how two families drift, so
these are the legacy sets or they are a rename of them, never a parallel set — and `live_cpu` is
that rename, applied to legacy's ten call sites in the same commit. Legacy spells it `oracle`,
which inside an argument named `gpu_oracle` says only that an oracle is an oracle, while the
choice it actually makes is between a frozen result and a live one.

There is no seventh argument for whether the run writes `.result.txt`. Legacy carries one
(`result_golden` / `no_result_golden`) because its producer and consumer are declared in
different files, and an orphan golden — written by a CPU case no GPU case reads — is silent
where a missing one is loud. Here both sides are in one declaration, so the predicate is
`gpu_oracle` naming a golden at any enabled mode, and the pairing cannot be stated wrongly.

**Goldens stop being one file per query.** Today each query carries its own `.cpu.txt`,
`.cost.txt` and `.result.txt` per mode, which is how `testdata/goldens/` reached 277 files for
tpch.sf1 and 625 for tpcds.sf1. This mode takes the shape its plan goldens already took: one
`.cpu.txt` and one `.cost.txt` per mode, each holding every query in `== <query>` sections, and
one `.result.txt` across all modes. The comparator is the plan goldens' own `section_differences`
— it names what moved, what is missing and what is out of order, and it has unit tests — so this
is a second caller rather than a second differ.

A query whose bit is clear for a mode still has a section in that mode's files, carrying a marker
that says it was skipped. An absent section and a skipped one are different facts and the file has
to hold both: a query that stopped planning is a regression, a query never enabled at that mode is
a decision, and a format that renders them alike loses the only artifact that could tell them
apart.

**`.result.txt` is one entry per query**, keyed by the query alone. The modes are supposed to
agree on results, so one of them authors it: the last mode the query declares in the fixed
sequence of five — `bp-tp4-sized` for most. Its authority comes from the declaration and not from
what happened to run, which is what keeps it well defined under a filtered regeneration. A run
that does not include the authoritative mode leaves the section untouched; it is the one golden
whose key carries no mode, so it is the one a partial regen could otherwise re-author from a mode
that is not the authority, with the body's own line moving to say so and nobody reading it. The
section still records which mode produced it: the key carries no mode because there is one entry,
and the body names one because where the modes disagree, that disagreement is what the file exists
to make visible. A result at or above `RESULT_GOLDEN_MAX_BYTES` (256 KB) keeps its section and
carries a marker saying so, rather than being deleted — and the cap is reached while rendering
rather than after it. Legacy renders the whole answer to one string and then measures it, so
`anti-join`'s 1.2 million rows are materialized in full to discover they are 240 MB and unwanted.
Here the rows are rendered one at a time against a running total and the whole set is dropped the
moment it passes the cap, so the peak is the cap and one row rather than the answer. Which also
settles the sort: a set that is never going to be written is never sorted — the legacy path deletes it, which reads as
"no golden" and as "golden not applicable" identically, and `build-test.md` states the old rule
and is corrected in the same commit.

**`live_cpu` is what the device uses when no frozen result can serve it, and there are two such
cases rather than one.** The first is legacy's: a result over the cap has no golden to compare
against, so the device is held to a live cpu run instead. The second falls out of this mode and
has no legacy counterpart — `.result.txt` holds one entry per query, authored by one mode, while
the device runs at every mode the query enables. Where the rows are the same at every mode that
is one golden serving five runs, which is the point of the single entry. Where they are not, it
is one golden that only one of them can match: `scan-limit` at `bp-tp1-single` returns different
rows from the `bp-tp4-sized` section, and `golden_exact` would fail on a correct device. So a
query whose `cpu_oracle` is `data_fusion_subset` writes `live_cpu` as its `gpu_oracle`, and is
compared against a cpu run at the *same* mode — where both walk one driver over one plan and the
rows are determined again.

**Written at the call site, never inferred from the argument beside it.** Every argument is
required and says what it means; a `gpu_oracle` deduced from a `cpu_oracle` is a behaviour
selected implicitly by an input, which is the shape `coding-style.md` names as an antipattern and
which this macro exists to avoid — seven arguments so a query's whole coverage reads off one
line. That both conditions are derivable is why a *check* can exist, never why a value would be
absent: a `golden_exact` where the cap or the subset oracle applies is asserted red, since it is a
test that fails on correct behaviour, and a `live_cpu` where neither applies is asserted red too,
since it spends a device-side live run on a comparison a committed file makes faster and harder.

**Every result comparison decides before it materializes, and today none of them do.** Three
places take the same wrong order and the fix is one idea applied three times. Containment collects
the unlimited answer and renders every row; the cap renders the whole result and then measures it;
and `assert_results_match`'s exact arm is `assert_eq!(render(actual), render(expected))`, whose
arguments Rust evaluates eagerly — so both sides are materialized in full to answer a yes-or-no
question, half a gigabyte of `String` for `anti-join`, and the rendering exists for the failure
message rather than for the comparison.

The exact arm becomes a digest: render a row, hash it, drop the string, sort the hashes, hash the
sequence. Memory goes from the answer to eight bytes a row, the verdict is unchanged, and nothing
is rendered on the green path at all. On a mismatch it re-streams and prints a bounded excerpt
around the first differing row — which is not a courtesy but the same lesson `section_differences`
carries, that a dump too large to read is a dump nobody reads, and a 240 MB `assert_eq!` diff is
that failure three orders of magnitude over. The tolerance arm keeps its map, because a digest
cannot express "within 1e-12"; it stops formatting the columns it does not key on.

No verdict changes, so no green test moves.

**A `live_cpu` comparison runs once per mode, beside the device run it checks.** It cannot reuse
another mode's answer: where the SQL does not fix the row set, a cpu run at `bp-tp4-sized` is no
more an authority on what the device returns at `bp-tp1-single` than the frozen section was, which
is the reason this value exists at all. Same rule for the over-cap case even though its result is
mode-invariant — the test is already per (query, mode), so same-mode costs nothing extra and one
rule beats two that differ by a condition a reader has to check.

What it costs is on the device leg, which is the leg that runs `--test-threads=1` on one host: a
`live_cpu` query executes twice per mode there, once on the device and once on the cpu backend
through the same driver. Legacy carries ten such queries and this mode adds `scan-limit`, so at
five modes it is on the order of fifty extra cpu runs on the gpu host. Named here rather than
discovered in a job duration, and it is part of the cost the corpus tier accepts rather than a
separate decision.

This is not the `result_golden` case one paragraph up. That argument is omitted because it has no
degrees of freedom left once the rest of the line is written — legacy needs the keyword only
because its producer and consumer live in different files. `gpu_oracle` has five values, and the
constraint pins it in two cases out of five.

A section can therefore turn from content into a marker, and back, as a result crosses the
threshold. Nothing guards that transition and nothing should: at a fixed scale factor a result's
size moves only when its answer moves, which is the thing every other check in the tier is
already watching. The marker is there so the file says why a result is absent, not to absorb a
size that oscillates.

**A run that stopped early says so, on its own line.** A satisfied limit ends the run with work
undone: lanes that were never pulled, row groups never read, batches produced and never consumed.
Every one of those shows up in the annotations as a smaller number, and a smaller number with no
stated cause is indistinguishable from a plan that produced less — which is the absent-versus-
skipped confusion this tier exists to prevent, one level down. So each section opens with
`early_exit=<node>@<ordinal>` naming the limit that was satisfied, or `early_exit=none`. Always
present, never inferred from absence: a query that ran to completion is a fact the file states.
It is stable, because [the determinism rules](#determinism-rules) pin the schedule — which
batches were consumed before the limit was met is fixed for a plan.

Beside it, `rows_skipped=N` on each node that has any: rows released without an unload call,
which the driver already counts per node precisely because the rows returned look identical
either way. It is the saving a limit buys, and the golden is where it becomes visible.

**A batch emitted and never consumed is still emitted.** It is counted where it enters a queue,
so it is in its producer's `batch_rows` and `batch_bytes` whether or not the parent ever took it;
what the parent took is its `in_rows`. The gap between the two is the work a limit threw away,
per child and per lane, and it is the quantity `early_exit` exists to explain — without the
marker it reads as a node that emitted more than its parent wanted, which is a defect
everywhere else.

The same rule decides the cost, and the answer is not the intuitive one: `.cost.txt` prices what
was produced, not what was used, because the device did that work and holding those bytes is what
the budget was spent on. A query that exits early is genuinely cheaper than one that does not,
and the batches it produced before exiting are genuinely not free.

Where the drop begins is bounded rather than recorded, and does not need a third list. Queues are
FIFO and arrival order is pinned, so what a parent took is a prefix of the child's lane — the
cumulative sum of `batch_rows[j]` reaching `in_rows`, ending mid-batch only where a limit sliced a
straddling one. So `in_rows[child][j]` lies between two adjacent prefix sums of that child's lane,
which is a tighter check than `<=` on the total and is the form to assert.

**Two checks change shape under it, and both get stronger rather than weaker.** The loader
identity — `batch_rows` having `partition_groups`' exact shape — is a plan-against-run
comparison, and early exit is exactly when the run does less than the plan. Per lane it becomes a
prefix, and per lane rather than over the flattened shape: `batch_rows[j]` is a prefix of
`partition_groups[j]` — its own lane's list, aligned from the start because a scan reads its
groups in order — with an empty lane allowed, and every lane equal on a run whose marker says
`none`. The scheduler stops where it is, so one run can have lane 0 complete, lane 1 short and
lane 2 empty. Compared flattened, that run either goes red or forces the check down to a total
length, which no longer says a batch lines up with the row groups that made it — the whole reason
the nesting is there.
The `in_rows` identity needs no marker at all, because `abandoned` is rendered beside it. That was
decided against a count rather than a hunch: keying the arithmetic on the marker would soften it
to `<=` at every node of an early-exiting section, and 84 of 99 tpcds queries carry a `LIMIT` — of
the 67 with a committed result golden, 35 return 100 rows or more, so the limit really fires. Half
the bench, not a handful. The marker is also not precise enough to carry arithmetic: it is
`any_satisfied()`, so a query returning exactly 100 rows sets it while consuming everything, and
the weakening would apply where nothing was skipped.

So the conservation law does the work — `consumed + abandoned == emitted`, one statement over the
file and over the report, rather than a law in the driver's tests and a weakening of it in the
golden. Same family as `in_flight_bytes` returning to zero and `holds == releases`, which the
report already carries: a batch that vanishes without being either consumed or abandoned is a
defect no inequality can see. Deciding it now costs one optional field in a format gaining five;
after T19 it costs regenerating eleven files per bench across a hundred queries.

The marker keeps the two jobs it is good at: telling a reader the run stopped early, and keying
the loader's prefix, which `abandoned` cannot close — a row group never read produced no batch to
abandon.

**Several test cases now write one file, and that is the task's one real hazard.** A whole-file
write is last-writer-wins, which would drop every other query's section and leave a green run. So
a regenerating write takes an advisory lock on the file, merges its section, and publishes by
writing a sibling and renaming onto the name — the rename because a crash mid-write must not
leave a truncated golden, which `point_canonical_root` already does for the same reason.

**The lock is on the file, not in the process, and the distinction is load-bearing.** Libtest runs
a binary's cases as threads in one process, so a `Mutex` would serialize them — but that is a
guarantee about one binary, and what makes it sufficient here is the separate fact that only the
CPU binary writes. An invariant, not a property of the language: the next writing binary breaks it
with nothing red, `cargo nextest` runs each case in its own process and would silently reduce a
mutex to no lock at all, and two shells regenerating at once are outside any of it. The
`canonical_root` comment records the same surprise one level down — two binaries reaching one path
at the same time, fixed with an atomic rename rather than a lock, because a lock in one process
could not see the other. `std::fs::File::lock` is stable on the toolchain in use, so this costs no
dependency.

**Partial regeneration follows from that.** A filtered run regenerates only the sections its cases
produced and leaves every other section as it is, under an environment variable of its own rather
than by widening `UPDATE_CANONICAL`, whose contract is a whole file. `PCK_TEST_FILTER` already
scopes which cases run, so the mechanism has a caller before it has a second one. What must
survive is the distinction above: a filtered regen that cannot tell "did not run" from "stopped
planning" deletes coverage silently, which is exactly what a golden exists to prevent.

**`.cost.txt` stays a pure function of its `.cpu.txt` and `cost_model.conf`**, so the derivation
is per section rather than per file, and `test_cost_model` re-derives every one.

**The GPU side reads what the CPU side wrote**, as in legacy: per-node plan shape and the
input/output statistics against that mode's `.cpu.txt` always, and `.result.txt` where the
`gpu_oracle` names a golden. It never writes either, and ignores the regeneration variable
rather than honouring it — a device that can author its own golden proves nothing against it.

One thing is easier here than in legacy, and worth not spending twice. Legacy's two engines run
separate executors, so the two rendering the same tree is a coincidence the golden exists to
check; here `batch_partitioned_driver` is generic over the backend, so both engines walk one
driver and report through one `RunReport`. The rendering is therefore written once, and what the
golden still checks is the answer the device gave, not the shape of the report it came in.

**Registry.** The five `bp_*` columns exist and mean plan enablement, declared by the golden's
section rather than by a macro — `test_batch_partitioned_plans` holds the two to each other in
both directions. Execution needs its own columns, five per engine, and those are macro-declared
through the existing link-time inventory, so the widget's three groups each have a source that
something checks. Adding ten columns to a seventeen-column csv is the point to decide whether the
row stays flat or the modes become a repeated group; the inventory tests are what must keep
working either way.

**Widget.** A batch-partitioned table per bench, repeating the legacy structure — peacockdb cost,
duckdb cost, ratio, features, tickets — with the four mode columns replaced by three: planning,
cpu execution, gpu execution. Each holds one cell per mode. A planning cell links to that query's
section in `<mode>.plans.txt`, which is also where its refusal is, so an enabled query and a
refused one link to the same file and differ only in what the reader lands on. A cpu cell links to
the query's section in that mode's `.cpu.txt`. Where not all modes are enabled, the cost columns
use the last mode in the sequence for which cpu execution is enabled, as the legacy rule does for
its own last mode.

**The cost-regression gate is a third rendering, and it degrades silently unless this task
changes it.** `--cost-diff` compares each `.cost.txt` against the same path at the PR's base and
upserts its own PR comment: improvements green, regressions red, and a non-zero exit that fails
the build. It is where a cost win becomes visible, so the new mode belongs in it. But it is
written for one file per query — `collect_cost_goldens` globs `*.cost.txt`, `read_total` takes
the *first* `peacockdb_cost=` line in a file, and `diff_label` takes the filename's first
dot-segment as the query name. Point that at `bp-tp4-sized-mini.cost.txt` and it picks the file
up, reads whichever query's section happens to be first, and labels the number with the mode: a
wrong row, rendered confidently, gating the build. The glob is what keeps it quiet — the files
are found, so nothing reports them missing.

So the differ becomes section-aware in the same way the comparator did: a row per (query, mode),
totals read per section, the label carrying both. `cost_diff` itself is unchanged, since it
already works over a map of label to number; what changes is what fills the map. Its unit tests
are the pattern to extend, and the red case is a two-section file whose second section moved —
the per-file reader reports no change, the section reader reports one.

**There are two widgets, and both get the table.** `cost-report` renders the same data twice —
`--md`, a markdown blob upserted as one PR comment keyed on a sentinel, and `--html`, the site
published to Pages from master. Close to identical rather than identical, and the differences
are the ones a format forces: markdown cannot set a row background, so a row over the ratio
threshold is flagged in the cell instead, and the mode cells have a markdown counterpart to
`mode_cells_html` rather than sharing it. A table added to one and not the other is the failure
to expect here, because the html is what a person looks at and the markdown is what the review
actually reads.

**No production change that alters what the engine computes, here or in T19.** A query that
does not run is disabled with a ticket, never fixed in passing: a fix made while enabling one
query is a fix nothing else in the branch proves, and the diff under review stops being the
infra. What this task does touch in `peacockdb-core/src` is the goldens' own surface and nothing
else — the node renderer per [Node display](#node-display) (`name@ordinal` references, layout in
place of the lane count, `fetch` and the aggregate lists wherever carried, schema in the plan
golden only), the per-batch line below, `cost_model.conf` entries for all eighteen node kinds at
once rather than as each is first seen — held there by a case red on a nineteenth, derived from
the exhaustive `as_node_ref` the way the writer's field cover is, since entered-at-once is a
moment and T19's "no `cost_model.conf`" rule needs a property — and
per-node emitted rows and bytes on `RunReport` — the one thing the engine does not already
report. Everything else the
comparison needs is there: `Batch` gives rows and bytes on both backends, the device's coming
through the frozen ABI's `PeacockNodeStats` priced against the declared schema, and
`trace`'s per-event `outputs` sums to `out_batches` per lane. What is absent is a per-node total
of what each node emitted; the driver reads both numbers already, for the accountant and the
limit interval, and keeps only aggregates. `rows_seen` is not the emitted total and must not be
reused as one — it counts rows arriving at a node for the limit rule, which is `in_rows` summed
over lanes rather than anything a node produced. Plus the
`pipeline.yml` steps `test_ci_coverage` requires, which are not engine code at all. Three more landed than that list, each approved on its own and each recorded here rather than
only in the exchange that approved it: `CpuBatch::byte_size` pricing from the plan's schema, which
is the one the headline forbids by name and which moved every budget decision in the engine; the
`gpu_backend` guard closing [#181](../tasks/active-tickets.md#t181); and `driver/mod.rs`'s `mod index` going
`pub(crate)` so the renderer walks the driver's own index rather than a second pre-order.

Naming that surface is what gives the rule an edge, and the edge only holds against a list that
is true: T19 has none of these seven left to touch, and T20 is where the engine moves again.

**Every node carries what it consumed and the size of every batch it emitted**, as parallel
structures on one continuation line under the node:

    GpuLoadParquet: table=lineitem, partition_groups=[[[0,1],[2,3]],[[4],[5,6,7]]], lanes=2, …
      in_rows=[] batch_rows=[[1500304,1500303],[1500304,1500304]] batch_bytes=[[31881456,31881440],[31881472,31881455]]

Lanes outermost and batches within, which is `partition_groups`' own nesting — so on a loader,
element `i` of lane `j` in all three lines is one batch, and the row groups that produced it sit
at the same index as its size.

`in_rows` is what the node consumed, per lane, nested by child rather than flat — a filter reads
`in_rows=[[860160,737280,…]]` and a join reads both of its sides. No space after a comma, which
is `partition_groups`' committed convention — the two nestings are read index for index and the
tail case is a 96-batch lane. Legacy prints one `in_rows`
per partition and it is the first child's, so a join's build side appears nowhere on the join's
own line; here the two sides differ in kind and the capability matrix turns on which is which,
so a format that can only show one of them is one that hides the interesting half. A source has
no children and prints `[]`, which is not the same as a child that consumed nothing. It is the
one figure the batch lists cannot imply, and the reason legacy's sub-line is dropped rather than
merely thinned: rows in against rows out, on the node's own line, is what makes a selectivity or
a skew visible without walking the tree.

Rows and bytes are separate lists rather than pairs because each is then a column a reader scans
down and a diff reports as one moved number; the cost is that a batch's two figures are not
adjacent, which the shared indexing is what makes navigable. All on one line, so a node stays one
entry however many batches it has.

This is where the per-batch record lives, rather than in a `batch-info.cpu.txt` for a chosen ten:
a separate file for a subset was worth it only while the figures were too bulky for every node,
and at a median of one batch and a p90 of six they are not. The tail is what to watch — the
worst source in the corpus is tpcds `inventory` at `bp-tp1-rowgroup` with 96 batches in one
lane, about two kilobytes across the two lists — so the comparator reports a moved section by
name and never dumps the line.

**A failing query is disabled with a ticket in [`active-tickets.md`](../tasks/active-tickets.md)**, not in the
main list. Rollout tickets arrive in bulk when a sweep hits a wall and close in bulk when it is
cleared, which is not what a triage pass reads for. The ID space is shared, so a number is never
two things. `TicketIndex::load` reads two files and gains a third here, and it is
not cosmetic: `cost-report` already exits 1 when a ticket the registry names resolves in neither
file, so the first rollout ticket filed in `active-tickets.md` fails the cost-report job until the
index reads it.

**What proves the infra, since every way it fails is quiet.** A dropped section, a filtered
regeneration that deletes coverage, a skipped marker a reader cannot tell from an absent one — none
of these turn a run red on their own, so each gets a case that goes red on demand, built from
strings rather than by editing a golden and undoing it:

- two writers interleaved at the merge keep both sections, and the same case with the lock
  removed loses one — a lock nothing can be shown to need is a lock the next refactor drops.
  Forced rather than raced: two threads that merely start together prove nothing on a fast
  machine and fail on a loaded one, so the overlap is imposed — a barrier between read and write,
  or the merge driven directly with a second writer's section already on disk. Every case here
  is deterministic or it is not a case: this tier's whole claim is that a golden means something,
  and a test that passes on the third run is a golden that means nothing;
- a filtered regeneration rewrites its own sections and leaves every other byte identical;
- a section absent because its case did not run and one absent because the query stopped planning
  are distinguished, and the run that confuses them fails;
- a skipped marker round-trips: a query whose bit is clear writes one, and clearing a bit that was
  set turns a real section into a marker rather than deleting it;
- both of `cost-report`'s renderings carry the new table, asserted per rendering rather than once;
- the section reader and the per-file reader disagree on a two-section file whose second section
  moved, which is the cost gate's red case.

**Three things have to agree, and the inventory test becomes the place they do.** Legacy holds
the macro invocations and `cost-registry.csv` to each other in both directions, per binary,
because `inventory` collects per linked binary. The corpus tier adds a third leg: the goldens.
Every `corpus_query!` registration means an enabled cell, every enabled cell means a
registration, and every enabled (query, mode) means a golden section carrying content rather than
a skipped marker — with the converse holding too, since a `disabled` cell whose section is full
is coverage nobody is reading. The plan columns stay declared by the goldens alone, as they are
today; what is new is the execution columns being declared by the macro and checked against both.
One consequence to state rather than discover: the gpu half of this runs only on the gpu host,
because that is where its registrations are linked, so a gpu-column drift does not go red on the
cpu leg.

**Soundness is checked against the file's own redundancy, not with regular expressions.** A
regexp says a line looks like a line. What is wanted is whether the numbers mean anything, and
the format carries the same quantity more than once by design, so the checks are arithmetic and
cheap:

- a node's `out_rows` is the sum of its `batch_rows` lanes, and `out_bytes` of its `batch_bytes`.
  A batch is counted where it enters a queue, so a scatter output dropped for being empty is not
  one — the sums are unaffected either way, and the batch count then means what flowed;
- the lane count of those lists is the `lanes=N` on the node line;
- a node's `in_rows` for child *k* equals that child's `out_rows`, **per lane and not only in
  total**: the index is the *child's* lane, so it lines up with that child's own `batch_rows`
  entry for entry. Indexing by the consuming node's lanes would make the identity checkable only
  in aggregate wherever the two counts differ, which is every emitter, merge and accumulator —
  the nodes it is most worth checking. An equality on every run, because the node renders
  `abandoned` after its own `batch_bytes` — rows `release_in_flight` dropped, per lane, omitted
  where zero and so absent from every node of every run that drained. It sits on the node that
  *emitted*, because that is whose output queue was dropped, so the law crosses parent to child
  exactly as `in_rows` already does. The law is
  `consumed + abandoned == the child's emitted`, with no marker in it and no `<=` anywhere. The
  two exceptions this once named were measured and are both wrong: dropping a probe batch is
  consuming it, and a satisfied limit falls short at whichever node the schedule stopped at rather
  than at the limit;
- on a loader, `batch_rows` has `partition_groups`' exact shape, which is the correspondence that
  nesting was chosen for;
- the root's `out_rows` is the row count in `.result.txt`, which the oracle checked.

That last one is the interesting one, and the rest are worth more than they look. The cost tree
has no external oracle — the golden is written by the run it will later check — but a file that
contradicts itself is a file a renderer got wrong, and a renderer is most of what could be wrong.
It is a weak oracle rather than none, and it is free: the redundancy is already committed.

**Reading it back needs one parser, and there are already two and a half.** What exists:
`ordered_sections` and `section_differences`, which split on `== <query>` and report what moved,
private to `test_batch_partitioned_plans.rs`; `parse_node_line` in `common/cost_model.rs`, which
finds `output_bytes=` and takes the leading identifier, private, and enough for the cost
derivation and nothing else; and `read_total_str` in `cost-report`, another crate reading a
`key=` off a line. None of them knows a tree, a lane, or a nested list, which is what the
arithmetic above asks for.

So: the comparator moves into `tests/common/`, where both corpus binaries can reach it — and that
move is owed anyway, since `test_batch_partitioned_plans.rs` is at 1418 lines against a
thousand-line cap. `parse_node_line` grows from `(type, bytes)` into the node's fields and its
indent depth, and `cost_text_from_cpu` becomes a caller, so the cost derivation and the soundness
checks read one structure rather than two that agree by luck. `cost-report` stays separate, since
it reads a total out of `.cost.txt` and never a node line; what must not drift between the crates
is the `== <query>` convention itself, which is why it is written here rather than inferred from
whichever file a reader opens first.

A fourth parser is the thing to refuse. Three readers of one format already disagree about what a
node line is, and the format is about to carry four more fields.

**Consolidating them saves no code, and that is not the reason to do it.** All the parsing in the
tree is about ninety lines: `ordered_sections` is fifteen, `parse_node_line` seventeen,
`cost-report`'s `field` and `read_total_str` thirteen between them, and the rest is
`section_differences` reporting. A parser that knows a tree, a lane and a nested list is larger
than the three it replaces, so the change adds lines. What it buys is one definition of the
format at the moment the format gains four fields — the divergence is the cost, never the
duplication.

**The crate boundary is where consolidation stops, on purpose.** `cost-report`'s `[dependencies]`
is empty, and deliberately: it builds in seconds in the cpu tier because it pulls in neither the
executor nor a device. Making it depend on `peacockdb-core` to share thirteen lines of `key=`
reading trades a stated property for nothing, and a new workspace crate for the same thirteen
lines is worse. It needs the convention, not the code — `== <query>` sections, one
`peacockdb_cost=` per section.

So pin the convention with a fixture rather than with shared code: one committed two-section
sample that `cost-report`'s unit tests and the test-side parser's unit tests both read, each
asserting the same extracted values. The two readers then diverge red without either crate
depending on the other, which is the property that was actually wanted.

**An unordered `LIMIT` runs at every mode, and drops the DataFusion oracle rather than the
mode.** Legacy canonizes `scan_limit` at tp1 because at tp>1 its rows and its per-node bytes vary
run to run. That is a property of legacy's executor, not of this one:
[the determinism rules](#determinism-rules) pin the schedule and require that one plan run twice
gives one answer byte for byte, so a section here is stable at every mode. What those rules
explicitly do not promise is agreement *across* plans — tp1 and tp4 may return different rows
where the SQL does not determine which, which is what an unordered `LIMIT` is.

So the mode is fine and the comparison is what narrows: DataFusion single-stream is not an
authority on *which* ten rows a four-lane plan returns. It is still an authority on the rest, and
an unordered limit determines more than it looks. The count is `max(0, min(n, |unlimited| - m))`
for `LIMIT n OFFSET m` — written with the offset from the start, because the zero-skip form is a
rule that goes wrong on the first query that carries one and T20 adds those deliberately. And the
rows are a sub-*multiset* of the unlimited result, compared as a multiset and not as a set: set
membership passes a run that returned one row twice where the oracle has it once, which is a
live failure mode for a limit over a join. Both are asked of the session that
already runs, neither needs the two plans to agree on which rows, and together they catch what a
frozen golden cannot: a limit dropped, an offset ignored, rows invented, the wrong table sliced.

That is the third `cpu_oracle` value, and it is named for what it checks rather than for what it
declines to — the count and the containment, against `data_fusion_exact`'s whole answer. So no
query's answer is frozen with no external check, `scan-limit` included. The device is held to the
cpu's section on top of that, which is a real check rather than a fallback: both engines walk one
driver over one plan with emission order pinned, so disagreeing means one of them broke the
schedule.

Two limits to state with it. An *ordered* `LIMIT` with ties at the boundary is not covered:
neither `sort_unstable_by` nor cuDF's `sorted_order` is stable, so which tied row survives is
decided by neither engine's contract and cpu-gpu agreement is not owed. And the scope is small —
the determinism section counts one corpus query with a bare `LIMIT` whose rows are undetermined,
`tpch-queries/scan-limit.sql`; the other four limit an already-single-row aggregate.

**A query carries every mode it is correct at, and a ticket for the ones it is not.** Not a
default to be tuned: all five where all five run and agree with the oracle, and where some do
not, the ones that do are enabled and the rest are `disabled` against a ticket naming which modes
and why. That is what makes the mode arguments a set rather than a switch, and it is the same
rule on both engines — a query can be correct at five modes on the cpu and at two on a device,
which is two tickets and not one.

**The cost of running all of it is accepted for now.** 119 queries at five modes is about 595 cpu
runs and as many on a device, against a corpus measurement nobody has: T17a's report covers
eleven queries chosen for being the cheapest. Measured serially, one row per query: 45.9 s for 87
cpu runs on one thread, 26.2 s on the two CI uses, and a 1.34 GB peak of which one query is
1.28 GB. It gates nothing. The one thing that would force this open
again is a job that stops finishing: pipeline.yml's dataset-matrix leg has already been lost once at
fifty-eight minutes on a tier a fraction of this size, and the device leg runs
`--test-threads=1` on one host.

**A first enablement freezes whatever the run produced, and only half of it has an oracle.** The
answer does: `assert_cpu_results_match_datafusion` builds a separate plain-DataFusion session at
`target_partitions = 1` and compares against it unconditionally, before and regardless of any
golden write, so a wrong answer cannot be frozen — and that comparison is the standing check on
every later run too, not a first-enablement rite. The `.cpu.txt` does not: under a regeneration
it is written with no comparison, and nothing independent says the plan should have that shape or
that an interior node emitted that many rows. The root is anchored — its `out_rows` is the
result's row count, which the oracle checked — and every node below it is taken on trust.

What later covers some of it is the device asserting against the same file read-only, and it is
worth being exact about which half of that assertion carries information. The tree carries none:
`batch_partitioned_driver` is generic over the backend, so both engines walk one driver over one
plan and produce the same shape by construction rather than by agreeing. That is the design and
not a weakness of it — the walk being identical is what makes the two engines comparable at all.
The evidence is the rows and the bytes: the same walk, and the device's numbers against the
CPU's. The tree assertion stays anyway, because it costs nothing and goes red on the day that
construction stops holding, which is the only day it could ever say anything.

So enabling a hundred queries is an ordinary act for the answers, and for the interior of the
cost tree it rests on two engines producing the same counts through one walk. The mitigation is
the one T19 already has: batches of five, where a tree that looks wrong is still attributable.

**`build-test.md` gains rows, not only counts**: one per new tier in the category table, and
three in the golden table — the per-mode `.cpu.txt` and `.cost.txt` and the one `.result.txt` —
each naming the partial-regeneration variable beside `UPDATE_CANONICAL`. That table's existing
`.result.txt` row says the golden is deleted above 256 KB, which is the rule this task replaces,
so the row is split rather than edited: legacy keeps its sentence and the new mode states its own.

**And enough queries to prove the infra**: about ten tpch and about ten tpcds at sf1, enabled on
the macro as it is built — **on both engines**, at every one of the five modes each is correct at,
under the same rule and with the same tickets. The device half is not deferred to the rollout. An
infra whose gpu path has never run is an infra T19 would debug at the same time as its first
query, which is the thing staging this task exists to prevent, and the `gpu_oracle` argument,
`live_cpu`, the read-only assert against the cpu's section and the staging array are all surface
that means nothing until a device has walked it. They are chosen by plan size off the committed goldens rather than by
taste, because the constraint is not what can plan — all 22 tpch benchmark queries already plan
and attach recipes at all five modes, as do 81 of the 99 tpcds — but what is small enough that a
failure is legible. The tpch ten are the smallest carrying no ticket: q6, q1, q14, q19, q12, q13,
q15, q17, q3, q10. The tpcds ten are q41, q42, q3, q43, q52, q55, q96, q15, q37, q82. A legacy
ticket on a row is not automatically a blocker here — [#97](../tickets.md#t97) is the real-8-way
join blocker and this is a different executor — so those rows are held back as unknown rather
than as known bad, and T19 is where that is settled query by query.

| bench | enabled here |
|---|---|
| tpch | `q6` `q14` `q19` `q12` `q13` `q15` `q3` `q10` |
| tpcds | `q41` `q42` `q3` `q43` `q52` `q55` `q96` (tp1 only) `q15` `q37` `q82` |

Seventeen queries at 88 (query, mode) cases, not twenty: enabling them is what found that three do
not run. `tpch/q1` and `tpch/q17` are out at every mode on [#163](../tickets.md#t163) — `avg`
declares its count state `UInt64` and the accumulator produces `Int64`, which the ticket records
from the device and which the cpu reaches too. `tpcds/q96` keeps both tp1 modes and loses the
three tp4 ones to [#180](../tasks/active-tickets.md#t180), where a shuffle puts a state merge under a
`count(*)` declared non-nullable.

The set is not topped back up to twenty. It was chosen as the smallest queries carrying no ticket
and two of them turned out to carry one, which is a fact about the corpus rather than a hole to
fill — and q96 partially enabled is worth more than a replacement would be, since it is the only
query here exercising the mode-scoped disablement the whole `cpu_modes`/`gpu_modes` shape exists
for.

~~**T19 — rollout.**~~ (done). Query-by-query enablement across the rest of the corpus on T18's macro,
starting from the twenty it already carries. No production code changes, per T18: a query that
does not run is disabled with a ticket in [`active-tickets.md`](../tasks/active-tickets.md), which is the whole
output of this task besides the enabled rows. Not one line of `peacockdb-core/src` moves here:
T18 left the goldens' own surface finished, so anything this task would have to change is by
definition a query's blocker rather than the rollout's.

**Four paths are edited by hand, one kind of file moves without being written, and a commit
touching anything else has stopped being a rollout.** By hand:

- the shared `corpus_query!` list, one file that every binary includes and reads through its own
  arm, as `gpu_cases.inc` is read today — so a query is enabled once, not once per engine;
- `testdata/cost-registry.csv`, cells rather than rows: all 138 corpus queries already have one;
- [`active-tickets.md`](../tasks/active-tickets.md), for what a query is disabled on — a whole query, or the
  subset of modes it is wrong at, which is the commoner case and the one whose ticket has to name
  the modes or the next reader re-derives them.

Every cell this task turns off carries a ticket, which is an invariant the registry already
holds: no row in it today has a `disabled` cell and an empty ticket list, and the four queries
DataFusion cannot plan at all carry [#23](../tickets.md#t23) against an `na`. A rollout is the
one thing that could break it, since it is the only task that turns cells off in bulk.

**The invariant proves less than it sounds like**, and the gap is worth knowing before leaning on it:
the tickets column is a per-row bag with no mapping to cells in either direction. A row already
carrying a legacy ticket satisfies "no disabled cell with an empty list" the moment a bp cell goes
off, with nobody having filed anything. Checked by hand at the rollout's close and the substance
holds — all 33 numbers resolve, and each of the nine rows disabling a strict subset of cpu modes has
a ticket naming those modes — but on six of the nine that ticket does not name the query, so a reader
of `tpcds/q5` opens four tickets to find which one applies.

**Two fixes ride along, both because the rollout is what breaks them.** The second is
[#194](../tickets.md#t194): `base_total` (`cost-report/src/main.rs:1623`) reads a base-side cost two
ways, and the git-ref arm — the one every PR uses — drops the `section` it was asked for and returns
the first `peacockdb_cost=` in the file. Legacy goldens are one file per query, so the two arms agree
there; the batch-partitioned per-mode files hold ~60 sections each, and this task is what fills them.
So every bp row on the cost widget is baselined against whichever query sorts first — measured on
`tpcds.sf1/bp-tp1-single-mini.cost.txt`, deltas from -99.6% to +341.3% against `q2`, none of them a
cost change. The git arm takes the same `entry_total(text, section)` path as the directory arm, and
the test is that the two arms agree on a multi-section file. The goldens are untouched: only the
baseline was wrong.

**The one test this task adds** asserts that every result section's
`mode=` is its query's last declared cpu mode, and goes in `test_corpus_goldens.rs`. Closing its
over-cap blind spot takes both halves. `over_cap`
(`common/corpus.rs`) writes the `skipped: ` prefix and no `mode=`, so an over-cap section records
everything except who produced it — and an over-cap section whose authority moved after a cut is
exactly the stale case this guard exists to catch. Four sections are over cap today, all in
`tpch.sf1/bp-mini.result.txt`: `q16` authored at `bp-tp1-rowgroup`, and `anti-join`,
`filter-project` and `semi-join` at `bp-tp4-sized`. They rewrite when their authoring mode runs, so
the guard's commit carries a four-section golden diff that the ticket has to explain.

`over_cap` gains the mode, **as its second line, after the marker**. The prefix stays first because
two readers use `starts_with(SKIPPED)` to mean *this section holds no rows*:
`corpus_gpu.rs:120` decides `frozen` by it, and moving the mode ahead of it would let a
`golden_exact` declaration pass against a section with no rows to compare. `corpus_gpu.rs:164` reads
`mode=` as the first line, but only for a `golden_*` oracle, which `assert_oracle_suits_the_golden`
has already refused for a marker — so an over-cap body never reaches it.

The guard therefore discriminates on **a `mode=` line being present**, not on `SKIPPED` being absent:
that set is the real sections and the over-cap ones, and it excludes `not enabled` markers, which
carry no author because none exists. It reads `ordered_sections` directly rather than
`sections_with_content`, whose prefix filter is what hid the over-cap case — and whose three
existing callers are undisturbed, since none of them wants the over-cap sections either.

Then the **golden sections** fill in: a query enabled at a mode stops carrying that mode's
skipped marker and starts carrying its plan, its per-batch sizes and its costs. That is eleven
files for the bench it belongs to — a `.cpu.txt` and a `.cost.txt` per mode, plus the one
`.result.txt` across modes — and twenty-two for a commit spanning both. The number is the point:
it is modes × benches and does not grow with the corpus, so the hundredth query enabled moves
the same eleven files as the first, where legacy would have added three more per (query, label)
and did, 277 of them for tpch.sf1 alone. And **`build-test.md`'s counts** move with the case
count — edited by hand like the three above, in the commit that moves them rather than in a later
sweep, which is why they are the fourth and not a second kind of thing that moves by itself.

**What it enables, and what stays out.** 120 of the corpus's 138 queries plan at all five modes
today, which is the eligibility test — not legacy enablement, since a query legacy disabled may
have been fixed since and the goldens are what know. Twenty are T18's, so a hundred are here:

| bench | enabled here |
|---|---|
| tpch (29) | `aggregate-groupby` `anti-join` `cross-join` `filter-project` `hash-join` `join-int` `left-join` `mixed-join` `nested-limits` `nested-loop-join` `nested-loop-left-join` `q11` `q16` `q18` `q2` `q20` `q21` `q22` `q4` `q5` `q7` `q8` `q9` `rollup-over-join` `scan-limit` `semi-join` `shuffle-additive` `shuffle-additive-avg` `shuffle-stddev` |
| tpcds (71) | `q1` `q10` `q11` `q13` `q14` `q16` `q17` `q18` `q19` `q2` `q21` `q22` `q23` `q24` `q25` `q26` `q29` `q30` `q31` `q32` `q33` `q34` `q35` `q38` `q39` `q4` `q40` `q45` `q46` `q48` `q5` `q50` `q54` `q56` `q58` `q59` `q6` `q60` `q61` `q62` `q64` `q65` `q66` `q68` `q69` `q7` `q71` `q73` `q74` `q75` `q76` `q77` `q78` `q79` `q8` `q80` `q81` `q83` `q84` `q85` `q87` `q88` `q9` `q90` `q91` `q92` `q93` `q94` `q95` `q97` `q99` |

`tpch/mixed-join` is CPU-only and the one query whose two engines differ: it plans, validates and
runs, and its recipes do not attach — [#168](../tickets.md#t168)'s interval `ScalarValue`, so the
crossing to a device is what fails, not the plan. `cpu_modes` carries all five and `gpu_modes`
none, which is the case the two mode arguments exist for.

The eighteen that stay out, every one against a ticket that already exists:

| held back | why | ticket |
|---|---|---|
| tpcds `q12` `q20` `q36` `q44` `q47` `q49` `q51` `q53` `q57` `q63` `q67` `q89` `q98` | the planner refuses `WindowAggExec` and `BoundedWindowAggExec` — the one capability the retired modes had and this one does not | [#143](../tickets.md#t143) |
| tpcds `q27` `q70` `q72` `q86` | DataFusion 45 does not physical-plan them at all (`plan_status=fail`) | [#23](../tickets.md#t23) |
| tpcds `q28` | a `DISTINCT` inside `count(DISTINCT …)`, refused by name | [#62](../tickets.md#t62) |

Window functions stay disabled and are not this task's to fix — [#143](../tickets.md#t143)
carries them.

**Seventeen of these hundred are already run by T17's tier, and the overlap is deliberate.**
`test_cpu_batch_partitioned` runs six queries at the five modes and eleven more at the modes plus
the injected shapes; every one of the seventeen falls in this task's list and none in T18's —
tpch `left-join` `q20` `q21` `shuffle-stddev` `anti-join` `nested-limits` `nested-loop-join`
`nested-loop-left-join`, tpcds `q38` `q87` `q2` `q8` `q16` `q33` `q45` `q93` `q97`. They are not
excluded, and a reader who finds the duplication should leave it.

The two tiers ask different questions of the same query: the injected one asks whether the
drivers tolerate a layout no planner would emit, over one plan; the corpus one asks whether the
answer, the plan and the costs match a golden, across the whole bench. Neither answers for the
other. And a corpus list defined by subtracting another list is a set held in prose — which is
exactly the defect T17a's completeness pass found in its own fixture list and closed by
generating `INJECTED` from one declaration. Reintroducing it one level up, so that removing a
query from the injected tier would silently need a matching addition here, buys a duplicate mode
run and costs the property that made the smaller list trustworthy.

**A device cell's ticket names its FIRST failure, not its only one.** The causes are ordered by how
far a plan gets: [#183](../tasks/active-tickets.md#t183) is the unload refusing an export, at the end of a plan
that ran, and [#152](../tickets.md#t152) is a join refusing its second probe batch, earlier. So a
mode whose join takes one probe batch reaches the unload and fails on the string, while a mode whose
join takes two never gets there — the same query reporting two different causes, chosen by the
partitioning rather than by its shape. `q18` is #152 at four modes and #183 at one.

Any count of cells per ticket is therefore a count of first failures. Fixing one moves cells to
another column rather than turning them green, and a cell's ticket is where a reader should start
rather than the whole of what stands in the way.

**The same shape appears on the cpu side, where the loser is not merely unmeasured but invisible.**
Three ordering questions have turned up in the rollout — #152 versus #183, #152 versus #175, and
#175 versus #189 on `tpcds/q77`, whose rollup made the hasher a candidate at exactly the three tp4
modes where the empty build side refuses first. A disabled cell runs nothing, so whether #189 would
also have rejected q77's grouping-set id cannot be read off the corpus at all; the question is not
open pending a measurement, it is closed to measurement until #175 is fixed. Any claim that a ticket
has been exhausted by the corpus should be read against that.

**The sixth batch showed the rule cleanly.** Four unrelated queries — a semi-join with a correlated
subquery, a five-way join, an eight-way join, an anti-join over a self-join — all fell the same way:
#183 at `bp-tp1-single` and #152 at the other four modes, without exception. `bp-tp1-single` is the
only mode whose joins happened to get a single probe batch, so it was the only one reaching the
unload — a correlation the eleventh batch retired, below.

The eighth batch narrowed that. #152 has two rows and only one is mode-sensitive: the build-side
copy is about a second probe batch erasing what the first consumed, so a single probe batch avoids
it, while the probe-side copy is refused at any batch count — one probe batch is still one copy.
Which half a query meets is a property of its JOIN TYPE, inner copying the build and outer the
probe: `tpcds/q97`'s outer join refuses at `bp-tp1-single` too and never reaches a second cause.

The eleventh batch replaced the mode shorthand with a batch-count rule — #152's build half fires
whenever a copying join's probe side arrives in more than one batch — and the fourteenth falsified
it. `tpcds/q11` at `bp-tp1-single` has eleven Inner joins, all copying, with probe batch counts
1,1,1,336,1,336,1,88,1,88,1 measured and recorded before the device ran; it reaches the unload and
fails on #183. A copying join took 336 probe batches at that mode and did not refuse.

So what decides it is **not known**. `tpcds/q71` and `tpcds/q2` refuse at `bp-tp1-single` and q11
does not, and all three have copying joins with streaming probe sides there — whatever separates
them is not something the goldens carry. The likeliest next question is what feeds the build side
rather than what feeds the probe, since a build that is another node's handle is consumed by its
first reader where one a scan re-materializes per call is not; that is readable in `cpp/src` and in
the recipe writer rather than in the corpus, and it belongs to #152 rather than to a rollout.

Three narrowings and one falsification, recorded because the alternative is a fourth version that
fits every batch so far. What survives is that #152's two halves differ by join type.

So the device column currently measures #152's reach rather than the engine's, and the two tickets
are ordered: fixing #152 moves most of those cells to #183 rather than turning them green, and #183
is one defect at one site. Those two, in that order, are what stand between this rollout and a
device column that is not empty — with the caveat the seventh batch added: where
`bp-tp1-single` gets past the join at all, it is not the mode that passes but the mode that gets far
enough to find out what else is wrong, and each query it reaches has its own answer. q7 fails there on a string, q2 on a decimal, q8 on an integer,
all at the unload. So #183 is not one defect at one site but one site with a family of type
mismatches under it, and #187 and #191 are two more of that family rather than separate work.

**A mode disabled after its file was last regenerated has no marker section**, and both golden
checks report it as an absence rather than as the disablement it is. The merge writes markers for
every declared section of a file, so regenerating any *enabled* query in that file fills the missing
ones in. Neither test's message says so, and a batch that only disables modes therefore looks
broken until an unrelated query is re-run.

**#163 is what decides where this rollout lands, and the number is knowable in advance.**
Seventeen of the fifty-six tpcds queries remaining after T19's ninth batch carry `avg`, whose count
state that ticket disables at every mode on both engines. So roughly a third of the tail produces no
cells at all: the ceiling is a hundred minus those seventeen, minus whatever else refuses whole,
which put it near **83** rather than near a hundred — with #163 being most of the gap, as
[#152](../tickets.md#t152) is most of the device column's. Neither is a rollout failure and neither
is fixable here.

**It closed at 68 fully enabled, 8 partially, 24 out entirely, 0 unreached** — 357 of 500 possible
cpu cells, and **zero** device cells beyond the six T18 left. So 76 rather than the 83 predicted, and
that miss is worth more than the number: the ceiling was computed twice, corrected from a wrong 88-to-90
down to 83, and was still high, because both computations counted only the blockers known at the
time. [#190](../tasks/active-tickets.md#t190) and [#192](../tasks/active-tickets.md#t192) arrived after the projection. **A
ceiling derived from known causes is a bound on optimism, not a prediction** — unknown blockers only
ever subtract.

Seventeen `avg` queries were projected against #163 after the eleventh batch and all seventeen were
RUN rather than declared. None failed to refuse and none refused for a different reason, which is
what makes the tail a measurement rather than an assumption.

**Roll out in batches of about five, not in one sweep.** A hundred queries enabled at once is a
regeneration whose diff nobody reads and a failure nobody can attribute: eleven golden files move
either way, so the batch size is the only thing that says which query moved which section. Five
is small enough that a red run names its cause without bisecting and large enough that the
regeneration cost is amortised. Each batch is its own commit — the five queries, their registry
cells, the sections they filled, and whatever went to `active-tickets.md` — so the history reads as
the rollout it was, and a batch that goes wrong is reverted without taking the ninety-five with
it.

Nothing else, and the list is still short enough to read off a diff: no `cost_model.conf`,
because T18 enters all eighteen node kinds at once rather than as each is first seen — an
exhaustive set entered piecemeal is one whose next gap is a rollout's problem; no `pipeline.yml` or `test_ci_coverage`,
because the targets exist by then; no `registry.rs`, whose columns are T18's.

That sweep is where T17's `nested-loop-left-join` gets its GPU columns. They ship `na` because
T17 could not commit a device run, which is [#116](../tickets.md#t116)'s shape — a cell with no
coverage and no blocker — so the reason is recorded here rather than left for a reader to
reconstruct from an empty column.

**Four tasks were specced during T19 and none of them is T20's.** They came out of what the rollout
found, and each closes tickets the device column is actually blocked on rather than shapes the corpus
lacks: [`refcounted-tables.md`](../tasks/refcounted-tables.md) for [#145](../tickets.md#t145) and
[#152](../tickets.md#t152), [`casts.md`](../tasks/casts.md) for [#183](../tasks/active-tickets.md#t183),
[`wire-schema.md`](../tasks/wire-schema.md) for [#187](../tasks/active-tickets.md#t187), and
[`empty-answers.md`](../tasks/empty-answers.md) for [#173](../tickets.md#t173) and
[#175](../tickets.md#t175). Order matters twice: `casts.md` before `wire-schema.md`, which writes the
schema `empty-answers.md` then reads. Between them they cover the two causes that hold 134 of the 138
disabled device rows.

**T20 — corpus shapes the benchmarks do not have.** Not done, and not a task any more: it is
[#195](../tickets.md#t195), which carries the audit, the six shapes and the engine work each
one needs.

**T22 is obsolete as written (2026-09-08).** `peacock_gpu_benchmarks` and the record tree it
wrote were deleted with the legacy modes, so there is nothing to port: a per-node measurement
for this mode starts from the protocol below rather than from that harness. The design
question it names — a node runs once per batch per lane, so a per-node figure is a sum over
calls and the call count belongs beside it — is the part worth keeping.

**T22 — per-node benchmarks for the new mode.** Port `peacock_gpu_benchmarks` to the
batch-partitioned executor, keeping the protocol that makes its numbers comparable: one
discarded warm-up, ten measured runs, the **2nd-smallest by `total_us`** reported whole, and
the floor measured over 200 samples. The run counts stay compile-time constants
(`tests/common/mod.rs`) so every record in the tree was taken at the same ones.

**A node is called many times here, and that is the port's one real design question.** The
legacy record carries one `time_us` per node, or one per partition — a node runs once. In
this mode a node runs once per batch per lane, so a per-node figure is a *sum over calls*
and the call count belongs beside it; a node at 40 ms over 200 calls and one at 40 ms over
two are different findings, and a record that cannot tell them apart measures nothing useful.
`CallStats` is already returned per call and is where the per-call figures come from.

The record gains the mode and the tier — `<query>.<mode>.benchmark.txt` alongside the legacy
`<query>.<mode>-<tp>-<tier>` — and keeps `build_profile`, `sync_floor_us`,
`nodes_at_or_below_floor` and the rest, since they mean the same thing. It also carries which
allocator measured the run: the pool landed with
[#151](../archive/archived-tickets.md#t151), [#148](../tickets.md#t148) is still open, and a
number taken without one is not comparable with a number taken with one.

Case list as the correctness tiers use, so the measured set cannot drift from the verified
one, and `test_ci_coverage` exempts the target explicitly because it asserts nothing.

---

<!-- archived from llm-wiki/tasks/schema_and_validation.md -->

# schema registry and validation (T7/T8 remainder)

**Goal.** Finish the two tasks whose implementations landed early on
`ENS-bp-plan-skeleton` but whose test surfaces did not: prove the `Schema` carried on every
node is right, and make `validate_schemas_and_partitions` a check that can go red for the
reasons it claims rather than only for the ones a real plan happens to hit.

**What already landed, so nobody rebuilds it.** `Schema` exists with `group_keys` and
`agg_state`, every node carries a populated one, the plan goldens print the declared type per
column, and node-local validation is called from `plan_batch_partitioned` with all ten
goldens passing it node by node. That half was pulled forward by a review finding — ten
guards existed and none ran on the live path.

## T7 — what the schema tests must show

- A hand-built plan carrying a project, an aggregate and a union produces the expected types
  and the expected semantics annotations, asserted on the tree rather than on rendered text.
- The annotations survive the aggregate sequence: `agg_state` is right at the init, at the
  per-lane merge and at the finalizing merge, which are three schemas for one logical
  aggregate.
- **Decimal precision and scale through project, aggregate and union-cast.** This is the
  one that earns the task: `avg`'s state columns were once typed backwards and per-node
  bytes could not show it, because both engines derive them from the same plan schema — so
  CPU and GPU agreed on the same wrong number and only a real divide would have diverged.

## T8 — what validation still owes

- The generic structural pass, over and above the per-node checks that exist.
- Manually constructed wrong combinations: each rule turned red by an input built to break
  it, per the reviewer's anchor that a guard which cannot go red is not a guard.
- Validation run over every canonized corpus plan as a standing check, not as a one-off.
- Defects in the checks themselves, including any the reviewer reported against
  `ENS-bp-plan-skeleton` and I deferred here.

**Constraints.** Every committed golden plan passes the validation this task adds. A
rejection is a planner defect until shown otherwise: stop, report it, and fix planning —
never weaken the check to fit the plan, and never regenerate a golden to silence one. The
expectation is still that no plan moves, so a golden that does move is a deliberate decision
taken with the human rather than a side effect of the regen. A test that only passes is worth
less than one shown to fail on the defect it guards.

**Verification bar.** Every rule in `validate_schemas_and_partitions` has an input that turns
it red. Decimal fidelity is asserted at each step of project → aggregate → union-cast. Every
committed golden plan passes validation, and a golden moves only where a planner fix required
it. `test_ci_coverage` names whatever targets appear.


Merged 2026-08-18 as PR #121.


---

<!-- archived from llm-wiki/tasks/join_capability_recipe.md -->

# join capability through the recipe plan (T0 extension)

**Goal.** Establish, by execution rather than argument, that the frozen FlatBuffers schema
and C++ operators can run **every** join mode in the batch-partitioned model — with the
build side complete and the probe side arriving in batches — and write the per-mode
lowering down where the implementation tasks will read it.

**What was built** (all in `scripts/exec_model/`, coordinator-owned):

- `operators/join_types.py` — `JoinType` in the fbs vocabulary and the capability matrix as
  one function, so no backend can hold a different opinion of it.
- `operators/cudf_calls.py` — the cuDF calls `cpp/src/operators/join.cpp` makes, at their
  own signatures: joins that return gather maps, `gather` with its out-of-bounds policy,
  `scatter`, `apply_boolean_mask`, `cross_join`.
- `operators/recipe.py` — the fb node structs, a handle registry that consumes on read as
  `NodeSession` does, and the node implementations mirroring join.cpp branch for branch.
- `operators/recipe_join.py` — the second join backend: every call answered by emitting fb
  seqs and making `execute_node` calls.
- `operators/joins.py` — the pandas backend, widened from five join types to all nine plus
  cross and nested loop.

**Constraints.** The two backends share no join code; agreement between them is the
evidence. The recipe backend may not reach for python where the frozen surface has no
answer — it names the gap and counts what working around it costs (`copy_handle`).

**Verification bar.** `scripts/exec_model/tests/test_end_to_end.py`: every join mode
against a SQL oracle, on both backends, at five batching/partitioning configs and across
the layout injector's presets; the emitted seq sequence asserted per mode against the
spec's table; the per-batch copy counts asserted per family; every refused shape refused
loudly on both backends. Whole prototype suite green, ~90 s.

**Outcome.** Recorded in the spec's [join capability
matrix](../architecture.md#capability-matrix): every mode is expressible,
the streamed-probe copy cost is [#152](../tickets.md#t152) quantified per family, and one
shape turned out to be a defect in the shipping engine rather than a limit of the mode —
[#153](../tickets.md#t153).


---

Merged 2026-08-04 as PRs #112 / #115 / #114.


---

<!-- archived from llm-wiki/tasks/build-test-flags.md -->

# Task: build-test.sh flag surface, failure semantics, and the regen guard

On `ENS-test-exec-mode`. Touches `scripts/build-test.sh`, `scripts/build-test-shadgpu.sh`,
`peacockdb-core/tests/test_plan_bytes.rs`, `llm-wiki/build-test.md`. The rules this
implements are already in `coding-style.md` ("Bash: the flag set is an interface, and
failure is fatal", b05a631) — that bullet was written from this script's defects.

No golden may move. No test may be added, removed or re-tiered.

## A — flag surface

1. **Delete `--cpu`.** It sets the default and has no callers anywhere in the repo.

2. **`--gpu` and `--rust-only` are mutually exclusive** and must be *rejected*, not
   resolved by argument order. Today:
   - `--rust-only --gpu` → `MODE=gpu` with `RUST_ONLY=1` still live, so the run branch
     sets `LD_ENV=":"` and the GPU binaries never get `LD_LIBRARY_PATH` — they fail to
     resolve `libpeacock_gpu` and it reads as a product fault.
   - `--gpu --rust-only` → the reverse, GPU silently ignored.
   Same two flags, opposite outcomes, no warning. Error naming the contradiction.

3. **Replace `--push-testdata` / `--pull-testdata KIND[,KIND]`** with per-kind flags:

       --push-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}
       --pull-{parquet,queries,goldens,duckdb-profiles,duckdb-dynfilters}

   The point is structural, not cosmetic: the argument parser *becomes* the validator.
   No comma splitting, no kind lookup at the call site, and an unknown kind is just an
   unknown flag caught by the existing `*) usage` arm before any side effect. The
   multi-kind form is generality nobody uses — every documented invocation moves exactly
   one kind. `testdata_dirs_for_kind` stays as the kind→dirs map; only its use as a
   validator goes.

4. **`--fetch-goldens` → `--pull-goldens`.** A rename, not an alias. Today it appends
   `goldens` to `PULL_TESTDATA`, so passing it alongside `--pull-testdata goldens` pulls
   twice. Keep the current ordering property: the flag must be resolved *before* the
   `--host` requirement check, so `--pull-goldens` alone still demands `--host`.

5. **`--rsync` → `--push-binaries`, in BOTH scripts.** `--rsync` names the tool rather
   than the intent. `build-test.sh`'s own first line says it mirrors
   `build-test-shadgpu.sh`, so renaming one and not the other breaks the parallel that
   makes either readable after time away.
   - It **still pushes goldens.** They are part of the payload, not an optional kind:
     binaries shipped without the fixtures they assert against is the trap that produced
     110/110 "canonical file not found". Requiring `--push-goldens` alongside would
     replace a footgun with a guarded footgun.
   - `--push-goldens` remains useful and is not redundant with it — it is the subset
     operation, refreshing fixtures without rebuilding or reshipping binaries.

6. **Require an action.** `--host x` with no `--build`/`--push-binaries`/`--run`/push/pull
   currently does nothing and exits 0.

7. **Value-taking flags check their value exists.**

8. **`usage()` states what is deliberately absent**, so the next reader does not file it
   as a gap:
   - no `--pull-binaries` — binaries flow one way, built locally and shipped;
   - `embeddings-cache` is a per-host intermediate for the tpch vector datasets
     (`fetch_embeddings.sh`, ~1.8 GB, gitignored) and is deliberately not syncable.
   Also state the mode ladder: rust-only ⊂ cpu ⊂ gpu, and that a mode which builds more
   never runs less.

## B — failure semantics

1. **`set -euo pipefail`.** `pipefail` is load-bearing: `cargo test --no-run … | python3`
   currently takes python's status, so a cargo failure is caught only by the explicit
   emptiness check afterwards.

2. **An empty derived suite is an error.** Verified: `mapfile -t A < <(helper)` with a
   helper that outputs nothing yields a zero-length array, the `for` body never runs, and
   the script exits 0. A typo in the derivation would silently run no tests and report
   success.

3. **All validation before the first side effect** — a bad flag must fail before anything
   is built, shipped or deleted.

4. **The remote heredoc keeps its deliberate `set -e` omission.** Running every test
   binary and accumulating `rc` is correct: a failing C++ test must not skip the Rust
   ones. This is the stated exception `coding-style.md` allows; keep the comment that
   says why, and keep the non-zero exit at the end.

## C — deduplicate the sync layer

1. **One `sync_goldens()`.** "Push goldens" exists twice with different flags: the
   `--push-binaries` block uses `rsync -r --delete`, `--push-goldens` uses
   `rsync -a --delete`. Same intent, different metadata handling, no shared code.

2. **Push mirrors (`--delete`) uniformly; pull is additive uniformly** — and the
   asymmetry is deliberate, so document it rather than "fixing" it. The remote is a
   *partial* mirror: `testdata/goldens/` contains `tpch.sf40/` (16 CSVs) and sf40 lives
   on shad-gpu, so mirroring downward from verda would delete fixtures that host never
   had. The destination is a git working tree.

   Known consequence, accepted: a regen deletes a `.result.txt` when a result exceeds
   256 KB (`maybe_write_result_golden`), and an additive pull cannot propagate that. The
   deletion is already announced on stderr and reaches the operator through the ssh
   heredoc — that is the handling, not a `--delete` flag armed for one rare case.

3. **Drop the `*.txt` filter on the goldens pull** — after A/B and D, not before. Its real
   job is keeping `plan_bytes.sha256` out of the round trip, which becomes the self-guard's
   job in D; keeping both would be two mechanisms for one invariant with the weaker one in
   the wrong place. Removing it also stops silently dropping the 16 sf40 CSVs.

4. **Clear `cpp/install/rust-tests` before staging**, matching `build-test-shadgpu.sh`.
   `build-test.sh` does not, so orphaned binaries from a previous mode accumulate;
   today that is mitigated only by running binaries by explicit name.

## D — move the regen guard into the test

`--update-canonical` exports `UPDATE_CANONICAL=1` to every staged binary, so the run set
doubles as the regen set and `regen_excluded()` subtracts `test_plan_bytes` back out.
That protects one invocation path only: `UPDATE_CANONICAL=1 cargo test --features
rust-only -p peacockdb-core --test test_plan_bytes` — the exact command the golden's own
header prints — still rewrites `plan_bytes.sha256` silently.

Move the refusal into `test_plan_bytes.rs`: under `UPDATE_CANONICAL`, refuse unless a
dedicated override (`PEACOCK_REGEN_PLAN_BYTES=1`) is also set, panicking with the reason —
the digests are the wire-format guard, the C++ side reads those bytes, and regenerating
rewrites the evidence instead of failing. Then **delete `regen_excluded()`**; the script
stops carrying knowledge about a test's internals.

`test_cost_model` stays in the regen set. That inclusion is a fix, not a risk: `.cost.txt`
derives from `.cpu.txt`, which a regen rewrites, so the old six-target list left every
`.cost.txt` stale and `test_cost_model` went red immediately after a "successful" regen.

## E — comments and docs

- Header comment says "Two suites" and then lists three.
- The `PUSH_TESTDATA` comment lists four kinds; `usage()` lists five (`duckdb-dynfilters`).
- The goldens-push comment justifies itself entirely in verify terms and disposes of the
  regen case in a parenthetical, which reads as "this push is redundant when
  regenerating" — the opposite of what is true. State both reasons: for a verify run the
  binaries assert against these files; for a regen the push establishes the baseline, so
  the pulled-back set is local-committed ∪ regenerated rather than remote-leftovers ∪
  regenerated, and `--delete` is the mechanism.
- `build-test-shadgpu.sh:176` references "the goldens that build-test.sh's --rust-only
  mode used to skip" — check it still says something true.
- `llm-wiki/build-test.md`: the verda row, the shad-gpu row (`--rsync` → `--push-binaries`),
  the golden-regen bullet, and a line recording that `embeddings-cache` is not syncable.

## Sequencing

**A+B → D → C → E.** A/B is the interface and the failure model and touches everything
later; D must land before C3; C is mechanical once D is in; E last, so the docs describe
the final state rather than an intermediate one.

## Verification

- `bash -n` on both scripts.
- A flag matrix: for each mode, print the derived suite and assert it matches; for each
  rejected combination, assert it errors non-zero. Include `--gpu --rust-only` in both
  orders, `--host` with no action, and a value-taking flag with no value.
- Prove the empty-suite error fires (temporarily break the derivation, confirm non-zero,
  restore).
- One real `--rust-only --build` to prove staging still produces binaries.
- `git status --porcelain testdata/goldens` must be empty at the end.

## Out of scope

`maybe_write_result_golden`'s discarded `remove_file` result and its unconditional "no
golden" message — a ticket, not this task. The `--cpu`/`--rust-only` naming axis is
resolved by A1/A2 and needs no further rename.


---

<!-- archived from llm-wiki/tasks/widget-golden-links.md -->

# Task: link CPU ✓ cells to their goldens; small-font Query and Σout columns

Branch `ENS-widget-golden-links` (off `ENS-test-exec-mode`). Cost-report widget only —
`cost-report/src/main.rs`. No changes to the registry CSV, the test suite, or any golden.

## 1. The premise, verified

Every `enabled` cell in `ftc_tp1`, `ftc_tp8` and `partitioned_cpu` **has a committed
`.cpu.txt`**. `assert_cpu_cost_canonical` runs unconditionally on every CPU macro
invocation (`common/mod.rs:847`), before and independent of the `ResultGolden` keyword —
that keyword gates only `.result.txt`. So a `✓` in those three columns always has a
golden to point at.

This does *not* extend to the GPU columns: those read the CPU golden rather than owning
one, so leave `full_table_gpu` / `partitioned_gpu` cells alone.

## 2. Which golden each ✓ points at

The device label is **not** in the CSV, and it is not one-per-column:

| Column | Golden label |
|---|---|
| `ftc_tp8` | `full_table-tp8-mini` |
| `partitioned_cpu` | `partitioned-tp8-standard` |
| `ftc_tp1` | `full_table-tp1-standard` |

**Corrected 2026-08-04.** This section originally said `ftc_tp1` is tp1-standard "except
`scan_limit`, which is `full_table-tp1-mini`". That is wrong, and the developer caught it:
`scan_limit` has **both** goldens. It is registered twice — `tp1_mini` at
`test_cpu_full_table.rs:24` and `tp1_standard` at `:188` — and `column_for` keys on the tp
count, not the memory tier, so both land in the single `ftc_tp1` cell. It is not the
exception to the column's label; it is the one query whose cell aggregates two runs.

**Decision: one label per column, `full_table-tp1-standard`.** It is the label every tp1
row uses including `scan_limit`, so the link target is predictable from the column alone.
Drop the tp1-mini candidate rather than carrying config that nothing can reach — this repo
already treats unreachable config as a hazard in its own right (`GOLDEN_INVARIANT_EXEMPT`
and `INTENTIONALLY_NOT_IN_CI` both carry staleness assertions for exactly that).

What a cell aggregating two runs should render is a real question, but a hyperlink cannot
express it and the answer would change the cell shape. Out of scope here; raise it as its
own task if the tp1-mini run ever needs to be reachable from the widget.

The fail-loud check below is what makes dropping the candidate safe: a future query
registered ONLY at tp1-mini has no golden under the single label, so the widget fails
naming it instead of rendering a dead link. That is strictly better than a silent second
candidate, because it forces the decision rather than guessing.

Link target: the same `links.golden_url(canon_rel, stem, "<label>.cpu.txt")` helper the
Σout cell uses, so dry runs with no sha degrade to plain text exactly as they do today.

Only `✓` becomes a link. `~`, `✗` and `—` stay plain — there is no golden behind them.

## 3. Small font

- **Query column:** non-numeric queries only — `aggregate_groupby`, `scan_limit`,
  `shuffle_stddev`, `hash_join`, … The discriminator already exists: `Row::number` is
  `None` for exactly these (it is `Some(n)` for `q<N>`). Numbered queries keep today's
  size.
- **PeacockDB Σout and DuckDB Σout:** small font for **both the header and the values**.
  The Ratio column is not in scope.

Both renders. They use different mechanisms and the difference is load-bearing: the HTML
report can use a CSS class (`th.modeh` is the precedent), while the PR-comment table must
use `<sub>` because GitHub strips `class`/`style` — see the `mode_cells_md` doc comment.

## 4. Watch for

- `MODE_COLUMNS` and `registry.rs::COLUMNS` are the CSV header contract. This task is
  display-only: do not touch either, and do not change any cell value.
- `ftc_cell()` currently renders `tp1✓ tp8✓` as one string inside a single `<td>`. Both
  glyphs need to become independently linkable, so that function has to return markup
  rather than a plain label — check its callers in both renders before changing its shape.
- `CPU_DEVICE` is the const the Σout cells resolve through. The new per-column labels are
  related but not the same thing; do not fold them together in a way that makes a Σout
  change silently move a mode link.

## 5. Verification

- `cargo test -p cost-report` — the widget's own unit tests, including the
  `mode_cells_md` shape asserts.
- `scripts/cost-report-preview.sh` and eyeball the generated HTML: a linked `✓` per
  enabled CPU cell, `scan_limit`'s tp1 link resolving to `full_table-tp1-mini`, small-font
  micro-query names, small-font Σout headers and values.
- Confirm the PR-comment render still parses as HTML on GitHub (`<sub>`, no class/style).
- No golden, CSV or test-suite file appears in `git status`.


---

<!-- archived from llm-wiki/tasks/test-exec-mode.md -->

# Task: execution mode explicit in test macros and golden filenames

Branch `ENS-test-exec-mode` (off `ENS-llm-wiki`). Pure refactor: **no behavior change, no
golden regeneration, no coverage change**. Every test that runs today must still run, with
the same assertions, reading the same bytes from renamed files.

## Why

Two things are inferred today that should be stated:

1. `common/mod.rs::partition_mode(device)` maps the string `"tp8-standard"` to
   `PartitionMode::RealMultiPartition` and everything else to `SinglePartition`. A device
   label silently picks an executor. Adding a device (say `tp8-mini` as a real 8-way tier,
   which #91 wants) would route it to the wrong executor with no diff to the routing code.
2. Golden filenames carry only `tp<N>-<tier>`, so `q15.tp8-mini.cpu.txt` does not say which
   CPU executor produced it. The mode is real: the same plan at tp8 produces a different
   per-node cost tree under full-table vs partitioned execution.

`node13` is an obsolete task number used as an executor name. It goes.

## 1. Macro renames

| Old | New |
|---|---|
| `cpu_result_test!` | `cpu_full_table_result_test!` |
| `cpu_result_approx_test!` | `cpu_full_table_result_approx_test!` |
| `cpu_node13_result_test!` | `cpu_partitioned_result_test!` |
| `cpu_node13_result_approx_test!` | `cpu_partitioned_result_approx_test!` |
| `gpu_test!` | `gpu_full_table_test!` **and** `gpu_partitioned_test!` |

`cpu_result_error_test!` / `cpu_result_fits_test!` keep their names — decision, not
oversight: they take a raw budget rather than a device, read no mode-tagged golden, and the
resident-OOM enforcer they drive is full-table-only by construction (#91 tracks porting it).
Say that in a one-line comment where they are defined so the asymmetry doesn't read as a
miss.

Generated test-fn names include the mode, so two modes at the same device can never collide:
`cpu_full_table_tpch_sf1_q1_tp8_mini`, `cpu_partitioned_tpch_sf1_q6_tp8_standard`,
`gpu_full_table_tpch_sf1_q1_full_table_tp1_standard`. Grep `pipeline.yml` and the
`build-test*.sh` scripts for `--exact` / filter strings built from the old names and fix them.

## 2. Call-site shapes

CPU — mode from the macro name, device stays `tp<N>_<tier>`:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, no_result_golden);
    cpu_partitioned_result_test!(tpch, 1, q6, tp8_standard, result_golden);

GPU — mode from the macro name; the device argument is the **combined golden label**, which
is the golden filename component verbatim:

    gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, golden_exact);
    gpu_partitioned_test!(tpch, 1, q3, partitioned_tp8_standard, golden_exact);

So `gpu_full_table_test!(tpch, 1, q1, full_table_tp1_standard, …)` reads
`goldens/tpch.sf1/q1.full_table-tp1-standard.{cpu,result}.txt` — reconstructible from the
invocation with no lookup. The GPU run config splits cleanly: `PartitionMode` comes from the
macro name, `tp` + budget are parsed out of the label. A crossed pair
(`gpu_full_table_test!` with a `partitioned_…` label) is visible at the call site; assert it
cannot pass silently — the label's mode prefix must equal the macro's mode.

## 3. Last parameter: bool → enum

The trailing `$gen:literal` bool becomes an ident backed by a real enum in `common/mod.rs`,
so the call site names the artifact it produces:

    pub enum ResultGolden { Write, Skip }   // `result_golden` | `no_result_golden`

Mirror `gpu_result_mode`'s shape (keyword → enum, unknown keyword panics with the accepted
set). Keep the existing invariant comment about when writing is correct — it is the useful
part of that parameter.

## 4. Delete the implicit routing

- Delete `partition_mode(device: &str)` outright.
- `assert_cpu_results_match_datafusion`: replace `use_node13: bool` with an explicit
  execution-mode parameter that carries both the executor choice and the `PartitionMode`.
- `assert_gpu_query` / `assert_gpu_nodes_match_golden`: take the `PartitionMode` from the
  caller (the macro), never from a label.
- `plan_is_node13_executable` → `plan_is_partitioned_executable`, unchanged behavior; it
  stays the safety assert on the partitioned path.
- `registry.rs::column_for`: kind `"node13"` → `"partitioned"`; kind `"gpu"` splits into
  `"gpu_full_table"` / `"gpu_partitioned"` mapping straight to their columns instead of
  sniffing `device.starts_with("tp1")`. The `ftc` arm keeps its tp1/tp8 split — that one
  reads a `tp` count out of a `tp` label, which is parsing, not routing.

### The plan tier keeps a label → mode lookup (amended 2026-08-04)

"Delete `partition_mode` outright" was wrong about the plan tier, and the developer caught
it. `test_plan_bytes.rs::corpus()` builds its entire corpus by reading `.plan.txt` filenames
off disk and splitting the device out of the stem — there is no call site to state a mode at,
and `plan_bytes.sha256` keys on `<query>.<device>`, which this task freezes. The mode is
load-bearing there: `shuffle-additive` @ tp8-standard is the one plan golden whose shape
depends on `RealMultiPartition`.

So: `partition_mode` is deleted from the **execution** path — CPU results, GPU query, GPU
nodes, and the oracle `CpuExecutor` all take the mode from the macro name. One narrowly
scoped `plan_partition_mode(device)` survives for the plan tier, documented as that tier's
label contract and made **exhaustive** — an unknown label panics instead of falling through
to `SinglePartition`. That catch-all is the actual hazard this task's "Why" names, and
because `corpus()` reads labels off disk, exhaustiveness is self-enforcing: a new plan golden
with an unlisted device fails loudly instead of silently planning single-partition.

Keep it to **one** mechanism. Do not also add an explicit mode argument to `query_plan_test!`
or `plan_for` — two sources for the same fact is how a plan golden gets generated under one
mode and its byte digest under the other. Concretely: `test_query_plan_misc.rs:27`
(`plan_for(…, "tp8-standard")`) is unchanged, while `test_cpu_executor_misc.rs:41` is an
execution-tier site and does pass `PartitionMode::RealMultiPartition` explicitly.

The `coding-style.md` entry names this as a deliberate exception, so the surviving function
reads as scoped rather than missed.

**CSV columns and `testdata/cost-registry.csv` do not change.** `COLUMNS` is the committed
fixture's header contract (`load_csv` asserts on it) and `MODE_COLUMNS` in cost-report keys
off the same names. Renaming `ftc_tp1` is a separate change; not this one.

## 5. Golden filenames

New: `<query>.<mode>-<tp>-<tier>.{cpu,cost,result}.txt`, e.g.
`q15.full_table-tp8-mini.cpu.txt`.

**Unchanged:** `.plan.txt` (plan shape is executor-independent, and
`goldens/plan_bytes.sha256` pins those names) and `.duckdb_cost.txt` (oracle, no peacock
executor involved).

`git mv` only — do not regenerate. The mapping is total and collision-free because no device
label is used by both modes today (verified: `tp8_standard` appears only under
`cpu_node13_*`; every other label only under `cpu_result_*`):

| Old | New | files |
|---|---|---|
| `*.tp8-mini.{cpu,cost}.txt` | `*.full_table-tp8-mini.…` | 129 + 129 |
| `*.tp1-mini.{cpu,cost}.txt` | `*.full_table-tp1-mini.…` | 1 + 1 |
| `*.tp1-standard.{cpu,cost,result}.txt` | `*.full_table-tp1-standard.…` | 110 + 110 + 104 |
| `*.tp8-standard.{cpu,cost,result}.txt` | `*.partitioned-tp8-standard.…` | 18 + 18 + 14 |

634 files total across `testdata/goldens/{tpch.sf1,tpcds.sf1}`. Counts are mine and worth
re-deriving — if yours differ, say so before renaming rather than after. Verify content is
untouched: the multiset of file *contents* must be identical before and after (e.g. compare
sorted `sha256sum` values of the renamed set).

## 6. File reorganization — by mode, not by memory tier

| Now | Becomes |
|---|---|
| `test_cpu_executor.rs` (tp8-mini + tp1-mini + tp8-standard) | `test_cpu_full_table.rs` (tp8-mini, tp1-mini, tp1-standard) |
| `test_cpu_h200.rs` (tp1-standard) | `test_cpu_partitioned.rs` (tp8-standard) |
| `test_gpu.rs` | `test_gpu_full_table.rs` + `test_gpu_partitioned.rs` |

`test_cpu_executor_misc.rs` / `test_gpu_executor_misc.rs` keep their names; they only need
their golden paths updated.

Registry ownership follows the files — update both the `registry_matches_csv_*` fns and their
doc comments:

- `test_cpu_full_table.rs` → `ftc_tp1` + `ftc_tp8`
- `test_cpu_partitioned.rs` → `partitioned_cpu`
- `test_gpu_full_table.rs` → `full_table_gpu`
- `test_gpu_partitioned.rs` → `partitioned_gpu`

The cross-binary caveat in `test_cpu_h200.rs`'s doc comment (scan_limit registered at
tp1-mini in one binary, tp1-standard in another) is moot once both live in
`test_cpu_full_table.rs` — delete it rather than carrying it forward. The note at the foot of
`test_gpu.rs` explaining why the cross-mode invariant lives in `test_query_plan.rs` is still
load-bearing; keep it on whichever GPU file you consider primary.

`test_gpu.rs`'s file-level doc explains the merged one-run-asserts-both design — that belongs
in both new GPU files or in the macro doc, not dropped.

## 7. Consumers that read the old names

- `cost-report/src/main.rs`: `CPU_DEVICE = "tp8-mini"` → `"full_table-tp8-mini"`. Check the
  surrounding comment about scan_limit being tp1-mini-only (~L345) — it still holds, but its
  wording names devices.
- `registry.rs::assert_cross_mode_golden_invariant`: the `("full_table_gpu", "tp1-standard")`
  / `("partitioned_gpu", "tp8-standard")` pairs become the new labels. This one is
  load-bearing — it is what catches an enabled GPU mode with no CPU golden.
- `test_ci_coverage.rs`: four new test-target names in, two out. This gate is the reason a
  renamed binary can't silently drop out of CI.
- `.github/workflows/pipeline.yml`: rust test target lists in the cpu-cpu tier and the
  GPU-remote job; `scripts/build-test.sh` and `scripts/build-test-shadgpu.sh` build/stage
  test binaries one `--test` at a time and name them.
- `test_cost_model.rs` globs `*.cpu.txt` and derives the sibling `.cost.txt` by string
  replace — should need no change; confirm rather than assume.
- `llm-wiki/build-test.md` and `architecture.md`: test-file names, device labels, golden
  naming. Same commit.

## 8. Wiki changes owed by this task

`coding-style.md` — new antipattern entry, in the voice of the existing thread-local one:
implicit routing from a label. `partition_mode(device)` turned the string `"tp8-standard"`
into `RealMultiPartition`; the executor a test ran was a side effect of how its golden was
named, and a new device label would have silently taken the wrong path with no diff to the
routing code. State the mode at the call site and pass it as a parameter.

`build-test.md` — one sentence: a refactor that must not change behavior is verified with a
representative subset (one query per mode/tier per binary) plus the full rust-only tier, not
a full CPU/GPU suite run; the goldens are the invariant.

## 9. Verification — subset only, explicitly

Do **not** run the full CPU or GPU suite.

1. The rust-only **golden/meta gates**, must be green — named by target, not by package:

       cargo test --features rust-only -p peacockdb-core \
         --test test_plan_bytes --test test_cost_model --test test_ci_coverage

   plus the registry tests, which live inside the execution targets and must be filtered to
   (`--test test_query_plan -- registry_`, and the same for `test_cpu_full_table` /
   `test_cpu_partitioned`). Together: the CSV contract, the cross-mode golden invariant, and
   the CI-coverage gate. Cheapest and highest-value gate for this change; seconds, not
   minutes.

   **Corrected 2026-08-04** — this item originally read `cargo test --features rust-only -p
   peacockdb-core` with no `--test`, which contradicted this section's own headline: nothing
   cfg's the CPU execution targets out of the rust-only build, so package-wide sweeps all 241
   + 18 of them. `--features rust-only` selects a *build*, not a tier; only `--test` selects
   the tier. `build-test.md`'s table lists the bare package command as the rust-only loop,
   which is what made this easy to mis-transcribe — the §8 sentence owed to `build-test.md`
   should make the distinction explicit rather than just saying "run a subset".
2. `cargo build --tests` clean, no new warnings.
3. CPU subset — a few per binary, chosen to cover each device label:
   `test_cpu_full_table` at tp8-mini, tp1-mini (scan_limit), tp1-standard;
   `test_cpu_partitioned` at tp8-standard (include one approx: tpcds q17).
4. GPU subset on shad-gpu — one per new binary is enough (e.g. tpch q1 full-table, tpch q6
   partitioned). This proves golden-path resolution on both binaries; it is not a
   correctness run.
5. **Test-count invariant**: capture `--list` output for the affected binaries before and
   after and show the sets correspond 1:1 under the intended renames. A refactor that
   silently drops a test is the failure mode here, and neither a green subset nor
   `test_ci_coverage` alone would catch a dropped `gpu_test!` line.

## Out of scope

CSV column renames; `ftc` as a kind name; porting the resident-OOM enforcer to the
partitioned driver (#91); regenerating any golden; re-tiering any test.

## 10. Follow-up (added 2026-08-04): fold the approx variants into an oracle argument

Human's instruction, on this same task rather than a new one.

The `_approx_` macros exist only to pass `Some(1e-12)` instead of `None` for `rel_tol`.
That is a property of how the result is compared, not a different kind of test, and
spelling it in the macro name means two names per mode where one plus an argument says
more. Delete `cpu_full_table_result_approx_test!` and `cpu_partitioned_result_approx_test!`
and add a **second-to-last** argument to the two surviving macros:

    cpu_full_table_result_test!(tpch, 1, q1, tp8_mini, data_fusion_exact, no_result_golden);
    cpu_partitioned_result_test!(tpcds, 1, q17, tp8_standard, data_fusion_approximate, result_golden);

Backed by a real enum in `common/exec_mode.rs`, keyword-mapped like `ResultGolden` and
`gpu_result_mode` (unknown keyword panics naming the accepted set):

    pub enum CpuOracle { DataFusionExact, DataFusionApproximate }

`DataFusionExact` → `rel_tol = None`, `DataFusionApproximate` → `Some(1e-12)`.

The name states what the oracle IS, which the old name did not: **both** variants compare
against a live plain-DataFusion run at `target_partitions = 1` (`build_session_state(1)`);
only the float tolerance differs. Nothing about the oracle changes — this is a rename of
an existing bool-in-disguise.

Move the 1e-12 rationale — float summation reassociates across partitions at tp>1, ~1 ULP,
while the `output_bytes` cost golden stays exact because a ULP does not change byte width —
onto the `DataFusionApproximate` variant, where `ResultGolden` and `GpuResultMode` keep
theirs. Do not leave it stranded on a deleted macro.

**The five call sites that become `data_fusion_approximate`** (every other CPU call site
takes `data_fusion_exact`):

| File | Query |
|---|---|
| `test_cpu_full_table.rs:65` | tpch `shuffle_stddev` @ tp8-mini |
| `test_cpu_full_table.rs:85` | tpcds `q14` @ tp8-mini |
| `test_cpu_full_table.rs:109` | tpcds `q39` @ tp8-mini |
| `test_cpu_full_table.rs:226` | tpch `shuffle_stddev` @ tp1-standard |
| `test_cpu_partitioned.rs:38` | tpcds `q17` @ tp8-standard |

Note `tpcds q14 @ tp1-standard` is **exact** today and stays exact — at tp1 there is no
reassociation. Converting it along with its tp8-mini sibling would silently loosen a check.

**Test-fn names do not change.** The approx macros already generated the same
`cpu_<mode>_<ds>_sf<sf>_<query>_<device>` pattern as the exact ones, so the 259/259
correspondence must still hold exactly. Re-run the `--list` comparison and say so; a
changed count here means something other than the intended edit happened.

Verification is §9 unchanged, and no golden may move: `rel_tol` affects only the result
compare, never `assert_cpu_cost_canonical`.
