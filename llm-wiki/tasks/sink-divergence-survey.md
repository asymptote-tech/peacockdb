# What the corpus already knows about schema divergence

Kind: prototype

Sixty-odd device cells are disabled against schema causes, and every one of them fails the same way:

```
the exported stream is not the sink's rows: {error}
```

The corpus runs those queries on a device already. They already fail. **The evidence exists and is
unreadable** — the message names neither the column, nor the type the plan declared, nor the type the
device handed back, so a rollout that touches sixty queries produces sixty identical lines.

This task makes that one message say what diverged, runs the corpus, and writes down what comes out.

**Its product is a report, not a feature.** `llm-wiki/reports/sink-divergence.md`. The branch is
never merged; the message change is throw-away scaffolding, and if it turns out to be worth keeping,
keeping it is a different task with a different justification.

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

### 1. The message names both types

`executor/gpu_backend/mod.rs:179` maps `concat_batches`' error to a string that discards everything
useful. Before the concat, walk the decoded batches' schema against `self.schema` and report the
first divergence — or all of them — as column name, declared type, exported type.

The decoded batches already carry the device's schema: `StreamReader::try_new` parses the IPC schema
message before any batch, so `stream.schema()` answers even where no batch follows. Nothing new is
needed and nothing is predicted.

Report **every** diverging column rather than the first. A query whose sink has one string and one
narrow decimal is two findings, and reporting one of them is how a rollout concludes that fixing the
string is enough.

Keep the existing sentence as the prefix so the failure is still greppable by the text every ticket
quotes, and append what it was missing.

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
