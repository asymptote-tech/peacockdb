# #187 — proposal: the unload holds the export to the precision the sink declares

Read at master 188c23ce. Paths are relative to `/media/data/peacockdb`. arrow-cast / arrow-array
54.2.1 cited from the cargo registry; cuDF read from `third_party/cudf` (25.10.00) and from the two
installed header sets (`~/data/miniforge3/envs/rapids-cuda-12.2` = 25.02, `rapids` = 26.02). Nothing
was built or run; section 7 names what only a device run can settle.

## 1. Issue

Every device cell whose sink carries a `Decimal128(p, s)` with p < 38 is refused at the unload after
the whole plan has run. `executor/gpu_backend/mod.rs:179-183` concatenates the decoded IPC batches
against the sink's declared schema; `concat_batches` ends in `RecordBatch::try_new`
(arrow-array-54.2.1 `record_batch.rs:333`), which requires exact type equality and says

    GpuUnload lane 0: the exported stream is not the sink's rows: column types must match schema
    types, expected Decimal128(15, 2) but found Decimal128(38, 2) at column index 1

The digits are the same; only the precision label differs. The 00-tickets.md row is right about the
set: gpu column only, `tpch/filter_project` at all five modes (its plan is scan → filter → unload,
nothing else; ticket text confirms all five fail here and only here) and the tp1-single cell of
`tpch/hash_join`, `tpch/q2`, `tpcds/q16 q33 q61 q77 q90 q94 q95` — registry rows 17, 34, 62, 78, 91,
95, 96, 102, 126, 127 carry `187`; corpus comments at `tests/common/corpus_cases.inc:48, 61, 120,
171`. Checked against each query's tp1-single sink schema in the plan goldens: every one declares a
decimal narrower than 38 (filter_project `(15,2)`; hash_join `(25,2)` ×2; q2 `s_acctbal (15,2)`;
q16/q94/q95 `(27,2)` ×2; q33 `(27,2)`; q61 `(17,2)` ×2 beside a `(38,8)` that already agrees; q77
`(27,2)`, `(32,2)`, `(33,2)`; q90 `(23,8)`).

Two corrections to the row, not to the ticket's cell list. (a) The ticket's file:line has moved:
the concat is `executor/gpu_backend/mod.rs:179`, and `widened_decimal` / `declared_as` are
`executor/cpu_backend/mod.rs:499` / `:239`. (b) The ticket's framing — "two rules for
produced-against-declared, one per engine, disagreeing on the same bytes" — is not the mechanism.
The device has no rule at all; it has an export that is never told a precision. Section 2.

## 2. Root cause

cuDF's `fixed_point` type is a storage width and a scale. It has no precision. So nothing on the
device can produce `Decimal128(15, 2)`; it can only produce "128-bit, scale 2", and the export has to
invent a precision when it writes Arrow metadata.

- `cpp/src/operators/scan.cpp:98-110` widens every DECIMAL32/64 the parquet reader picks to
  DECIMAL128 (scale preserved), so every decimal on the device is 128-bit storage from the scan on.
  `architecture.md` "Every cast is explicit" sanctions this as "the source honouring the output
  schema it already declares".
- `cpp/src/gpu_executor.cpp:51-96`, `export_table_to_ipc`: builds `column_metadata` from the name
  alone (`:57`), widens any remaining DECIMAL32/64 to DECIMAL128 (`:64-65`, needed because arrow-rs
  54 has no `Decimal32`/`Decimal64` type), then `cudf::to_arrow_schema(export_view, col_meta)`.
- cuDF's `to_arrow_schema.cpp` (vendored 25.10, `:114-119`): for `decimal128` the precision is
  `metadata.precision.value_or(cudf::detail::max_precision<__int128_t>())` = 38. On 25.02 — the
  version shad-gpu runs and the only one a functional GPU run is verified on — `column_metadata` has
  **no `precision` member at all** (`interop.hpp:108-119` there; 26.02 adds
  `std::optional<int32_t> precision` at `:110`), so the export cannot be told a precision through
  cuDF on that version even if the C++ knew it. And it does not know it: `TableResult`
  (`cpp/src/plan_executor.h`) is a `cudf::table` plus names; `PlanNode.output_schema` exists in
  `gpu_plan.fbs` and is never written; `peacock_result_from_handle` takes a handle and a row range.
