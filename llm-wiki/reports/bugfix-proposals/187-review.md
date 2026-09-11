# #187 — review of the proposal

Read at master 188c23ce, the commit the proposal names. Every file:line below was opened, not
copied. Nothing built or run.

## 1. Verdict

**Needs changes.** Root cause and fix placement are right and the code shape is sound, but the
CPU-side deletion does not compile as listed, nine of the ten "cells re-enabled" cannot come back
on this fix for a reason the proposal did not see, and the frozen specs it collides with are
under-listed.

## 2. Findings

### 2.1 `widened_decimal` has a third caller the proposal deletes out from under — important

Section 3c deletes `cpu_backend/mod.rs:493-506` (`widened_decimal`) and moves it into
`executor/declared.rs` as a *private* `fn`. But `check_state_layout` at `cpu_backend/mod.rs:467-491`
calls it at `:478`:

    if theirs.data_type() == ours.data_type()
        || widened_decimal(theirs.data_type(), ours.data_type())

That is the aggregate's declared-state check, run at construction (`aggregate_exec`, `:382`), and it
tolerates the same merge-widened decimal. Grep before the change: `widened_decimal` at `:251`, `:478`,
`:499` — two callers, not one. The branch as written fails to compile at `:478`.

Correction: `declared.rs` exposes `pub(crate) fn widened_decimal` as well, `executor/mod.rs` adds a
second one-expression facade item beside `declared_as`, and `cpu_backend/mod.rs` imports
`use super::{declared_as, widened_decimal};`. The `DataType` import at `:24` still goes (only the
predicate's signature named it); the facade needs `use datafusion::arrow::datatypes::{DataType,
SchemaRef};` instead. Two callers of the predicate on the CPU side plus one of `declared_as` on the
device is still one rule, so section 3h's claim stands once the count is right.

### 2.2 The nine tp1-single "candidates" cannot go green; expect five cells, not up to fourteen — important

Section 6 predicts `tpch/hash_join` and `tpcds/q16 q94 q95 q90` "may go green" at tp1-single
because "over one probe batch [the merge] consumes one partial row and emits one". That is true of
the device and false of the golden it is compared against. The CPU join returns DataFusion's output
as it comes — `CpuJoin::probe_and_fetch` (`cpu_backend/join.rs:236-251`) hands back every batch
`run_node` produced — and `HashJoinExec` chunks a large answer at 8192 rows. The committed sections
show it:

- `tpch.sf1/tp1-single-mini.cpu.txt`, `== hash-join`: `GpuHashJoin … batch_rows=[[8192 ×732, 4671]]`,
  `GpuAggregate … batch_rows=[[1 ×733]]`, `GpuAggregateBatches … in_rows=[[733]]`.
- `tpcds.sf1/tp1-single-mini.cpu.txt`: q16, q94, q95, q90, q33, q77 and q61 every one carry a join
  line with `batch_rows=[[8192, 8192, …]]`, and downstream joins consume those chunks as several
  probe batches.

The device join emits at most one batch per probe batch (`gpu_backend/join.rs:164-180`,
`out.extend(prior)`), and the device tier asserts the **whole rendered section** byte for byte
(`tests/common/corpus_gpu.rs:101` → `corpus_golden::assert_section`, `:111-119`, `canonical == body`).
So every one of those nine cells fails on the join's `batch_rows` and the aggregate's `in_rows`
whatever the unload does with a decimal. `q19` passes at tp1-single today only because its join
emits 121 rows — one chunk — and `q6` has no join.

Two consequences for the proposal. (a) Section 6 should predict exactly the five `filter_project`
cells (filter is 1:1 on both engines: `CpuExec::exec` concatenates DataFusion's pieces at
`cpu_backend/mod.rs:193`), and name the batch-boundary divergence as where the other nine land.
(b) This is the same pattern #185 describes — "reports its own output as `in_rows`": one partial
batch reaching the merge has exactly as many rows as the merge emits, which is why q93 reported 7169
against 7486 and q96 `[[1]]` against 34. The class is a CPU/GPU batch-boundary divergence, not (or
not only) a reporting bug, and it is not ticketed as such. The shad-gpu cycle this proposal budgets
will land nine rows on it; the consolidator should know before that cycle, not after. The citation
`corpus_cases.inc:187-190` in section 6 is about a fact table's own batching and is the wrong
mechanism.

### 2.3 Frozen specs the fix falsifies are under-listed, and the one that supports the placement is not cited — important

Section 3f flags `operator-harness.md` and `operator-cases-impl.md`. It misses the spec that is
literally about `declared_as`:

- `llm-wiki/tasks/declared-schemas.md:6-17` argues from `declared_as` being "private to
  `cpu_backend` and every call site is inside it … so it is not reachable from the device path even
  in principle" and "The device side has no equivalent"; `:240-270` (step 7) says #187's ticket text
  "gets corrected here". `declared-schemas-impl.md:669-673` repeats it. `tasks.md:95` (task 10's
  board blurb) says "the device path has no equivalent". All three become false when this lands, and
  two tasks would then both be correcting #187's ticket.
- The same spec, `declared-schemas.md` step 3 ("What the export can and cannot answer"), says of the
  precision question: "*does the data fit the declared precision* is a **value** check … `declared_as`
  already asks it on the CPU side, and that is where the production fix will start." That is the one
  approved document placing a #187 fix at `declared_as`, and it answers the objection the
  `wire-schema` rejection note invites ("fixed at its source, not carried as a per-column width" —
  `archive/archived-tasks.md:161`). Section 4 distinguishes this proposal from `casts.md` but never
  cites the sentence that authorises it. It should; the consolidator otherwise has to decide whether
  a Rust-side cast at the unload is the rejected half of `casts.md` returning.

Correction: add the three documents to 3f's flagged list, and add the declared-schemas §3 sentence to
section 4 as the reason a cast at the unload is the sanctioned shape.

### 2.4 The "minimum corpus query" has no vehicle — minor

`SELECT s_acctbal FROM supplier` would plan and would reach `GpuExport::unload`, but nothing runs an
ad hoc SQL on a device: the CLI is CPU-only by design (`peacockdb/src/main.rs:1-6`), and the device
corpus takes a committed `.sql`, a registry row, a `corpus_query!` line and a cpu-authored section
(`test_gpu_corpus::the_registry_matches_the_gpu_corpus_in_both_directions`, plan-golden meta test).
The section should lead with `tpch/filter-project` as the smallest *runnable* query and say the
device test in 3e is the minimal reproduction; the ad hoc query is a thought experiment.

### 2.5 Section 3g overstates what the unload still refuses — minor

"The unload still refuses every type difference it refused before" is not literally true: a wider
same-scale `Decimal128` is now cast rather than refused. What answers hacks-audit finding 12 is that
the cast is validated per value (`safe: false`, `arrow-cast-54.2.1/src/cast/decimal.rs:154-161`), so
what reaches the names-only comparator is the declared type by construction. Say that; the sentence
as written invites a reviewer to check it and find it false.

### 2.6 The new unload message dumps two whole schemas — minor

3d wraps `declared_as`'s refusal, whose text is "the node declares {declared:?} and its executor
produced {:?}: {error}". At the sink of `tpch/q2` that is two eight-field `Schema` debug prints before
the `try_new` sentence the tickets quote. The `sink-divergence-survey` exists because this site said
too little; this says too much in a CI log line. Either keep the inner arrow error alone at the
unload site, or have `declared_as` name only the differing columns. Not blocking: the greppable
inner text survives.

### 2.7 Small bookkeeping the developer hits in the first hour — minor

- `build-test.md:7`'s grand total (1569) moves with the rows: +3 Lib unit, +1 device → 1573. The
  proposal names the rows and not the header.
- `test-layout.md` (board task 4, approved to build) moves `tests/test_gpu_executors/` into `src/`;
  the 3e test lands in a file about to move. Trivial rebase either way; worth a line in 7.
- The `declared.rs` doc comment is exactly ten lines, the cap from `coding-style.md`. Count it after
  rustfmt, not before.

## 3. Claims verified

- The mechanism: `export_table_to_ipc` builds `column_metadata{name}` only (`gpu_executor.cpp:55-56`)
  and widens DECIMAL32/64 (`:61-68`); vendored cuDF 25.10 `to_arrow_schema.cpp:111-119` labels a
  decimal128 `metadata.precision.value_or(max_precision<__int128_t>())` = 38; the 25.02 header
  (`~/data/miniforge3/envs/rapids-cuda-12.2/include/cudf/interop.hpp:108-119`) has no `precision`
  member, 26.02 (`envs/rapids/…/interop.hpp:110`) has `std::optional<int32_t> precision`.
  `scan.cpp:98-110` widens at the scan. `PlanNode.output_schema` is `None` at both writer sites
  (`wire/writer.rs:102`, `:131`). `TableResult` is table + names (`plan_executor.h:15-18`).
- The unload: `gpu_backend/mod.rs:176-183` concat against `self.schema`, set from `input(0)` at
  `gpu_backend/backend.rs:143`; empty export at `:170-175`. `concat_batches` ends in
  `RecordBatch::try_new` (`arrow-select-54.2.1/src/concat.rs:295`, `arrow-array-54.2.1/src/
  record_batch.rs:333` is the quoted message; `:298` is the nullability refusal).
- The CPU rule: `declared_as` `cpu_backend/mod.rs:236-275`, `widened_decimal` `:493-506`, callers
  `mod.rs:188`, `join.rs:272`, `accumulate.rs:431`; test at `cpu_backend/tests/backend.rs:354-384`.
- arrow-cast: `cast_decimal_to_decimal_same_type` (`cast/decimal.rs:165-202`) with 38 → 15 at equal
  scale takes `convert_to_bigger_or_equal_scale_decimal` with `mul = 10⁰`; `safe: false` runs
  `validate_decimal_precision` per valid value via `try_unary`, which skips null slots and keeps the
  null buffer (`arrow-array/src/array/primitive_array.rs:903-927`). NULLs survive the cast.
- Sink schemas at tp1-single match the proposal's list exactly: hash_join `(25,2)` ×2; q2 `(15,2)`;
  q16/q94/q95 `(27,2)` ×2; q33 `(27,2)`; q61 `(17,2)` ×2 + `(38,8)`; q77 `Utf8`, `(27,2)`, `(32,2)`,
  `(33,2)`; q90 `(23,8)`. q6 and q19 declare `(38,4)`. Registry `187` on rows 17, 34, 62, 78, 91,
  95, 96, 102, 126, 127; #97 and #116 are archived.
- The device test would cross the wire: `Expr::Cast` writes `CastExprNode` with precision and scale
  (`wire/expr_writer.rs:81-93`), `is_ast_able` routes a non-INT64/FLOAT64 cast to the column path
  (`expr.cpp:439-447`), which calls `cudf::cast` to `DECIMAL128` at `-scale` (`:913-935`).
  `one_node` is at `test_gpu_executors.rs:283`; the fixture is `k:Utf8, v:Int64`.
- Bytes are precision-blind: `CpuBatch::byte_size` is `logical_size_from_schema`
  (`executor/cpu_batch.rs`), `Decimal128(_, _) => rows * 16` (`common.rs:39`). No `.cpu.txt`,
  `.cost.txt` or `.result.txt` moves; the recipe walk exports and compares on its own path.
- `join.rs:27` and `accumulate.rs:20` importing `super::declared_as` keep compiling through a private
  `use` in `cpu_backend/mod.rs` — a child module sees its parent's imports.
- Layout rules: `declared.rs` as `mod declared;` with `pub(crate)` items and a one-expression facade
  in `executor/mod.rs` matches `test_module_layout.rs` (`a_components_api_is_declared_in_its_mod_rs`
  flags bare `pub` only; `no_subcomponent_reaches_a_sibling` is not touched). Inline
  `#[cfg(test)] mod tests` is the idiom at `executor/row_range.rs:15` and `forwarder.rs:56`.
