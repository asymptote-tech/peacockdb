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
call's output through a deliberately thin exporter, compares the two, and records every disagreement
as a `bug_` test. It **fixes nothing**.

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
wrong.** Step 3 replaces two of the three rewrites, in a test-only path, with a *check* — so that
what remains unexplained is cuDF's. The production exporter is not touched, and the difference
between the two exporters is itself a finding worth writing down.

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

### 3. A thin exporter, for tests only — and it checks rather than states

A second export path that differs from the production one in exactly two dimensions, precision and
nullability, and in nothing else.

**It casts to the declared type; it does not relabel.** The distinction is the whole value of it. A
path that restated `Decimal128(15,2)` on the way out would assert the declaration, could never fail,
and would measure nothing — worse, it would make precision and nullability *unmeasurable*, since
exported would equal declared by construction. Casting asks a question nothing in the engine
currently asks: **does the data the device produced actually satisfy the type the plan declared?** A
value too wide for its declared precision fails, naming its column, and that failure is a finding.

Nullability the same way: compare the declared flag against the **actual null count**, which is a
fact cuDF does hold. A column declared non-nullable that exported nulls is a finding, not a relabel.

This makes it the device-side analogue of `declared_as`, which casts a widened decimal back on the
CPU side and takes the error rather than a silently-NULLed row. So the instrument doubles as a
prototype of the production fix: if the cast holds across the corpus, the later task knows what to
build. The production exporter is untouched — making it thin is that task's decision, and it needs
the evidence this one produces.

**How the precision is stated is version-dependent, and both arms are written.** `column_metadata`
gained a `precision` member in **26.02**; before that it is name and children only. So the 26.02 arm
passes precision through the supported channel, and the 25.02 arm restates the types after
`ImportSchema` — `ImportRecordBatch` does not cross-check the array against the schema's precision,
which is what makes that sound. Both are behind a `CUDF_VERSION_MAJOR`/`MINOR` guard.

**The 26.02 arm is dead code today** and says so where it is written. `shad-gpu` runs 25.02 and CI's
26.02 leg only compiles, so nothing executes it. It is written now because writing it later means
rediscovering why the other arm exists — which is precisely the ground `wire-schema` lost when it
chose a 26.02 mechanism and did not notice the leg that runs could not provide it.

**Which exporter produced a finding is recorded with the finding.** A `(38,2)` through the production
path is *our* default and not a cuDF divergence, and a catalog that cannot tell those apart sends the
next task to the wrong file. That is exactly what happened to `wire-schema`.

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
| 2 | `SELECT l_extendedprice FROM lineitem WHERE l_orderkey = 1` | tp1-single | narrow `Decimal128(15,2)` — [#187](active-tickets.md#t187), and the thin-versus-production exporter difference |
| 3 | `SELECT CAST(l_extendedprice AS DECIMAL(38,4)) FROM lineitem WHERE l_orderkey = 1` | tp1-single | a decimal already at max precision. Through the production exporter it agrees **for the wrong reason**; the thin one is what makes that visible |
| 4 | `SELECT l_shipdate FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Date32` → `TIMESTAMP_DAYS` → back |
| 5 | `SELECT l_orderkey, l_linenumber FROM lineitem WHERE l_orderkey = 1` | tp1-single | `Int64` and `Int32` identity |
| 6 | `SELECT n_name FROM nation WHERE n_nationkey < 0` | tp1-single | the **zero-row** batch, and whether an empty string column types as a populated one |
| 7 | `SELECT n_name AS label, n_nationkey AS id FROM nation` | tp1-single | column **names** survive the crossing. Declared-versus-device only: the CPU relabels names to the declaration, correctly, so it has no opinion to compare against |
| 8 | `SELECT CASE WHEN n_nationkey > 10 THEN n_name END FROM nation` | tp1-single | **nullability**. The production exporter reports whether this batch happened to contain a null; the thin one compares the declared flag against the actual null count, so the answer is about the declaration being honoured rather than about the data |
| 9 | `SELECT n_regionkey, n_nationkey FROM nation` | tp1-single | column **order and arity** — [#190](active-tickets.md#t190)'s uncaught half |
| 10 | `SELECT CAST(n_nationkey AS BIGINT), CAST(n_nationkey AS DOUBLE) FROM nation` | tp1-single | cast targets, fixed-width |
| 11 | `SELECT CAST(n_nationkey AS VARCHAR) FROM nation` | tp1-single | [#45](../tickets.md#t45) — a cast target that is not fixed-width |
| 12 | `SELECT extract(year FROM o_orderdate) FROM orders` | tp1-single | [#191](active-tickets.md#t191) — integer **narrowing**, `Int16` exported where `Int32` was declared |
| 13 | a scan yielding several batches into a coalesce | tp1-rowgroup | **the firings of one call agree with each other.** The mode is the point: `rowgroup` is what produces more than one batch per lane. Confirm against that mode's plan text and name the query in `-impl.md` |

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
