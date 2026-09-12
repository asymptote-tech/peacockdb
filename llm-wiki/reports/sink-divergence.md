# What reaches the sink: schema divergence across the corpus

Method: every query whose registry row names #183, #187, #191 or #163 — 94 queries — was run
once on shad-gpu at `tp1_single`, on branch `ENS-sink-divergence-survey` (`228dcbcc` plus the
enablement commit), with the sink's mismatch message naming every diverging column.
The only changes were that message and the 94 `gpu_modes` arguments in `corpus_cases.inc`;
the registry, the goldens and the tickets are untouched, and nothing was fixed. Raw lines per
cell: [`tasks/sink-divergence-survey-detail.md`](../tasks/sink-divergence-survey-detail.md).

Outcome in one line: 89 of 94 cells reached the sink and every one of them failed there on
one to three of **three classes**, and nothing else; 2 ran clean on the device; 3 failed
above the sink.

## 1. Which divergence classes reach the sink

One row per exact (declared type → exported type), counted in queries, sorted by count.
Scale is preserved in every decimal row; only precision moves, and it always moves to 38.

| declared → exported | queries | columns | predicted by |
|---|---|---|---|
| `Utf8View` → `Utf8` | 76 | 204 | #183 |
| `Decimal128(17, 2)` → `Decimal128(38, 2)` | 23 | 38 | #187 |
| `Decimal128(27, 2)` → `Decimal128(38, 2)` | 9 | 12 | #187 |
| `Decimal128(25, 2)` → `Decimal128(38, 2)` | 7 | 10 | #187 |
| `Decimal128(15, 2)` → `Decimal128(38, 2)` | 6 | 6 | #187 |
| `Decimal128(11, 6)` → `Decimal128(38, 6)` | 4 | 10 | #187 |
| `Decimal128(7, 2)` → `Decimal128(38, 2)` | 3 | 4 | #187 |
| `Int32` → `Int16` | 3 | 3 | #191 |
| `Decimal128(19, 6)` → `Decimal128(38, 6)` | 2 | 4 | #187 |
| `Decimal128(23, 6)` → `Decimal128(38, 6)` | 2 | 8 | #187 |
| `Decimal128(33, 2)` → `Decimal128(38, 2)` | 2 | 3 | #187 |
| `Decimal128(16, 6)` → `Decimal128(38, 6)` | 1 | 7 | #187 |
| `Decimal128(23, 8)` → `Decimal128(38, 8)` | 1 | 1 | #187 |
| `Decimal128(32, 2)` → `Decimal128(38, 2)` | 1 | 1 | #187 |
| `Decimal128(5, 2)` → `Decimal128(38, 2)` | 1 | 1 | #187 |

Collapsed to the three classes, with how many of the affected queries carry the ticket that
predicts the class:

| class | queries | columns | registry names it |
|---|---|---|---|
| `Utf8View` → `Utf8` | 76 | 204 | 60 of 76 |
| `Decimal128(p, s)` → `Decimal128(38, s)` | 53 | 105 | 10 of 53 |
| `Int32` → `Int16` | 3 | 3 | 1 of 3 |

Three things the table says that the tickets do not:

- **The decimal widening is on scanned columns as much as on aggregates.** `i_current_price`
  `(7, 2)`, `l_quantity` `(15, 2)`, `s_acctbal` `(15, 2)` and `o_totalprice` `(15, 2)` are
  projected straight from the table and export at 38. #187's second sighting guessed "one width
  rather than each value widened by a step"; 12 distinct declared precisions from 5 to 33 all
  arriving at 38 settles it.
- **44 of the 89 sinks carry more than one class.** 33 are string only, 12 decimal only, 1
  integer only; the rest mix. `try_new`'s first mismatch was the string in 75 of 89, which is
  why #187 has ten rows in the registry and 53 in the device.
- **`Int32` → `Int16` is every `extract(year …)` that reaches a sink**: `o_year` in tpch q8 and
  q9, `l_year` in q7. #191 names q8 alone.

## 2. Which classes no ticket names

**No new class.** Three classes reached 89 sinks and each has a ticket: #183, #187, #191. What
is new is the count, not the kind: #183's twelve cases are 76 queries, #187's "six device
cells" are 53, #191's one cell is three.

The `(p, 6)` and `(p, 8)` decimals — `avg` outputs and ratios — are the same class as the
`(p, 2)` ones and are listed with #187 above. No ticket names them because every query
carrying one was disabled against #163 or hidden behind a string.

## 3. Cells whose failure disagrees with the ticket they are disabled against

The registry's `tickets` column is per query, across all five modes, so a row naming #152
beside #183 is not a claim about `tp1_single`. Read the rows below as "what the device said at
this one mode against what the column names", which is the comparison the spec asked for.

- **#152 did not fire on any of the 60 rows that name it.** Every one reached the sink. The
  one #152 refusal of the survey is `tpcds/q6`, whose row names #163 only. #191's note that
  `tp1-single` is the mode that gets past #152 holds for the whole set.
- **#163 (the `avg` count state, `UInt64` declared, `Int64` produced) reached no sink.** Its 23
  rows split: 18 failed at the sink on the string and decimal classes; 2 ran the whole device
  plan clean — `tpch/q17` and `tpcds/q14` — and failed only because their cpu cell is disabled
  and the cpu golden section says `skipped: not enabled at this mode`; 3 failed above the sink
  on a cause their row also names: `tpcds/q39` on #57 (value-form CASE unsupported),
  `tpcds/q9` on #63 (`copy_if_else` type mismatch in `CudfProject`), `tpcds/q6` on #152. So at
  this mode the count-state divergence is invisible to the sink and does not stop the device.