- No test pins "DataFusion answered with" or "the exported stream is not the sink's rows"; #190 at
  `active-tickets.md:200` quotes the former.
- hacks-audit: finding 12 is the only #187 item, and "What I found nothing for" says so.
- Enabled device cells (q6 ×5, q19 tp1-single) declare `(38,4)`: `widened_decimal(38 ≥ 38)` takes
  the `array.clone()` arm of the same-type cast — a relabel, no value walk. Nothing enabled moves.

## 4. Corrected proposal

Only the sections that change.

### 3b. `peacockdb-core/src/executor/mod.rs`

Two facade items, not one, after `forwarder_for`:

```rust
/// The same columns under the schema the node declares … (as proposed)
pub(crate) fn declared_as(batch: RecordBatch, declared: &SchemaRef) -> Result<RecordBatch, BackendError> {
    declared::declared_as(batch, declared)
}

/// Whether `produced` is `declared` with a decimal's precision widened at the same scale — the one
/// mismatch `declared_as` casts and the aggregate's state check tolerates.
pub(crate) fn widened_decimal(produced: &DataType, declared: &DataType) -> bool {
    declared::widened_decimal(produced, declared)
}
```

Imports: `use datafusion::arrow::datatypes::{DataType, SchemaRef};`.

### 3c. `peacockdb-core/src/executor/cpu_backend/mod.rs`