- So the stream arrives on the Rust side as `Decimal128(38, s)` for every decimal, whatever the plan
  declared. `GpuExport` already holds the declared schema (`gpu_backend/mod.rs:131`, set from the
  sink's input at `gpu_backend/backend.rs:143`) and uses it for the empty export (`:172`) and as the
  concat target (`:179`), but never reconciles the two.

Why q6 and twenty other queries passed this site: their decimals are sums declared `Decimal128(38,
4)` already, so the invented precision happened to equal the declared one.

Why the CPU is green on every one of these cells: arrow-rs's parquet reader reads the file's
annotation (`Decimal128(15, 2)`), DataFusion's expressions produce the precision it derived, and
where a merge widens a sum the CPU casts it back at every stage output — `declared_as`
(`cpu_backend/mod.rs:239-275`), whose one tolerated mismatch is `widened_decimal` (`:499-506`): same
scale, produced precision ≥ declared, cast with `safe: false` so a value that does not fit ends the
query instead of becoming a NULL. Every other mismatch is refused by `RecordBatch::try_new`.

That predicate is exactly the device's divergence: same scale, produced 38 ≥ declared p. The device
path simply never calls the rule. It is one engine with the rule applied on one side and not the
other, which is what the ticket saw as two rules.

## 3. Localized fix

One rule, two callers. Move `declared_as` and `widened_decimal` out of the CPU backend into the
`executor` component where both backends can reach it, and call it at the device's export. No C++,
no ABI, no wire change, no plan or golden change.

### 3a. `peacockdb-core/src/executor/declared.rs` — new implementation module (~60 lines + tests)

The body is `cpu_backend/mod.rs:239-275` and `:493-506` moved verbatim except for the doc and the
message, which stop naming DataFusion:

```rust
//! The produced-against-declared rule both backends hold a batch to.

use datafusion::arrow::array::RecordBatch;
use datafusion::arrow::compute::{CastOptions, cast_with_options};
use datafusion::arrow::datatypes::{DataType, SchemaRef};
use datafusion::arrow::util::display::FormatOptions;

use super::BackendError;

/// The same columns under the schema the node declares. Positional, and checked by arrow:
/// a column whose type is not the declared one is a wrong answer everywhere above rather
/// than an error, so everything but one mismatch is refused.
///
/// The one is a decimal wider than declared at the same scale, which both engines produce
/// for different reasons. DataFusion widens a sum's precision at every merge, so a state
/// merged twice would carry a wider type than one merged once. cuDF's fixed_point has a
/// scale and no precision at all, so a decimal leaves a device labelled at the storage
/// width's maximum, 38. The declaration is the fixed point in both cases and the cast back
/// is what keeps the two engines' batches one type rather than one value in two.
pub(crate) fn declared_as(batch: RecordBatch, declared: &SchemaRef) -> Result<RecordBatch, BackendError> {
    if batch.schema() == *declared {
        return Ok(batch);
    }
    let mut columns = batch.columns().to_vec();
    for (column, field) in columns.iter_mut().zip(declared.fields().iter()) {
        if widened_decimal(column.data_type(), field.data_type()) {
            // Unsafe rather than safe casting, which is the whole point: arrow's safe cast
            // turns a value that does not fit the declared precision into a NULL, and a
            // NULL in a sum column is indistinguishable here from one the data had.
            let options = CastOptions { safe: false, format_options: FormatOptions::default() };
            *column = cast_with_options(column, field.data_type(), &options).map_err(|error| {
                BackendError::new(format!(
                    "{} does not fit the {} the node declares: {error}",
                    field.name(), field.data_type()
                ))
            })?;
        }
    }
    RecordBatch::try_new(declared.clone(), columns).map_err(|error| {
        BackendError::new(format!(
            "the node declares {declared:?} and its executor produced {:?}: {error}",
            batch.schema()
        ))
    })
}

/// Same scale, because a scale difference moves the point and is a different number rather
/// than a wider one; and wider only, because narrower is not what a merge does and casting
/// it up would be inventing precision the state never had.
fn widened_decimal(produced: &DataType, declared: &DataType) -> bool {
    match (produced, declared) {
        (DataType::Decimal128(theirs, their_scale), DataType::Decimal128(ours, our_scale)) => {
            their_scale == our_scale && theirs >= ours
        }
        _ => false,
    }
}
```

arrow-cast 54.2.1 makes the same-scale, smaller-precision cast a validated relabel:
`cast_decimal_to_decimal_same_type` (`cast/decimal.rs:165-202`) takes the
`convert_to_bigger_or_equal_scale_decimal` branch with `mul = 10^0`, and with `safe: false`
`validate_decimal_precision` runs per value (`:154-161`), then `with_precision_and_scale` relabels.
Same digits in, declared type out, overflow refused.

`#[cfg(test)] mod tests` in the same file, no device:

- `a_state_value_too_large_for_its_declared_precision_ends_the_query` — moved verbatim from
  `cpu_backend/tests/backend.rs:354-384`, calling `super::declared_as`.
- `a_decimal_produced_wider_than_declared_at_the_same_scale_comes_back_as_declared` — a
  `Decimal128(38, 2)` column of `[Some(3100), None, Some(5000)]` against a declared `(15, 2)`:
  the answer's schema equals the declaration and the values are unchanged. This is the device's
  shape and is red before this module exists on the device path only in the sense that nothing
  called it; it is the unit statement of the rule.
- `a_decimal_at_another_scale_is_refused` — produced `(38, 4)`, declared `(15, 2)`: `Err`, message
  names both types.
- `a_type_that_differs_in_kind_is_refused` — two columns, `Int16` against `Int32` (the #191
  shape) and `Utf8` against `Utf8View` (the #183 shape): `Err` for each. This is what proves the
  check the unload has today survives the change.

### 3b. `peacockdb-core/src/executor/mod.rs`

- `:11-15`: add `mod declared;` between `mod cpu_batch;` and `mod driver;`.
- imports: `use datafusion::arrow::datatypes::SchemaRef;` beside the `RecordBatch` import at `:32`.
- one delegating facade item, the `RowRange::clamp` idiom (`:258-260`), placed after
  `forwarder_for` (`:391-393`):

```rust
/// The same columns under the schema the node declares, cast where the declaration is the
/// fixed point a produced decimal widened past, refused for any other difference. One rule
/// for both backends: the CPU holds every stage's output to it, a device its export.
pub(crate) fn declared_as(batch: RecordBatch, declared: &SchemaRef) -> Result<RecordBatch, BackendError> {
    declared::declared_as(batch, declared)
}
```

### 3c. `peacockdb-core/src/executor/cpu_backend/mod.rs`

- delete `:236-275` (`declared_as`) and `:493-506` (`widened_decimal`).
- `:23` becomes `use datafusion::arrow::compute::concat_batches;`; drop `DataType` from `:24` (its
  only other use was `widened_decimal`) and delete `:25` (`FormatOptions`).
- add `use super::declared_as;`. `join.rs:27` and `accumulate.rs:20` import `super::declared_as`
  and keep compiling, since a child sees its parent's private import; `mod.rs:188` is unchanged.
- `cpu_backend/tests/backend.rs:354-384`: the test moves to 3a; delete here.

### 3d. `peacockdb-core/src/executor/gpu_backend/mod.rs` — the fix proper

- add `use crate::executor::declared_as;` to the imports at `:33-37`.
- `:176-183` become:

```rust
        let decoded = decode(unsafe { std::slice::from_raw_parts(ipc, len as usize) });
        unsafe { peacock_result_free(ipc) };
        // A device labels every decimal at precision 38, having no precision to label it
        // with; the sink's declaration is what the column is, and `declared_as` is the one
        // rule that says so on either engine. Every other difference still refuses here.
        let batches = decoded?
            .into_iter()
            .map(|batch| declared_as(batch, &self.schema))
            .collect::<Result<Vec<RecordBatch>, BackendError>>()
            .map_err(|error| {
                BackendError::new(format!(
                    "the exported stream is not the sink's rows: {}",
                    error.message
                ))
            })?;
        let batch = concat_batches(&self.schema, batches.iter()).map_err(|error| {
            BackendError::new(format!("the exported stream is not the sink's rows: {error}"))
        })?;
```

The prefix the tickets quote is kept, and the inner `try_new` text ("expected … but found … at
column index …") is still what #183 and #191 print, so their sightings stay greppable. The
`concat_batches` after it is now a pure concatenation of batches already in the declared schema
(the export writes one record batch, `gpu_executor.cpp:81`), kept because it is where an empty
`Vec` would otherwise need its own arm.

### 3e. Device regression test — `peacockdb-core/tests/test_gpu_executors/exec.rs`

After `an_export_whose_offset_is_past_the_end_answers_empty` (`:262-281`), using `one_node`
(`test_gpu_executors.rs:283`) and the existing six-row fixture:

```rust
/// A device has no precision to label a decimal with and exports every one at 38; the sink's
/// declaration is what comes back, digits untouched.
#[test]
fn an_export_labels_a_decimal_with_the_precision_the_sink_declares() {
    let out = schema_of(&[("price", DataType::Decimal128(15, 2))]);
    let node = GpuProject::new(
        source(),
        vec![NamedExpr::new(
            Expr::Cast { expr: Box::new(Expr::column(1, "v")), target: DataType::Decimal128(15, 2) },
            "price",
        )],
        Schema::new(Arc::new(out.clone())),
    );
    let answer = one_node(Box::new(node), &out);
    assert_eq!(answer.record_batch().schema().field(0).data_type(), &DataType::Decimal128(15, 2));
    assert_eq!(
        rows(&answer).into_iter().map(|row| row[0].clone()).collect::<Vec<ScalarValue>>(),
        values().iter().map(|v| ScalarValue::Decimal128(Some(*v as i128 * 100), 15, 2)).collect::<Vec<_>>()
    );
}
```

Red today at `one_node`'s `.expect("the rows cross the boundary")` with the #187 message; the cast
crosses the wire as `CastExprNode{Decimal128, precision 15, scale 2}` (`wire/expr_writer.rs:81-93`)
and the C++ column path builds it (`cpp/src/expr.cpp:915-935`). `build-test.md`'s
`test_gpu_executors` row goes 31 → 32 and the Lib unit row up by the three new cases in 3a.

### 3f. Corpus lines, registry, wiki

- `tests/common/corpus_cases.inc:49`: `filter_project`'s gpu modes `none` → all five, once the
  shad-gpu run confirms; the batch-1 comment at `:47-49` loses "#187's widened decimal for
  filter-project". The `:61`, `:120`, `:171` sentences change to whatever each cell lands on.
- `testdata/cost-registry.csv:126` (`filter_project`): five gpu cells `disabled` → `enabled`, tickets
  `187` → empty. Rows 17, 34, 62, 78, 91, 95, 96, 102, 127: drop `187`, add the cause the run
  reports (section 6). `116` on row 127 and `97` on rows 17, 34, 78, 95, 96 are closed tickets and
  come off whatever the run says. `test_gpu_corpus::the_registry_matches_the_gpu_corpus_in_both_directions`
  ties the two files.
- `llm-wiki/architecture.md` "Every cast is explicit", after "Two stay in C++ with a reason …":
  one sentence — the unload is the loader's rule from the other side: cuDF's fixed_point carries a
  scale and no precision, so a decimal leaves a device labelled at 38 and `declared_as` casts it
  back to the sink's declared precision, the same rule the CPU applies to a merge-widened sum at
  every stage. The cuDF-options table's "IPC export" row stays true as written.
- `llm-wiki/build-test.md:44` (Corpus, device): the cell count and the blocker list, after the run.
- `llm-wiki/tasks/active-tickets.md` #187: corrected to the mechanism in section 2 when the code
  lands, archived when its cells are gone. `:200` (#190) quotes the old CPU message "DataFusion
  answered with"; it now reads "its executor produced".
- `llm-wiki/tasks/operator-harness.md` (the `decimals` fixture paragraph) and
  `operator-cases-impl.md:70-71, 191, 428, 1059` say #187 is "open and owned by no task" and expect
  every decimal case to land as its `bug_` test. Frozen specs; flagged for the human rather than
  edited. If the harness lands first, its #187 `bug_` tests are deleted by this change; if this
  lands first, they are never written and the decimal cases go green on their first run.

### 3g. What it deliberately does not touch

- `cpp/src/gpu_executor.cpp`: the DECIMAL32/64 → 128 widening stays (arrow-rs 54 cannot read
  narrower); no `column_metadata.precision`, which 25.02 does not have.
- The ABI, `gpu_plan.fbs` (`PlanNode.output_schema` stays unwritten), `TableResult`.
- `executor/cpu_backend/source.rs:117` `as_declared` — the parquet-boundary cast is a different
  policy (blanket, because a file's types are not produced types) and stays where it is; the count
  of produced-against-declared rules stays at two, not three.
- `tests/common/result_text.rs:92` `schema_digest` (names only). The unload still refuses every
  type difference it refused before, so the comparator's last line is not asked to do more; hashing
  types is #183's item.
- Planner declarations, plan goldens, `.cpu.txt`, `.cost.txt`, `.result.txt`: a Decimal128 is 16
  bytes at any precision (`common.rs:39`) and rows render the same digits, so no golden moves.

### 3h. How CPU and GPU stay one engine

The rule is one function. The CPU calls it at every stage output (`cpu_backend/mod.rs:188`,
`join.rs:272`, `accumulate.rs:431`) because DataFusion's next operator reads Arrow types; the device
calls it once, at the only point where its tables become Arrow. Between those points a device table
carries no precision to hold to anything, so there is nothing else to reconcile. Unit tests in 3a
pin the predicate; the device test in 3e pins that the export applies it; the corpus cell pins the
query.

### 3i. hacks-audit scaffolding

Finding 12 ("What the known bugs are propping up"): "the device has none. A fix for #187 that adds
a third rule at the device unload makes three rules for one question." This fix adds no rule — it
moves the second one to where both engines reach it. The finding's other half (the comparator
narrowed to names) is #183's and is untouched here. Nothing else in the audit names #187.

## 4. Alternatives rejected

- **Carry precision on the wire and in `TableResult` (archived `wire-schema.md`, PR #137):**
  rejected by the human 2026-09-10 — "not carried as a per-column width"; also unfixable on 25.02
  through `column_metadata`, which has no precision there.
- **Predict export types at plan time and cast by reason at the unload (archived `casts.md`, PR
  #136):** rejected — builds the divergence into the plan (`exports=` on every unload line) and
  steers the cast by an enum with a variant per bug. This proposal predicts nothing, renders
  nothing, and casts by a structural predicate the engine already had.
- **A new ABI symbol handing `result_from_handle` the declared schema so the C++ rewrites the Arrow
  schema after `to_arrow_schema`:** the smallest non-local form. Costs a frozen-surface change
  (seventeenth symbol, `peacock_gpu.h`, `peacockdb-ffi`, `test_gpu_abi`), and the C++ would need
  its own "only where the difference is decimal precision at equal scale" test before substituting
  a format string — a second copy of `widened_decimal`, in the other language. The precision would
  travel Rust → C++ → IPC bytes → Rust to label one message the Rust side already knows the label
  of. Take it only if a cast on the Rust side of the boundary is ruled out on principle.
- **Declare every decimal `Decimal128(38, s)` in the planner:** DataFusion derives sum/divide
  precisions from inputs and the CPU produces them; every plan golden moves, and `widened_decimal`
  would then see produced 15 < declared 38 and refuse on the CPU. Wrong direction.
- **Stop widening at the export and let cuDF label DECIMAL64 as 18:** still not the declared 15,
  and arrow-rs 54 rejects the narrow stream anyway.
- **Wait for the operator harness / declared-schemas catalog:** they record the divergence as a
  `bug_` test; its mechanism is already read to the line.

## 5. Minimum corpus query

    SELECT s_acctbal FROM supplier

tpch sf1 (also in `tpch.minimal`), tp1-single, GPU backend; `s_acctbal` is `Decimal128(15, 2)`. Plans
at all five modes as `GpuUnload` over `GpuLoadParquet` — no filter, no arithmetic. Runs the scan on
the device, exports, and is refused at `GpuExport::unload` (`gpu_backend/mod.rs:179`): "the exported
stream is not the sink's rows: column types must match schema types, expected Decimal128(15, 2) but
found Decimal128(38, 2) at column index 0". The CPU backend answers it. After the fix the same run
answers `Decimal128(15, 2)` with the file's digits. The smallest committed query on the same path is
`tpch/filter-project` (`testdata/tpch-queries/filter-project.sql`), off at all five device modes on
`187` alone.

## 6. Cells re-enabled

Derived from each row's tp1-single plan (node kinds counted over the golden) and its other tickets;
a shad-gpu run decides, in one batch of ten.

**Back on this fix alone (5 cells):** `tpch/filter_project` at all five modes — scan, filter,
unload; no join, no aggregate, no string; oracle `live_cpu`. The ticket records #187 as the only
failure at every mode.

**Candidates at tp1-single (9 cells), with the wall each is likely to meet next:**

- `tpch/hash_join`, `tpcds/q16 q94 q95 q90`: no string at the sink, one `GpuAggregateBatches` each
  (q90 two) over keyless aggregates. #185 bites only where that node consumes more than one batch;
  at tp1-single over one probe batch it consumes one partial row and emits one, so these may go
  green. `corpus_cases.inc:187-190` warns that a fact table can still arrive in several batches at
  tp1-single, in which case they land on #185 (`in_rows`) rather than refusing.
- `tpcds/q33`, `q77`: a `GpuUnion` under the final aggregate, so its merge sees several partials —
  expect #185. q77 also carries #47 (40 rows against 45) and its `channel:Utf8` literal column
  exports as `Utf8`, which matches.
- `tpcds/q61`: two one-row aggregates under a `GpuCrossJoin` (a single-batch probe, so #152's copy
  is not needed); next is #46 if the promotions sum is still wrong, else green.
- `tpch/q2`: seven `Utf8View` columns at the sink — lands on #183 deterministically.

**Stay off behind another ticket:** the tp1-rowgroup and tp4 cells of those nine (#152; q77 and q90
have no CPU cell at the tp4 modes either, #175 and #180). Every other `187`-free decimal query in
the corpus is unaffected by definition. #183's proposal lists 29 rows whose cells will *arrive* at
#187 once the string agrees; with this fix landed first they pass through instead.

## 7. Risks and unknowns

- **Not run on a device.** That `filter_project` goes green at all five modes rests on the ticket's
  own report that #187 was its only failure and on the plan holding nothing else; the nine tp1-single
  predictions are softer, since #152 at tp1-single is admitted not to be predictive
  (`corpus_cases.inc:223-227`).
- **Scale agreement.** The relabel needs the device's scale to equal the declared one. For scan
  columns it is the file's; for q90's `(23,8)` and q61's `(38,8)` divides it rests on
  `cpp/src/expr.cpp:587-600` pre-scaling to `out_decimal_scale`. A scale that differs is a different
  number and is (rightly) still refused — but keeps that cell off, on a new ticket.
- **cuDF 25.02's exact label.** Read from the vendored 25.10 source (`max_precision<__int128_t>()`
  = 38) and inferred for 25.02 from its header lacking `precision`; the ticket's observed
  `Decimal128(38, 2)` on shad-gpu is the empirical confirmation.
- **Per-value validation cost.** `safe: false` walks every exported decimal once
  (`validate_decimal_precision`); on filter_project that is ~3.4M values per run. Not measured;
  expected well under the PCIe transfer it follows. A future arrow-cast could turn the same-scale
  narrowing into a bare relabel, which stays correct and loses only the overflow check.
- **Message change on the CPU.** `declared_as`'s refusal no longer says "DataFusion answered with";
  no test pins the wording (`grep` over `src/` and `tests/`), one ticket quotes it (#190).
- **Sequencing with two approved tasks.** `sink-divergence-survey` rewrites `gpu_backend/mod.rs:176-184`
  on a throw-away branch — a trivial rebase either way, but its report will not see the decimal class
  if this lands first. `operator-harness`/`operator-cases` expect #187 `bug_` tests; see 3f.
- **Nullability.** `RecordBatch::try_new` refuses a null under a non-nullable declared field, as
  `concat_batches` already did; behaviour unchanged, noted because the new site is where it would
  now be reported.

## 8. Complexity

**S.** Six source files, ~150 LOC net including tests (a ~45-line move, ~10 lines of facade, ~8 at
the unload, ~35 of device test, ~60 of unit tests); no C++, no ABI symbol, no `gpu_plan.fbs`, no
wire-format change, no declared-schema contract change; no golden regenerated — plan text, byte
pricing and rendered results are all precision-blind. Closing the ticket costs one shad-gpu cycle
over the ten registry rows, each enabled or re-ticketed from what the run says.
