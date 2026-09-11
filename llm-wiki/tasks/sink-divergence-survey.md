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