As proposed, plus: `use super::{declared_as, widened_decimal};` so `check_state_layout` (`:467-491`)
keeps its `:478` call. In `declared.rs`, `widened_decimal` is `pub(crate)`, not private.

### 3f. Corpus lines, registry, wiki

As proposed, plus three frozen documents flagged for the human rather than edited:
`declared-schemas.md:6-17` and step 7 (`:240-270`), `declared-schemas-impl.md:669-673`, and the task-10
blurb at `tasks.md:95` — each states that `declared_as` is CPU-private and the device has no
equivalent, and step 7 assigns #187's ticket correction to that task. One of the two must yield.

### 4. Alternatives rejected

Add, at the top: `declared-schemas.md` step 3 — "`declared_as` already asks it on the CPU side, and
that is where the production fix will start" — is the approved statement of where a #187 fix belongs,
and is why a Rust-side cast at the unload is not `casts.md` returning: no prediction, no plan
rendering, no reason enum; the structural predicate the engine already had.

### 5. Minimum corpus query

`tpch/filter-project` (`testdata/tpch-queries/filter-project.sql`), tp1-single, gpu: scan → filter →
unload, `l_quantity:Decimal128(15,2)`, off at all five device modes on `187` alone. It is the smallest
committed query on the path; the CLI is CPU-only and an ad hoc SQL has no device vehicle without a
corpus line. The minimal reproduction is the device test in 3e (one project, one cast, six rows).