- **50 sinks report a class their row does not name**, 11 of them on two counts. 43 show
  the decimal class without #187: tpcds q3 q7 q8 q13 q15 q18 q19 q24 q25 q26 q30 q32 q37 q40
  q42 q43 q45 q46 q52 q55 q56 q58 q59 q60 q65 q68 q76 q79 q80 q81 q82 q85 q91 q92, tpch q1
  q10 q18 q22 aggregate-groupby anti-join semi-join shuffle-additive shuffle-additive-avg.
  16 show the string class without
  #183: tpcds q1 q7 q17 q18 q22 q24 q26 q30 q35 q65 q81 q85 (all #163 rows), tpch q1 q22
  shuffle-additive-avg (#163) and tpch q2 (#152 #187 — the decimal is its first mismatch).
  2 show `Int16` without #191: tpch q7 and q9. The per-cell rows are in the detail file.
- **No row named #183 or #187 failed to show that class.** The tickets are under-attributed,
  not wrong.

## 4. What the sink cannot see

`declared-schemas.md` lists fifteen queries. Against this evidence:

- **`UInt64` → `Int64` on the `avg` count state (#163): cannot reach the sink.** 23 queries
  carried it and none showed it, and two of them ran clean. The state column is consumed by the
  device's own merge and finish and never crosses the boundary. This is the one class only a
  per-call harness can measure, and it is the argument for buying one.
- **Nullability flag: cannot reach, by construction.** `RecordBatch::try_new` compares types,
  not the field's nullable flag, and the exporter derives the flag from `has_nulls()`. One
  correction to the spec's premise: `try_new` does refuse null *values* under a non-nullable
  declaration, with a different sentence — that is #180's message on `tpcds/q96` at the tp4
  modes. No cell showed it at `tp1_single`; the survey's appendix would not list it either,
  since it compares types only.
- **Identity classes — `Int64`, `Int32`, `Date32`, `Float64`, `Utf8`, cast targets (rows 4, 5,
  10): untested here, not confirmed.** The message lists only diverging columns, so a column
  that agrees leaves no trace, and this survey cannot tell "agrees" from "no such column
  reached a sink". 89 sinks with no `Date32` or `Int64` clause is consistent with agreement,
  and that is all it is.
- **`Decimal128(38, 4)` agreeing for the wrong reason (row 3): confirmed by the mechanism.**
  Every decimal exports at 38 whatever was declared, so a declared 38 agrees without being
  measured.
- **Zero-row export (row 6): cannot reach.** `GpuExport::unload` returns an empty batch on the
  declared schema before `concat_batches` when the export has no bytes, so no comparison
  happens.
- **Column names and order (rows 7, 9): cannot see.** The comparison is by position and
  `try_new` ignores names; a swapped pair of same-typed columns passes.
- **`Date64` / `Timestamp` (#200, rows 13, 14) and `LargeUtf8`: do not occur** at any of
  the 89 sinks. Each would export as a different type and so would have produced a clause,
  and none did; that matches `casts.md`'s reading that no corpus column declares them.
  `Binary`/`Null`/`Float16` reach cuDF as `EMPTY` and would fail above the sink; no surveyed
  cell did on that cause, so they do not occur either.
- **Several batches into a coalesce agreeing with each other (row 15): cannot see** at this
  mode; `tp1_single` is one batch per lane, and the survey ran no other mode.
- **Above-sink failures the harness would have to drive through**: `tpcds/q39` (#57),
  `tpcds/q9` (#63), `tpcds/q6` (#152). These three cells never produce a sink and are the
  ones a per-call instrument sees something for.

## What this changes for the tasks waiting on it

### `declared-schemas.md`

Keep rows 1, 2 and 12; they are the three classes and they are all of them. Row 2 can name
any narrow decimal — a scanned `(7, 2)` and an `avg`'s `(11, 6)` are one class. Row 3 stays,
now with the survey as its evidence. The class worth the harness is the one the sink cannot
reach: the `UInt64`/`Int64` count state under `avg`, which task 2's aggregate arm should
measure first, on any of the 23 #163 queries. Rows 7 and 9 (names, order) stay because the sink
is blind to them. Row 8's note on nullability should say that `try_new` refuses null values
under a non-nullable field — the flag is the limitation, not the data. Nothing here says a
listed class does not occur, so no row is dropped on this evidence; the identity rows are
simply unmeasured.

### `walk-drives-every-plan.md`

Everything that crosses the boundary surfaces at the sink, and it is three classes. The walk
earns its cost only for what does not cross: the aggregate state (#163), the nullability flag,
and the three cells that fail above the sink. If the walk's first task argues scope from this
report, that is the scope: aggregates and the two refusals, not the string and decimal
classes, which the sink already reports in full.

### The `casts` and `wire-schema` rewrites

Both tickets' accounts of themselves are true and small. #183 is 76 queries and 204 columns;
#187 is 53 queries and 105 columns across 12 declared precisions, on scanned columns as well
as sums; #191 is three queries, all `extract(year)`. All three are the device's representation
— cuDF has no `Utf8View`, carries no decimal precision, and types a year as 16 bits — so all
three are decided at the export and nowhere upstream, which is what the sink's evidence can
show and the per-call harness would confirm. The class set is closed at three for the whole
corpus at this mode; a rewrite that handles those three and refuses the rest has nothing left
to predict. A fix for the decimal class alone would move 12 of the 89 sinks to green; the
string class alone, 33; both together, 86, leaving the three `extract(year)` sinks.
