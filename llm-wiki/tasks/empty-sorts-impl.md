# empty-sorts implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The cpu's accumulating sort and merge answer one zero-row batch where every batch they
held had zero rows, as the device does (#205); a query whose sink received nothing answers one
zero-row batch under the sink's input schema on both backends; `tpcds/q17`'s result section gains
its header.

**Architecture:** One helper in `cpu_backend/accumulate.rs`, `sorted_and_cut`, serves both sorts:
held batches with no row are concatenated, not sorted. The empty answer is the driver's:
`Driver::answer` (`executor/driver/partitioned.rs`) hands back what the sink unloaded, or one
zero-row batch under the root's input schema when nothing reached it. The corpus and end-to-end
oracles give DataFusion's empty answer its columns the same way, since both comparisons read
names off the first batch.

**Tech stack:** Rust over DataFusion 45 / arrow 54 (`--features rust-only`); the `gpu`-feature
build of `--lib` against cuDF 25.02, compiled and not run.

**Spec:** [`empty-sorts.md`](empty-sorts.md). Fourth of chain K, after
[`limits.md`](limits.md).

## Where the empty answer is made, and what it looks like

- **The driver, not either unload.** `UnloadExecutor::unload` is called once per batch, per lane.
  No call says that no batch came, so neither `CpuUnload` nor `GpuExport` can answer for one. A
  `finish` call on the trait would be a trait change, and the spec says "Component-level API:
  none". The driver is generic over `Backend`, its `results` are already `CpuBatch`, and it alone
  knows the run is over. So it is one rule for both backends by construction.
- **The schema** is the sink's input's declared schema:
  `index.nodes[ROOT].node.children()[0].kind().schema().fields`. `GpuExport::new` is built from
  the same schema (`gpu_backend/backend.rs:155`, `input(0)`), and `check_output_schema`
  (`plan/validate.rs:160`) holds it to DataFusion's planned schema.
- **Not a node's emission.** No executor made the batch and no call moved it, so it is not
  recorded in `emitted`, the trace, `abi_calls` or the accountant. The execution goldens read
  `emitted` (`plan_text/run_text.rs:59`), so q17's tp4 sections do not move, and holds equal
  releases as before.
- **What crosses.** Nothing for the driver's batch: it is built on the host. A zero-row batch
  that reaches the device's unload is exported as today, and `GpuExport::unload` answers it under
  the same sink schema (a zero-length stream becomes `RecordBatch::new_empty(self.schema)`; a
  stream with a zero-row batch is concatenated under it).
- **What a reader sees.** `RunReport::batches` is never empty. `pretty_format_batches` of one
  zero-row batch draws a top border, the header row, the header rule and a bottom border, four
  lines. `batches_to_sorted_str` leaves four lines unsorted, so q17's `mini.result.txt` section
  becomes `mode=tp4-sized` and then those four lines. The CLI's `print_batches`
  (`peacockdb/src/main.rs:61`) prints the same header where it printed `++`/`++`.

## Corpus queries that answer zero rows

Only `tpcds/q17`, and at all five modes. Checked by scanning every `*-mini.cpu.txt` for a
`GpuUnload` with `output_rows=0` and for an accumulating sort or merge with `batch_rows=[[]]`;
q17 alone matches. No `tpch.sf1` result section is empty. What moves:

| mode | why | goldens |
|---|---|---|
| tp1-single, tp1-rowgroup | `GpuAccumulateBatchesAndSort` receives one zero-row batch and now emits it; the unload unloads it | the q17 sections of `tp1-*-mini.cpu.txt` and `.cost.txt` |
| tp4-single, tp4-rowgroup, tp4-sized | every lane is emptied by the scatter, so `GpuMergeSortedPartitions` receives nothing and still answers nothing; the driver supplies the answer | none in `.cpu.txt`; `mini.result.txt` (authored at tp4-sized) gains the header |

So the result section's header comes from the driver's rule, not from the sort fix.

## Global constraints

- **No GPU.** No device run and no GPU cycle (chain K's board header). The task reaches `done` when
  every CI job but the GPU tests is green. The three device cases are built here and run under
  [#281](../tickets/corpus-coverage.md#t281).
- **Builds on limits.** Branch from limits' branch. Limits also edits `driver/partitioned.rs`
  (`settle_limit`, `rows_seen`), `driver/tests/limit.rs`, `corpus_cases.inc`, the registry, the
  build-test counts, `architecture.md` and the ticket files. Recount `build-test.md` from the code
  as limits left it; the numbers below are deltas.
- **Chain L** (keyless-identity) edits `accumulate.rs` and the driver too. Touch only
  `SortedRuns::mark_done_and_fetch`, the tail of `CpuPartitionAccumulator::accumulate_and_fetch`,
  the new helper, and `Driver::report`. Leave `coalesce_or_nothing` and `AggregateBatches` alone.
- **Restriction.** The limit's zero-row drop is #214 and a keyless aggregate over nothing is #199.
  Neither changes here.
- **No facade, trait, ABI or wire change.** `RunReport::batches` gains a doc sentence. No backend
  unload changes.
- **Goldens.** Only the five q17 sections above move: `tp1-single` and `tp1-rowgroup` in
  `.cpu.txt` and `.cost.txt`, plus `mini.result.txt`. A tp4 `.cpu.txt` or any plan golden moving
  is a defect.
- **q17's five cpu corpus cells are red from Task 1 until Task 3 regenerates them.** Expected,
  and no other cell may go red.
- `driver/partitioned.rs` stays under 1000 lines (947 before; this adds about 20).
- Every build and test command runs under `timeout`. Corpus runs on a 15 GiB host use
  `--test-threads=2`; the coordinator may route the full bar to verda
  (`scripts/build-test.sh --host verda --rust-only`).
- Commit messages are at most 10 lines and end with the `Co-Authored-By` trailer. Build in a
  workspace, never in the primary checkout, and never share a cargo target dir across worktrees.

## Review focus

1. **Zero-row batches among rows.** The empty branch must apply only when *no* held batch has a
   row; otherwise the rows are sorted and cut as before. Pinned in Task 1 by
   `a_zero_row_batch_among_rows_is_sorted_with_them`, with a fetch.
2. **A merge whose lanes split between a zero-row batch and nothing.** Something arrived, so the
   answer is one zero-row batch. The pre-fix code sorts that batch and answers nothing. Pinned in
   Task 1 by `a_merge_over_one_zero_row_lane_and_one_empty_lane_answers_one_zero_row_batch`.
3. **`LIMIT 0` at the root.** It is satisfied at `seed`, before any step, and no unload call is
   made. The answer is still one zero-row batch under the input's columns. Pinned in Task 2 by
   `a_zero_fetch_answers_one_zero_row_batch_under_the_sinks_input_columns`.
4. **Zero-row batches that did reach the sink.** A two-lane sink given a zero-row batch per lane
   answers exactly those two, and the driver adds none. Pinned in Task 2 by
   `zero_row_batches_that_reached_the_sink_are_the_answer_and_none_is_added`.
5. **The driver's batch leaking into the record.** If it is counted in `emitted` or the trace,
   the tp4 q17 sections move and `consumed + abandoned == emitted` still holds, so nothing else
   says so. Pinned in Task 2 (`emitted[ROOT]` empty, no `Unload` call) and in Task 3, step 6:
   `git diff --stat` must not list a tp4 golden.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs` | `sorted_and_cut`; both sorts call it |
| `peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs` | eight cases over zero-row batches and no arrival |
| `peacockdb-core/src/executor/driver/partitioned.rs` | `Driver::answer`, read by `report` |
| `peacockdb-core/src/executor/mod.rs` | `RunReport::batches`' doc |
| `peacockdb-core/src/executor/driver/tests/flow.rs`, `limit.rs` | three driver cases |
| `peacockdb-core/src/test_support/corpus.rs`, `test_support/mod.rs` | `oracle_answer`: DataFusion's empty answer with its columns |
| `peacockdb-core/src/tests/end_to_end.rs` | the oracle through `oracle_answer`; the empty ordered query |
| `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` | the three `bug_` pins become agreement cases |
| `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup}-mini.{cpu,cost}.txt`, `mini.result.txt` | q17 |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | q17's comment; its ticket `205` → `281` |
| duckdb-oracle's files, only if it has merged (Task 5) | q17 `duckdb_exact`; the #205 references |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets.md`, `tickets/corpus-coverage.md`, `archive/archived-tickets.md` | the break struck; counts; #205 archived |

---

### Task 1: The cpu's sort and merge answer one zero-row batch over zero-row batches

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/accumulate.rs` (`SortedRuns::mark_done_and_fetch`
  ~l.206, `CpuPartitionAccumulator::accumulate_and_fetch` ~l.245, after `first_rows` ~l.281)
- Test: `peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs`

**Interfaces:** none produced; `sorted_and_cut` is private to `accumulate`.

- [ ] **Step 1: Write the sort cases**, after `a_fetch_keeps_the_rows_whose_keys_win_and_keeps_the_same_ones`:

```rust
/// One batch of zero rows under the columns the node declares — what the device answers
/// there, and what a lane that received only zero-row batches owes.
fn assert_one_empty_batch(out: &[CpuBatch]) {
    assert_eq!(out.len(), 1, "one batch, not nothing");
    assert_eq!(out[0].record_batch().num_rows(), 0);
    assert_eq!(
        out[0].record_batch().schema(),
        columns(&GROUPED).fields,
        "under the columns the node declares"
    );
}

/// DataFusion's sort answers zero rows with no batch at all. The lane received a batch, so
/// it owes one.
#[test]
fn an_accumulating_sort_over_one_zero_row_batch_answers_one_zero_row_batch() {
    assert_one_empty_batch(&drive(accumulating_sort(true, None), vec![numbers(vec![])]));
}

#[test]
fn an_accumulating_sort_over_two_zero_row_batches_answers_one() {
    assert_one_empty_batch(&drive(
        accumulating_sort(true, None),
        vec![numbers(vec![]), numbers(vec![])],
    ));
}

/// A fetch has no rows to cut, and cutting nothing is not answering nothing.
#[test]
fn a_fetch_over_zero_rows_answers_zero_rows() {
    assert_one_empty_batch(&drive(accumulating_sort(false, Some(2)), vec![numbers(vec![])]));
}

/// No arrival at all is still nothing: the lane that received nothing, which a lane that
/// received a zero-row batch is not.
#[test]
fn an_accumulating_sort_that_received_nothing_emits_nothing() {
    assert!(drive(accumulating_sort(true, None), Vec::new()).is_empty());
}

/// A zero-row batch among rows takes the sort's path: the rows are ordered and cut, not
/// skipped because one arrival was empty.
#[test]
fn a_zero_row_batch_among_rows_is_sorted_with_them() {
    let out = drive(
        accumulating_sort(true, Some(3)),
        vec![numbers(vec![5, 2]), numbers(vec![]), numbers(vec![4, 1])],
    );
    assert_eq!(out.len(), 1);
    assert_eq!(values_of(&out[0]), vec![1, 2, 4]);
}
```

- [ ] **Step 2: Write the merge cases**, after `a_fetch_over_the_merge_keeps_the_top_of_every_lane_together`:

```rust
/// Every lane's only arrival a zero-row batch: one zero-row batch at the last done, whatever
/// the fetch.
#[test]
fn a_merge_over_a_zero_row_batch_per_lane_answers_one_zero_row_batch() {
    let out = drive_lanes(
        merge_sorted(2, Some(3)),
        vec![
            (0, Some(numbers(vec![]))),
            (1, Some(numbers(vec![]))),
            (0, None),
            (1, None),
        ],
    );
    assert_one_empty_batch(&out);
}

/// One lane sent a zero-row batch and the other sent nothing. Something arrived, so the
/// merge owes a batch.
#[test]
fn a_merge_over_one_zero_row_lane_and_one_empty_lane_answers_one_zero_row_batch() {
    let out = drive_lanes(
        merge_sorted(2, None),
        vec![(0, Some(numbers(vec![]))), (0, None), (1, None)],
    );
    assert_one_empty_batch(&out);
}

#[test]
fn a_merge_whose_lanes_received_nothing_emits_nothing() {
    assert!(drive_lanes(merge_sorted(2, None), vec![(0, None), (1, None)]).is_empty());
}
```

- [ ] **Step 3: Run them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests::accumulate
```

  Expected: five FAIL with `one batch, not nothing` (left `0`, right `1`):
  `…over_one_zero_row_batch…`, `…over_two_zero_row_batches…`, `a_fetch_over_zero_rows…`,
  `a_merge_over_a_zero_row_batch_per_lane…`, `a_merge_over_one_zero_row_lane…`. Three PASS:
  `…that_received_nothing_emits_nothing`, `a_zero_row_batch_among_rows…` and
  `a_merge_whose_lanes_received_nothing…`, which guard what must not change.

- [ ] **Step 4: Implement.** Add the helper after `first_rows`:

```rust
/// Every held batch as one ordered stream, cut to the fetch and answered as one batch, and
/// nothing where nothing arrived.
///
/// Batches that arrived holding no row are concatenated, not sorted: DataFusion's sort
/// answers zero rows with no batch at all, which [`coalesce_or_nothing`] would read as a lane
/// that received nothing. The device answers one batch of zero rows there, as every other
/// accumulator does. A fetch has no rows to cut.
fn sorted_and_cut(
    sort: &Arc<dyn ExecutionPlan>,
    held: Vec<RecordBatch>,
    fetch: Option<usize>,
    schema: &SchemaRef,
    ctx: &Arc<TaskContext>,
) -> CallResult<Vec<CpuBatch>> {
    if held.iter().all(|batch| batch.num_rows() == 0) {
        return coalesce_or_nothing(schema, &held);
    }
    let sorted = run_node(sort, vec![held], ctx)?;
    let (ordered, _) = coalesce_or_nothing(schema, &sorted)?;
    Ok((
        ordered
            .into_iter()
            .map(|batch| first_rows(batch, fetch))
            .collect(),
        CallStats::default(),
    ))
}
```

  `SortedRuns::mark_done_and_fetch` becomes:

```rust
    fn mark_done_and_fetch(self) -> CallResult<Vec<CpuBatch>> {
        sorted_and_cut(&self.sort, self.held, self.fetch, &self.schema, &self.ctx)
    }
```

  In `CpuPartitionAccumulator::accumulate_and_fetch`, everything after the `partition_major`
  binding (its `is_empty` return, the sort and the cut) becomes:

```rust
        sorted_and_cut(&self.sort, partition_major, self.fetch, &self.schema, &self.ctx)
```

  The helper's `all` is vacuously true over no batch, so `coalesce_or_nothing` answers nothing
  there. That is why both `is_empty` early returns go.

- [ ] **Step 5: Run the accumulator and contract tests.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend
```

  Expected: PASS, the eight new cases included. `rustfmt --edition 2024` on `accumulate.rs` and
  `tests/accumulate.rs` only (`coding-style.md`: name the leaves).

- [ ] **Step 6: Commit.**

```bash
git add peacockdb-core/src/executor/cpu_backend/accumulate.rs peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs
git commit -F - <<'EOF'
#205: the cpu's accumulating sort and merge answer one zero-row batch over zero-row batches

Held batches with no row are concatenated rather than sorted, since DataFusion's sort answers
zero rows with no batch. q17's cpu cells are red until its goldens are regenerated.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 2: A query whose sink received nothing answers one zero-row batch

**Files:**
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs` (`report`, ~l.913; imports)
- Modify: `peacockdb-core/src/executor/mod.rs` (`RunReport::batches`, ~l.633)
- Test: `peacockdb-core/src/executor/driver/tests/flow.rs`, `peacockdb-core/src/executor/driver/tests/limit.rs`

**Interfaces:** `RunReport::batches` is never empty. No signature changes.

- [ ] **Step 1: Write the driver cases.** At the end of `flow.rs` (it already imports `ExecRule`,
  `Script`, `spec` and `plans::*`):

```rust
/// A sink that received no batch still answers: one batch of zero rows under the columns its
/// input declares. The driver makes it, so no node emitted it and no call moved it.
#[test]
fn a_sink_that_received_nothing_answers_one_zero_row_batch_under_its_inputs_columns() {
    let script = Script::default().source("part", vec![vec![]]);
    let report = run(unload(coalesce_all(source("part", 1))).as_ref(), &script);
    assert_eq!(report.batches.len(), 1, "one batch, not none");
    let answer = report.batches[0].record_batch();
    assert_eq!(answer.num_rows(), 0);
    assert_eq!(answer.schema(), schema().fields, "the sink's input's columns");
    assert_eq!(count(&report, CallKind::Unload), 0, "no unload call made it");
    assert!(
        report.emitted[crate::executor::ROOT]
            .iter()
            .all(Vec::is_empty),
        "and the sink is not recorded as having emitted it"
    );
    assert_eq!(report.holds, report.releases);
    assert_eq!(report.in_flight_bytes, 0);
}

/// Zero-row batches that reached the sink are the answer as they are, one per lane, and the
/// driver adds none.
#[test]
fn zero_row_batches_that_reached_the_sink_are_the_answer_and_none_is_added() {
    let script = Script::default()
        .source("part", vec![vec![spec(10, 80)], vec![spec(10, 80)]])
        .with_exec(ExecRule::Empty);
    let report = run(unload(filter(source("part", 2))).as_ref(), &script);
    assert_eq!(report.batches.len(), 2, "one per lane, as the sink unloaded them");
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(count(&report, CallKind::Unload), 2);
}
```

  In `limit.rs`, after `a_zero_fetch_unloads_nothing_at_all_and_the_plan_still_completes`:

```rust
/// A zero fetch is an answer that ran out of rows before anything ran, and it still has its
/// columns.
#[test]
fn a_zero_fetch_answers_one_zero_row_batch_under_the_sinks_input_columns() {
    let report = run(chain(0, Some(0)).as_ref(), &six_batches());
    assert_eq!(report.batches.len(), 1);
    assert_eq!(report.batches[0].record_batch().num_rows(), 0);
    assert_eq!(report.batches[0].record_batch().schema(), schema().fields);
}
```

  If limits renamed `chain` or `six_batches`, use what `limit.rs` has. Do not use
  `assert_accounted` in the first case: no batch carries bytes, so its non-zero peak check is
  not about this.

- [ ] **Step 2: Run them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver::tests
```

  Expected: the `nothing` case and the zero-fetch case FAIL with `left: 0, right: 1`.
  `zero_row_batches_that_reached_the_sink…` PASSES, since it guards against an added batch.

- [ ] **Step 3: Implement.** In `partitioned.rs` add `use datafusion::arrow::array::RecordBatch;`
  and, beside `report`:

```rust
    /// What the query answers: the batches the sink unloaded, in the order it unloaded them,
    /// or one batch of zero rows under the columns the sink's input declares where none
    /// reached it. So an answer's schema never depends on how its rows ran out: a lane that
    /// received nothing, a limit satisfied before anything ran. Here, not in either unload,
    /// which is called per batch and never told that none came; and outside `emitted`, the
    /// trace and the accountant, since no node made it and no call moved it.
    fn answer(&mut self) -> Vec<CpuBatch> {
        if !self.results.is_empty() {
            return std::mem::take(&mut self.results);
        }
        let declared = self.index.nodes[ROOT].node.children()[0]
            .kind()
            .schema()
            .expect("a sink's input declares its columns")
            .fields
            .clone();
        vec![CpuBatch::new(RecordBatch::new_empty(declared))]
    }
```

  `report(self)` becomes `report(mut self)`. Bind `let batches = self.answer();` first, and set
  `batches` from it in place of `batches: self.results`. Every root the driver runs is a sink:
  the planner validates it (`plan/validate.rs:26`), and every driver-test plan is built under
  `unload(…)` (`driver/tests/plans.rs`). So `children()[0]` needs no guard.

  In `executor/mod.rs`, above `pub batches: Vec<CpuBatch>,`:

```rust
    /// The answer. Never empty: a query whose sink received no batch answers one of zero rows
    /// under the sink's input schema.
```

- [ ] **Step 4: Run the driver tests, then the end-to-end tier.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end -- --test-threads=2
```

  Expected: PASS. No existing driver test counts `report.batches`; they sum rows
  (`rows_returned`), and an added zero-row batch sums to nothing. The end-to-end queries all
  answer rows, so none takes the new branch. rustfmt the three touched files.

- [ ] **Step 5: Commit.**

```bash
git add peacockdb-core/src/executor/driver/partitioned.rs peacockdb-core/src/executor/mod.rs peacockdb-core/src/executor/driver/tests/flow.rs peacockdb-core/src/executor/driver/tests/limit.rs
git commit -F - <<'EOF'
#205: a query whose sink received nothing answers one zero-row batch under its schema

The driver makes it, for both backends, from the sink's input schema; no node emitted it,
so no execution golden records it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 3: The oracles' empty answer, the empty ordered query, and q17's goldens

**Files:**
- Modify: `peacockdb-core/src/test_support/corpus.rs` (`collect`, ~l.326, and its two callers, l.153 and l.266)
- Modify: `peacockdb-core/src/test_support/mod.rs` (a delegate)
- Modify: `peacockdb-core/src/tests/end_to_end.rs` (the oracle, ~l.75; one test)
- Modify: `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup}-mini.{cpu,cost}.txt`, `mini.result.txt`
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (~l.270-275), `testdata/cost-registry.csv` (q17's row)

**Interfaces:** `test_support::oracle_answer(ctx: &SessionContext, sql: &str, what: &str) ->
Vec<RecordBatch>` (async, `pub(crate)`), for the corpus and the end-to-end tier.

The spec's query, `n_nationkey < 0`, prunes nation's only row group: its statistics rule the
predicate out. The scan then has no survivors and `partition()` refuses the plan
(`scan_mapping/partition.rs:25`, #282). A predicate the statistics cannot evaluate keeps the scan
and reaches the sort. Distinct-companions made the same choice.

- [ ] **Step 1: Write the end-to-end case**, after the `end_to_end!` lines:

```rust
/// Zero rows through an accumulating sort at every mode: the answer is one column named
/// `n_name`, whether the sink received a zero-row batch or nothing at all. The predicate is
/// one row-group statistics cannot rule out; `n_nationkey < 0` prunes the scan away, which
/// is #282's refusal and not this case.
#[tokio::test]
async fn an_empty_ordered_answer_keeps_its_column() {
    let sql = "select n_name from nation where n_nationkey + n_regionkey < 0 order by n_name;";
    sql_answers_match_datafusion("tpch", "empty-ordered", sql, None, Coverage::ModesOnly).await;
}
```

- [ ] **Step 2: Run it, and q17's cells.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::an_empty_ordered_answer_keeps_its_column
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_ --test-threads=2
```

  Expected: the end-to-end case FAILS at its first mode, `answers different columns from the
  oracle`, left `[("n_name", Utf8)]`, right `[]`. DataFusion's own sort answered with no batch.
  The five q17 cells FAIL with `the column names differ — expected [], actual [...]` for the same
  reason. If the end-to-end case passes here, DataFusion answered this query with a zero-row
  batch; the q17 cells are then this step's red.

- [ ] **Step 3: The oracle's empty answer.** In `corpus.rs`, replace `collect` (and rename its
  two callers) with:

```rust
/// DataFusion's answer, with its columns even where it has no rows. Its sort answers zero
/// rows with no batch at all, where the engine answers one batch of zero rows under the
/// declared columns, and both comparisons read the columns off the first batch.
pub(crate) async fn oracle_answer(ctx: &SessionContext, sql: &str, what: &str) -> Vec<RecordBatch> {
    let frame = ctx
        .sql(sql)
        .await
        .unwrap_or_else(|e| panic!("{what}: the oracle does not plan it: {e}"));
    let schema = Arc::clone(frame.schema().inner());
    let batches = frame
        .collect()
        .await
        .unwrap_or_else(|e| panic!("{what}: the oracle does not run it: {e}"));
    match batches.is_empty() {
        true => vec![RecordBatch::new_empty(schema)],
        false => batches,
    }
}
```

  (`use std::sync::{Arc, Mutex, OnceLock};`.) In `test_support/mod.rs`, beside
  `batches_to_sorted_str`:

```rust
/// DataFusion's answer to `sql`, with its columns where it answers zero rows. `pub(crate)`:
/// its callers are the corpus and the end-to-end tier.
pub(crate) async fn oracle_answer(ctx: &SessionContext, sql: &str, what: &str) -> Vec<RecordBatch> {
    corpus::oracle_answer(ctx, sql, what).await
}
```

  In `end_to_end.rs`, the oracle's `let expected = oracle_ctx.sql(sql)…collect()…` becomes
  `let expected = oracle_answer(&oracle_ctx, sql, &format!("{dataset}/{query}")).await;`, with
  `oracle_answer` added to the `crate::test_support::{…}` import.

- [ ] **Step 4: Run again.** Use the two commands of step 2.

  Expected: the end-to-end case PASSES at all five modes. The q17 cells now FAIL only on golden
  text: the tp1 cells on their `.cpu.txt` sections, and tp4-sized on its `mini.result.txt`
  section.

- [ ] **Step 5: Regenerate q17, merge-only.**

```bash
timeout 3600 env PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_ --test-threads=2
git diff --stat testdata/
```

  Expected `--stat`: exactly `tpcds.sf1/tp1-single-mini.cpu.txt`, `tp1-single-mini.cost.txt`,
  `tp1-rowgroup-mini.cpu.txt`, `tp1-rowgroup-mini.cost.txt` and `mini.result.txt`.

- [ ] **Step 6: Read the diff.**
  - `tp1-*.cpu.txt`: q17's `GpuUnload` and `GpuAccumulateBatchesAndSort` lines go from
    `batch_rows=[[]] batch_bytes=[[]]` to `batch_rows=[[0]] batch_bytes=[[N]]`, and their
    `output_bytes` go from 0 to N. N is the byte size the `GpuSort` below already prints, 12
    today: three `Utf8` columns, one offset each. The unload's `in_rows` stays `[[0]]`. Nothing
    else moves.
  - `mini.result.txt`: q17's `++`/`++` becomes the bordered 15-column header (`i_item_id`,
    `i_item_desc`, `s_state`, then the twelve `store_sales_quantitycount` … `catalog_sales_quantitycov`),
    four lines, under `mode=tp4-sized`.
  - A tp4 `.cpu.txt` in the list means the driver's batch reached `emitted`: go back to Task 2.
    A section missing from a file is #213's lost section: refill it with `PCK_UPDATE_SECTIONS=1`
    and `--exact`.

- [ ] **Step 7: The corpus line's comment and the registry.** In `corpus_cases.inc`, the T19 batch
  19 comment's clause `q17 answers zero rows, and the cpu's accumulating sort emits nothing where
  the device emits the zero-row batch (#205);` becomes `q17 (zero rows) waits on its first device
  run (#281);`. The line itself does not change: `gpu_modes` stays `none`. In
  `testdata/cost-registry.csv`, q17's tpcds row ends `…,ok,stddev_var avg top_n,281` (was `205`).
  A disabled cell must name a ticket (`registry.rs:236`), and #281 is the one that holds it.

- [ ] **Step 8: Verify.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- tpcds_q17 registry --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
```

  Expected: PASS. The cpu registry test is green in both directions, and
  `the_root_emitted_the_rows_the_result_golden_holds` reads one `|` line, the header, as zero rows.

- [ ] **Step 9: Commit.**

```bash
git add peacockdb-core/src/test_support/corpus.rs peacockdb-core/src/test_support/mod.rs peacockdb-core/src/tests/end_to_end.rs testdata/goldens/tpcds.sf1 peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv
git commit -F - <<'EOF'
#205: q17's empty answer carries its header; the oracle's empty answer its columns

DataFusion answers zero rows with no batch, so the corpus and end-to-end oracles take the
columns from its plan. q17's tp1 sections record the sort's zero-row batch; its device row
names #281.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 4: The three device pins become agreement cases

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` (~l.181, 204, 495)

**Interfaces:** none.

These are device-tier tests. They are built here, not run, and their first run is #281's.

- [ ] **Step 1: Replace the three `bug_` cases**, each with its `// #205 —` comment, in place:

```rust
// Zero rows: the lane received a batch, so both answer one batch of zero rows.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn one_zero_row_batch_sorts_to_zero_rows_on_both() {
        run_both(&sorted(None), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}
```

```rust
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn a_fetch_over_zero_rows_is_zero_rows_on_both() {
        run_both(&sorted(Some(5)), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}
```

```rust
operator_case! {
    GpuMergeSortedPartitions,
    fn every_lane_a_zero_row_batch_merges_to_zero_rows_on_both() {
        let lanes = vec![vec![synthetic(0, 1)], vec![synthetic(0, 2)]];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}
```

  If `each_answers` has no caller left, leave it if another file calls it; delete it only if
  the compiler says it is dead.

- [ ] **Step 2: Build the device tier, no run.**

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
grep -c 'bug_.*_on_the_cpu' peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs
```

  Expected: the build succeeds with no new warning. The grep prints `0`.

- [ ] **Step 3: Commit.**

```bash
git add peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs
git commit -F - <<'EOF'
#205: the three accumulate pins are agreement cases, built and not run (#281)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 5: DuckDB, only if chain J's duckdb-oracle has merged

**Files (merged branch only):** `peacockdb-core/tests/common/corpus_cases.inc`,
`peacockdb-core/src/test_support/duckdb_oracle.rs`, `test_support/duckdb_oracle/tests.rs`,
`llm-wiki/build-test.md`, `llm-wiki/tickets/testinfra.md`.

- [ ] **Step 1: Which branch.**

```bash
test -f peacockdb-core/src/test_support/duckdb_oracle.rs && echo merged || echo not-merged
```

- [ ] **Step 2a: `merged`.** As of `origin/ENS-duckdb-oracle` (0df60133), five places name q17's
  divergence or #205. Find them with
  `rg -n '205|q17' peacockdb-core/src/test_support peacockdb-core/tests/common llm-wiki/build-test.md llm-wiki/tickets/testinfra.md`:
  - `corpus_cases.inc`: q17's tpcds line `duckdb_divergent(205)` → `duckdb_exact`. Its comment
    stops citing #205.
  - `duckdb_oracle.rs` (~l.111-115): the row-level comment drops its example, keeping the rule:
    "A ROW-LEVEL divergence — `duckdb_divergent(<ticket>)`, no positions — says the two answer
    different row sets, and the width is part of what differs. Under every other oracle a width
    difference is an engine that dropped a column."
  - `duckdb_oracle/tests.rs`, `a_row_level_divergence_admits_a_column_count_difference`: the doc
    loses "tpcds q17's shape … (#205)" and states the shape. `divergent(205, vec![])` →
    `divergent(251, vec![])`. `compare` does not read the ticket; `compare_sections` does.
  - `duckdb_oracle/tests.rs`, `an_archived_ticket_is_not_open_and_a_listed_one_is`: delete
    `assert!(ticket_is_open(205), …)`, since #205 is archived by Task 6 and would go red. The
    comment becomes "#251 is what `corpus_cases.inc`'s divergent lines name."
  - `build-test.md`'s DuckDB tier: `93 lines are duckdb_exact` → 94, `4 duckdb_divergent` → 3
    (recount from `corpus_cases.inc`). `tickets/testinfra.md`: the CI-skips-docs ticket's
    "asserts `ticket_is_open(205)` and `ticket_is_open(251)`" → `ticket_is_open(251)`, and its
    "#205 is on the archival path already…" sentence goes. #252's "four `duckdb_divergent`
    cases — `duckdb_tpcds_q17`, `q58`, `q61`, `q66`" → three, without q17, and its "Six cases
    are in the class, not four" → "Five …, not three".

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- duckdb_tpcds_q17 --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- test_support::duckdb_oracle
```

  Expected: PASS. Both sides are 15 columns over zero rows. Commit:

```bash
git add peacockdb-core/tests/common/corpus_cases.inc peacockdb-core/src/test_support llm-wiki/build-test.md llm-wiki/tickets/testinfra.md
git commit -F - <<'EOF'
#205: q17 meets DuckDB exactly, header and all; its divergent line goes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Step 2b: `not-merged`.** No code touches DuckDB. Write in
  `llm-wiki/tasks/empty-sorts-detail.md` what duckdb-oracle owes when it rebases over chain K,
  which is step 2a's list: once #205 is archived, its `duckdb_divergent(205)` line and its
  `ticket_is_open(205)` assertion go red. #235's "An empty answer" bullet is Task 6's.

---

### Task 6: The wiki, #205 archived, and the full bar

**Files:**
- Modify: `llm-wiki/architecture.md` (~l.779-781), `llm-wiki/build-test.md`, `llm-wiki/tickets.md`,
  `llm-wiki/tickets/corpus-coverage.md`, `llm-wiki/archive/archived-tickets.md`

- [ ] **Step 1: `architecture.md`, "Zero-row batches change no answer".** The last bullet
  becomes: "- A producer that drops a zero-row batch exposes the breaks above: the limit
  ([#214](tickets/corpus-coverage.md#t214))." Nothing is added. The empty answer's rule is
  stated at `Driver::answer` and `RunReport::batches`.

- [ ] **Step 2: `build-test.md`.** Recount from the code as limits left it, then apply these
  deltas:

| row | delta | description change |
|---|--:|---|
| CPU backend executors | +8 | "…and an accumulating sort or merge over zero-row batches answering one" |
| Drivers over a mock backend | +3 | "…a sink that received nothing answering one zero-row batch under its input's columns" |
| End to end | +1 | "ten cases no query list can carry" → eleven, adding "a query answering zero rows answering them under its column at every mode" |
| cpu block `--lib`, cpu block, Rust, grand total | +12 each | — |

  The device corpus row's "the device's own tickets (#57, #63, #205)" names #281 in place of
  #205, unless limits already added #281 there. The gpu block's counts do not change: renames
  only.

- [ ] **Step 3: Archive #205.** Move its block, anchor included, from `corpus-coverage.md` to the
  top of `archive/archived-tickets.md`'s `## Done`, and append:

  > **Done.** The accumulating sort and merge answer held batches with no row as one zero-row
  > batch, unsorted (`sorted_and_cut`, `cpu_backend/accumulate.rs`). A query whose sink received
  > nothing answers one zero-row batch under the sink's input schema, made by the driver
  > (`Driver::answer`, `driver/partitioned.rs`). The corpus oracle gives DataFusion's empty
  > answer its columns the same way. q17's result section carries its header. The three `bug_`
  > pins are agreement cases, built and not run (#281).

  Remove #205 from `corpus-coverage.md`'s contents list (l.24) and from `tickets.md`'s
  corpus-coverage row. Decrement that row's count and the open total.

- [ ] **Step 4: Sentences #205's fix falsified.**
  - `corpus-coverage.md`, #199's fix: "Sources of nothing below the init (#214's limit, #205's
    sort, the joins)" → "(#214's limit, the joins)".
  - `corpus-coverage.md`, #235's expected divergences, only if #235 is still open (Task 5 took
    `not-merged`): strike the "**An empty answer.**" bullet. q17 now renders its header.
  - "as #205 says" in #214 (`corpus-coverage.md`) and the cross-join ticket (`joins.md`) stay
    as they are. The number resolves in the archive.

- [ ] **Step 5: The full bar.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 7200 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_golden_format --test test_module_layout
git diff --stat <limits-branch>..HEAD -- testdata/
```

  Expected: all green; the cpu registry test passes in both directions; the `testdata` diff is
  Task 3's five goldens and the registry row, plus `corpus_cases.inc` if Task 5 merged. The device
  tier was built in Task 4 and is not run (#281).

- [ ] **Step 6: Commit.**

```bash
git add llm-wiki
git commit -F - <<'EOF'
#205 archived: the break struck from architecture.md; counts; #199's sources of nothing

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```