### 6. Cells re-enabled

**Back on this fix alone (5 cells):** `tpch/filter_project` at all five modes, as proposed.

**Stay off, on a cause this fix does not touch (9 cells):** the tp1-single cells of `hash_join`, `q2`,
`q16`, `q33`, `q61`, `q77`, `q90`, `q94`, `q95`. Every one carries a join whose CPU golden emits
8192-row chunks and whose downstream nodes consume them as several batches; the device emits one
batch per probe batch and the device tier compares the whole section. `q2` also lands on #183 first.
The shad-gpu cycle should be run to record what each reports, and the row's ticket updated to name
the batch-boundary class — as #185 re-read, or a new ticket if #185 is kept as a reporting bug.

### 8. Complexity

Code stays **S**. Closing is **M**: the cycle over ten rows is one run, but nine of them land on a
class that is not ticketed under its own mechanism, and each needs a dated line or a new ticket
before the registry can carry it. Budget the re-attribution, not only the run.

## 5. Complexity

**M**, against the proposal's S. Same code; the difference is section 6. The proposal prices closing
as one shad-gpu cycle whose rows are "enabled or re-ticketed from what the run says". Nine of ten will
be re-ticketed onto a divergence nobody has written down yet (2.2), and writing it down — and
deciding what it means for #185 — is the part that takes the time.
