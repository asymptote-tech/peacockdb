# empty-sorts implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A query whose sink received nothing answers one zero-row batch under the sink's input
schema on both backends, and DataFusion's empty answer, which the corpus and end-to-end tiers
compare against, gets its columns the same way; the cpu's accumulating sort and merge answer one
zero-row batch where every batch they held had zero rows, as the device does (#205);
`tpcds/q17`'s result section gains its header.

**Architecture:** The empty answer is the driver's: `Driver::answer`
(`executor/driver/partitioned.rs`) hands back what the sink unloaded, or one zero-row batch under
the root's input schema when nothing reached it. The oracles' is `test_support::oracle_answer`,
which takes the columns from DataFusion's plan where it collected no batch. One helper in
`cpu_backend/accumulate.rs`, `sorted_and_cut`, serves both sorts: held batches with no row are
concatenated, not sorted.

**Tech stack:** Rust over DataFusion 45 / arrow 54 (`--features rust-only`); the `gpu`-feature
build of `--lib` against cuDF 25.02, compiled on this host and not run.

**Spec:** [`empty-sorts.md`](empty-sorts.md). Fourth of chain K, after [`limits.md`](limits.md).
Pre-build review: blocking B1, important I1 and I4, minor 4, 5, 6, 8, 9, 10, 11, 14, 15 and 16
are answered here.

## Where the empty answer is made, and what it looks like

- **The driver, not either unload.** `UnloadExecutor::unload` is called once per batch, per lane.
  No call says that no batch came, so neither `CpuUnload` nor `GpuExport` can answer for one. A
  `finish` call on the trait would be a trait change. The driver is generic over `Backend`, its
  `results` are already `CpuBatch`, and it alone knows the run is over. So it is one rule for
  both backends by construction.
- **The schema** is the sink's input's declared schema:
  `index.nodes[ROOT].node.children()[0].kind().schema().fields`. `GpuExport::new` is built from
  the same schema (`gpu_backend/backend.rs:155`, `input(0)`), and `check_output_schema`
  (`plan/validate.rs:160`) holds it to DataFusion's planned schema.
- **Not a node's emission.** No executor made the batch and no call moved it, so it is not
  recorded in `emitted`, the trace, `abi_calls` or the accountant. The execution goldens read
  `emitted` (`plan_text/run_text.rs:59`), so no `.cpu.txt` moves with the driver's batch, and
  holds equal releases as before.
- **What crosses.** Nothing for the driver's batch: it is built on the host. A zero-row batch
  that reaches the device's unload is exported as today, and `GpuExport::unload` answers it under
  the same sink schema.
- **What a reader sees.** `RunReport::batches` is never empty. `RunReport` is `pub` and the CLI
  reads `batches` (`peacockdb/src/main.rs:56-61`), so this is a documented guarantee on a public
  field, with no signature change; the spec's "Component-level API: none" does not mention it,
  and Task 2 Step 13 records that for the human. `pretty_format_batches` of one zero-row batch
  draws a top border, the header row, the header rule and a bottom border, four lines, which
  `batches_to_sorted_str` leaves unsorted.

## Corpus queries that answer zero rows

At 0753f8c9, only `tpcds/q17`, at all five modes. Task 1 Step 1 re-runs the scan on the branch as
it stands, since distinct-companions' and limits' new queries could add one. What moves:

| mode | why | goldens | commit |
|---|---|---|---|
| all five | the sink receives nothing (tp1: the sort emits nothing before Task 2; tp4: the scatter empties every lane of the merge), so the driver answers | `mini.result.txt` (authored at tp4-sized) gains the header; no `.cpu.txt` | Task 1 |
| tp1-single, tp1-rowgroup | `GpuAccumulateBatchesAndSort` receives one zero-row batch and now emits it; the unload unloads it | q17's sections of `tp1-*-mini.cpu.txt` and `.cost.txt` | Task 2 |
| tp4-single, tp4-rowgroup, tp4-sized | `GpuMergeSortedPartitions` receives nothing and still answers nothing | none | — |

So the result section's header comes from the driver's rule, not from the sort fix.

## Global constraints

- **No GPU.** No device run and no GPU cycle (chain K's board header). The task reaches `done` when
  every CI job but the GPU tests is green. The device cases are built here and run under
  [#281](../tickets/corpus-coverage.md#t281). The `--features gpu --no-run` build in Task 3 is a
  local compile against cuDF 25.02 on this host, which the header's "every build and run is
  local … the C++ build against cuDF 25.02" covers; CI's `cpp-build-2502` compiles the same
  feature. No GPU host is involved.
- **Every commit is green.** The driver's answer, the oracle's answer, the empty ordered query
  and q17's result golden land together (Task 1); the sort and merge fix lands with q17's tp1
  goldens (Task 2). Between them no corpus cell is red.
- **The CI cost gate is red on this PR, and only the human can accept it.** The `cost-report` job
  (`pipeline.yml:886-939`) fails on any section whose `peacockdb_cost=` rises against the PR's
  base, exactly, no tolerance (`cost-report/src/main.rs:1210`). It is a CPU job, so it is inside
  "every CI job but the GPU tests". q17's two tp1 sections rise by 24 bytes each in Task 2.
  Task 2 Step 13 runs the same diff locally and records every rising section in
  `empty-sorts-detail.md` for the human; Task 5 Step 7 re-runs it over the whole branch.
- **Builds on limits.** Branch `ENS-empty-sorts` from `ENS-limits`. Limits (and distinct-companions
  under it) also edit `driver/partitioned.rs` (`settle_limit`, `rows_seen`, `rows_emitted`),
  `driver/tests/limit.rs` and `mock.rs`, `tests/end_to_end.rs` (distinct-companions moves the
  oracle into `sql_answers_match_oracle`), `corpus_cases.inc`, the registry, the build-test
  counts, `architecture.md` and the ticket files. Find by the quoted text, not by line number.
  Recount `build-test.md` from the code as limits left it; the numbers below are deltas.
- **Chain L** (keyless-identity) edits `accumulate.rs` and the driver too. Touch only
  `SortedRuns::mark_done_and_fetch`, the tail of `CpuPartitionAccumulator::accumulate_and_fetch`,
  the new helper, and `Driver::report` with the new `Driver::answer`. Leave `coalesce_or_nothing`,
  its doc and `AggregateBatches` alone.
- **Restriction.** The limit's zero-row drop is #214 and a keyless aggregate over nothing is #199.
  Neither changes here.
- **No facade, trait, ABI or wire change.** `RunReport::batches` gains a doc sentence. No backend
  unload changes.
- **Goldens.** Only `mini.result.txt`'s q17 section (Task 1) and q17's `tp1-single` and
  `tp1-rowgroup` sections of `.cpu.txt` and `.cost.txt` (Task 2) move, unless Task 1 Step 1 finds
  more. A tp4 `.cpu.txt` or any plan golden moving is a defect.
- **#205 is archived on this branch** (Task 5), because the spec says so (`empty-sorts.md:41`,
  "#205 is archived here", and its Scope row for `tickets/`). That differs from limits, guard-checks
  and distinct-companions, which leave their tickets to the helper at merge; the spec's DuckDB
  clause depends on it.
- `driver/partitioned.rs` stays under 1000 lines (947 at master; limits adds about 20; this adds
  about 20). Task 1 Step 5 checks it.
- Every build and test command runs under `timeout`. Corpus runs on a 15 GiB host use
  `--test-threads=2`; the coordinator may route the full bar to verda
  (`scripts/build-test.sh --host verda --rust-only`).
- rustfmt only the leaf files touched (`rustfmt --edition 2024 <file>…`), never a `mod.rs`.
  rustfmt follows `mod` declarations (`end_to_end.rs` has four), so restore any file it
  reformatted that the task did not touch (`git checkout -- <path>`).
- Commit messages are at most 10 lines and end with the `Co-Authored-By` trailer. Build in a
  workspace, never in the primary checkout, and never share a cargo target dir across worktrees.

## Review focus

1. **Zero-row batches among rows.** The empty branch must apply only when *no* held batch has a
   row; otherwise the rows are sorted and cut as before. Pinned in Task 2 by
   `a_zero_row_batch_among_rows_is_sorted_with_them`, with a fetch.
2. **A merge whose lanes split between a zero-row batch and nothing.** Something arrived, so the
   answer is one zero-row batch. The pre-fix code sorts that batch and answers nothing. Pinned in
   Task 2 by `a_merge_over_one_zero_row_lane_and_one_empty_lane_answers_one_zero_row_batch`, and
   on the device by `one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both`
   (Task 3, built).
3. **`LIMIT 0` at the root.** It is satisfied at `seed`, before any step, and no unload call is
   made. The answer is still one zero-row batch under the input's columns. Pinned in Task 1 by
   `a_zero_fetch_answers_one_zero_row_batch_under_the_sinks_input_columns`.
4. **Zero-row batches that did reach the sink.** A two-lane sink given a zero-row batch per lane
   answers exactly those two, and the driver adds none. Pinned in Task 1 by
   `zero_row_batches_that_reached_the_sink_are_the_answer_and_none_is_added`.
5. **The driver's batch leaking into the record.** If it is counted in `emitted` or the trace,
   q17's `.cpu.txt` sections move in Task 1 and `consumed + abandoned == emitted` still holds, so
   nothing else says so. Pinned in Task 1 (`emitted[ROOT]` empty, no `Unload` call) and by Task 1
   Step 11: `git diff --stat` lists `mini.result.txt` alone.
6. **The answer cannot tell the sort fix from the driver's.** Both give the nation query its
   column. `an_empty_sort_and_merge_each_emit_one_zero_row_batch` (Task 2) reads the sort's and
   the merge's own emissions off the run, so reverting either half of `sorted_and_cut` reddens it.

## File structure

| file | responsibility | task |
|---|---|---|
| `peacockdb-core/src/executor/driver/partitioned.rs` | `Driver::answer`, read by `report` | 1 |
| `peacockdb-core/src/executor/mod.rs` | `RunReport::batches`' doc | 1 |
| `peacockdb-core/src/executor/driver/tests/flow.rs`, `limit.rs` | three driver cases | 1 |
| `peacockdb-core/src/test_support/corpus.rs`, `test_support/mod.rs` | `oracle_answer`: DataFusion's empty answer with its columns | 1 |
| `peacockdb-core/src/tests/end_to_end.rs` | the oracle through `oracle_answer`; the empty ordered query (1); the empty sort and merge read off the run (2) | 1, 2 |
| `testdata/goldens/tpcds.sf1/mini.result.txt` | q17's header | 1 |
| `peacockdb-core/src/executor/cpu_backend/accumulate.rs` | `sorted_and_cut`; both sorts call it | 2 |
| `peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs` | eight cases over zero-row batches and no arrival | 2 |
| `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup}-mini.{cpu,cost}.txt` | q17's sort emits its zero-row batch | 2 |
| `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | q17's comment; its ticket `205` → `281` | 2 |
| `llm-wiki/tasks/empty-sorts-detail.md` | the cost-gate rows and the API note, for the human | 2, 5 |
| `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` | the three `bug_` pins become agreement cases; the mixed merge | 3 |
| duckdb-oracle's files, only if it has merged | q17 `duckdb_exact`; the #205 references | 4 |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets.md`, `tickets/corpus-coverage.md`, `archive/archived-tickets.md` | the break struck; the Python model's gap; counts; #281's q17 modes; #235's line; #205 archived | 5 |

---

### Task 1: A query whose sink received nothing answers one zero-row batch, and so does the oracle

**Files:**
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs` (`report`, ~l.913; imports)
- Modify: `peacockdb-core/src/executor/mod.rs` (`RunReport::batches`, ~l.633)
- Test: `peacockdb-core/src/executor/driver/tests/flow.rs`, `peacockdb-core/src/executor/driver/tests/limit.rs`
- Modify: `peacockdb-core/src/test_support/corpus.rs` (`collect`, ~l.326, and its two callers,
  `oracle_rows` ~l.153 and `count_of` ~l.266), `peacockdb-core/src/test_support/mod.rs` (a delegate)
- Modify: `peacockdb-core/src/tests/end_to_end.rs` (the oracle in `sql_answers_match_oracle`; one test)
- Modify: `testdata/goldens/tpcds.sf1/mini.result.txt` (q17)

**Interfaces:** `RunReport::batches` is never empty. `test_support::oracle_answer(ctx:
&SessionContext, sql: &str, what: &str) -> Vec<RecordBatch>` (async, `pub(crate)`), for the corpus
and the end-to-end tier. No signature changes.

- [ ] **Step 1: Recheck which corpus queries answer zero rows.** Write the scan outside the tree:

```bash
mkdir -p /tmp/empty-sorts && cat > /tmp/empty-sorts/scan.awk <<'EOF'
# A node line is followed by its stats line, so a sort's stats line is the line after it and
# its one child's stats line two lines further on.
/^== /{q=$2; next}
/^ *(GpuAccumulateBatchesAndSort|GpuMergeSortedPartitions):/{
  node=$1; getline own; getline child; getline fed
  if (own ~ /batch_rows=\[(\[\],?)+\]/ && fed ~ /batch_rows=\[.*[0-9]/)
    print FILENAME": "q": "node" fed zero-row batches, emitted none"
}
/^GpuUnload: output_rows=0,/{print FILENAME": "q": GpuUnload answered zero rows"}
EOF
awk -f /tmp/empty-sorts/scan.awk testdata/goldens/*/*-mini.cpu.txt
```

  Expected, exactly these seven lines (as at 0753f8c9):

```
testdata/goldens/tpcds.sf1/tp1-rowgroup-mini.cpu.txt: q17: GpuUnload answered zero rows
testdata/goldens/tpcds.sf1/tp1-rowgroup-mini.cpu.txt: q17: GpuAccumulateBatchesAndSort: fed zero-row batches, emitted none
testdata/goldens/tpcds.sf1/tp1-single-mini.cpu.txt: q17: GpuUnload answered zero rows
testdata/goldens/tpcds.sf1/tp1-single-mini.cpu.txt: q17: GpuAccumulateBatchesAndSort: fed zero-row batches, emitted none
testdata/goldens/tpcds.sf1/tp4-rowgroup-mini.cpu.txt: q17: GpuUnload answered zero rows
testdata/goldens/tpcds.sf1/tp4-single-mini.cpu.txt: q17: GpuUnload answered zero rows
testdata/goldens/tpcds.sf1/tp4-sized-mini.cpu.txt: q17: GpuUnload answered zero rows
```

  Each extra `GpuUnload` line is another result section that gains its header in this task (add
  its `mini.result.txt` to Step 11's expected `--stat` and its cells to every `cpu_tpcds_q17_`
  filter below). Each extra sort line is another `.cpu.txt` and `.cost.txt` section that moves in
  Task 2 (add it to Task 2's Step 8 and Step 13 expectations). Write every extra line into
  `empty-sorts-detail.md`.

- [ ] **Step 2: Write the driver cases.** At the end of `flow.rs` (it already imports `ExecRule`,
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

- [ ] **Step 3: Run them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver::tests
```

  Expected: `a_sink_that_received_nothing…` and `a_zero_fetch_answers…` FAIL with `left: 0,
  right: 1`. `zero_row_batches_that_reached_the_sink…` PASSES: it guards against an added batch.

- [ ] **Step 4: Implement.** In `partitioned.rs` add `use datafusion::arrow::array::RecordBatch;`
  and, beside `report`:

```rust
    /// The batches the sink unloaded, in order, or one of zero rows under the columns the
    /// sink's input declares where none reached it: an answer's schema never depends on how
    /// its rows ran out. Here because the unload is never told that none came; outside
    /// `emitted`, the trace and the accountant because no node made it.
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

- [ ] **Step 5: Run the driver tests; check the file budget.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver
wc -l peacockdb-core/src/executor/driver/partitioned.rs
```

  Expected: PASS. No existing driver test counts `report.batches`; they sum rows
  (`rows_returned`), and an added zero-row batch sums to nothing. `wc -l` prints under 1000; at
  1000 or more, cut `answer`'s doc to its first sentence.

- [ ] **Step 6: Write the end-to-end case**, after the `end_to_end!` lines in `end_to_end.rs`:

```rust
/// Zero rows through an ordered query at every mode: the answer is one column named `n_name`,
/// whether the sink received a zero-row batch or nothing at all. The predicate is one
/// row-group statistics cannot rule out; `n_nationkey < 0` prunes the scan away, which is
/// #282's refusal and not this case.
#[tokio::test]
async fn an_empty_ordered_answer_keeps_its_column() {
    let sql = "select n_name from nation where n_nationkey + n_regionkey < 0 order by n_name;";
    sql_answers_match_datafusion("tpch", "empty-ordered", sql, None, Coverage::ModesOnly).await;
}
```

- [ ] **Step 7: Run it, and q17's cells.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::an_empty_ordered_answer_keeps_its_column
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_ --test-threads=2
```

  Expected: the end-to-end case FAILS at its first mode, `answers different columns from the
  oracle`, left `[("n_name", Utf8)]`, right `[]`: the engine now answers with the column and
  DataFusion's sort answered with no batch. The five q17 cells FAIL with `the column names differ
  — expected [], actual [...]`, the fifteen names. If the end-to-end case passes here, DataFusion
  answered this query with a zero-row batch; the q17 cells are then this step's red.

- [ ] **Step 8: The oracle's empty answer.** In `corpus.rs`, `use std::sync::{Mutex, OnceLock};`
  becomes `use std::sync::{Arc, Mutex, OnceLock};`, and `collect` is replaced by the following,
  its two callers (`oracle_rows`, `count_of`) calling `oracle_answer` instead:

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

  In `test_support/mod.rs`, beside `batches_to_sorted_str`. The type is spelled in full because
  `mod.rs` imports `SessionContext` only under `not(feature = "rust-only")` (l.36-37), and the
  attribute is `cpu_schema_validator`'s (l.613), for the same reason:

```rust
/// DataFusion's answer to `sql`, with its columns where it answers zero rows. Its only caller
/// is the end-to-end tier, so a `test-support` build without `test` sees it as dead.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn oracle_answer(
    ctx: &datafusion::execution::context::SessionContext,
    sql: &str,
    what: &str,
) -> Vec<RecordBatch> {
    corpus::oracle_answer(ctx, sql, what).await
}
```

  In `end_to_end.rs`, inside `sql_answers_match_oracle` (distinct-companions moved the oracle
  there; it runs `oracle_sql`, never `sql`), the oracle's
  `let expected = oracle_ctx.sql(oracle_sql)…collect()…;` becomes:

```rust
    let expected = oracle_answer(&oracle_ctx, oracle_sql, &format!("{dataset}/{query}")).await;
```

  with `oracle_answer` added to the `crate::test_support::{…}` import. Passing `sql` would send
  `distinct_functions_answer_as_their_hand_lowered_form`'s engine SQL to DataFusion, which
  refuses `stddev(DISTINCT)`.

- [ ] **Step 9: Run again.** The two commands of Step 7, then the oracle-variant case:

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::distinct_functions_answer_as_their_hand_lowered_form
```

  Expected: the end-to-end case PASSES at all five modes, and so does the distinct-functions
  case. Of the q17 cells only `cpu_tpcds_q17_tp4_sized` FAILS, on its `mini.result.txt` section:
  tp4-sized is the mode that authors it (`corpus.rs`, `authoritative_mode`), and the section now
  holds a header. The four others PASS: no `.cpu.txt` moved.

- [ ] **Step 10: Regenerate q17's result section.**

```bash
timeout 3600 env PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_ --test-threads=2
git diff --stat testdata/
```

- [ ] **Step 11: Read the diff.** Expected `--stat`: `testdata/goldens/tpcds.sf1/mini.result.txt`
  alone, q17's `++`/`++` becoming the bordered 15-column header (`i_item_id`, `i_item_desc`,
  `s_state`, then the twelve `store_sales_quantitycount` … `catalog_sales_quantitycov`), four
  lines under `mode=tp4-sized`. A `.cpu.txt` or `.cost.txt` in the list means the driver's batch
  reached `emitted`: go back to Step 4. A section missing from a file is #213's lost section:
  refill it with `PCK_UPDATE_SECTIONS=1` and `--exact`.

- [ ] **Step 12: Verify.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- tpcds_q17 registry --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end executor::driver --test-threads=2
```

  Expected: PASS. `the_root_emitted_the_rows_the_result_golden_holds` reads q17's one `|` line,
  the header, as zero rows (`rendered_rows` saturates). The end-to-end queries all answer rows
  except the new case, so no other takes the new branch. Then rustfmt the leaves:

```bash
rustfmt --edition 2024 peacockdb-core/src/executor/driver/partitioned.rs peacockdb-core/src/executor/driver/tests/flow.rs peacockdb-core/src/executor/driver/tests/limit.rs peacockdb-core/src/test_support/corpus.rs peacockdb-core/src/tests/end_to_end.rs
git status --short
```

  Restore with `git checkout --` any file `git status` lists that this task did not touch.

- [ ] **Step 13: Commit.**

```bash
git add peacockdb-core/src/executor/driver/partitioned.rs peacockdb-core/src/executor/mod.rs peacockdb-core/src/executor/driver/tests/flow.rs peacockdb-core/src/executor/driver/tests/limit.rs peacockdb-core/src/test_support/corpus.rs peacockdb-core/src/test_support/mod.rs peacockdb-core/src/tests/end_to_end.rs testdata/goldens/tpcds.sf1/mini.result.txt
git commit -F - <<'EOF'
#205: an empty answer keeps its columns, on both backends and in the oracle

The driver answers one zero-row batch under the sink's input schema where nothing reached
the sink; no node emitted it, so no execution golden records it. DataFusion's empty answer
takes its columns from the plan. q17's result section gains its header.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 2: The cpu's sort and merge answer one zero-row batch over zero-row batches

**Files:**
- Modify: `peacockdb-core/src/executor/cpu_backend/accumulate.rs` (`SortedRuns::mark_done_and_fetch`
  ~l.206, `CpuPartitionAccumulator::accumulate_and_fetch` ~l.245, after `first_rows` ~l.281)
- Test: `peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs`
- Test: `peacockdb-core/src/tests/end_to_end.rs` (one test)
- Modify: `testdata/goldens/tpcds.sf1/{tp1-single,tp1-rowgroup}-mini.{cpu,cost}.txt` (q17)
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (the T19 batch 19 comment, ~l.270-275),
  `testdata/cost-registry.csv` (q17's tpcds row)
- Create or modify: `llm-wiki/tasks/empty-sorts-detail.md` (Step 13)

**Interfaces:** none produced; `sorted_and_cut` is private to `accumulate`.

- [ ] **Step 1: Write the sort cases** in `cpu_backend/tests/accumulate.rs`, after
  `a_fetch_keeps_the_rows_whose_keys_win_and_keeps_the_same_ones`:

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

- [ ] **Step 3: Write the integration case** in `end_to_end.rs`, after
  `an_empty_ordered_answer_keeps_its_column`. The nation query is one lane at every mode, and the
  answer cannot tell the sort fix from the driver's; customer is two row groups (lanes
  `[[0]],[[1]],[],[]` at tp4-single), so the tp4 modes feed the merge two lanes of zero-row
  batches and two lanes of nothing:

```rust
/// What the answer cannot show, read off the run: the accumulating sort and the merge each
/// emit one zero-row batch over zero-row batches. The driver answers zero rows under the
/// columns whether they did or not, so the answer alone passes with either fix reverted.
/// customer is two row groups, so the tp4 modes feed the merge two lanes that each sent a
/// zero-row batch and two that sent nothing.
#[tokio::test]
async fn an_empty_sort_and_merge_each_emit_one_zero_row_batch() {
    use crate::executor::CpuBackend;
    use crate::plan::{GpuNode, NodeRef, as_node_ref};

    fn preorder<'a>(node: &'a dyn GpuNode, out: &mut Vec<&'a dyn GpuNode>) {
        out.push(node);
        for child in node.children() {
            preorder(child, out);
        }
    }

    let sql = "select c_name from customer where c_custkey + c_nationkey < 0 order by c_name;";
    sql_answers_match_datafusion("tpch", "empty-merge", sql, None, Coverage::ModesOnly).await;
    let data_dir = data_dir_for("tpch", "1");
    let (mut sorts_fed, mut merges_fed) = (0, 0);
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql(sql)
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("empty-merge at {name}: {error}"));
        let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|error| panic!("empty-merge at {name}: {error}"));
        let mut nodes = Vec::new();
        preorder(tree.as_ref(), &mut nodes);
        for (at, node) in nodes.iter().enumerate() {
            let merges = match as_node_ref(*node) {
                NodeRef::AccumulateBatchesAndSort(_) => false,
                NodeRef::MergeSortedPartitions(_) => true,
                _ => continue,
            };
            // Both have one child, which is the next node in preorder. A sort owes per lane;
            // the merge owes once, on its one lane, if any input lane sent a batch.
            let fed: Vec<bool> = report.emitted[at + 1]
                .iter()
                .map(|lane| !lane.is_empty())
                .collect();
            let owed: Vec<bool> = match merges {
                true => vec![fed.iter().any(|sent| *sent)],
                false => fed,
            };
            assert_eq!(report.emitted[at].len(), owed.len(), "{} lanes", node.name());
            for (lane, (out, owes)) in report.emitted[at].iter().zip(&owed).enumerate() {
                let rows: Vec<u64> = out.iter().map(|batch| batch.rows).collect();
                let expected: Vec<u64> = if *owes { vec![0] } else { Vec::new() };
                assert_eq!(rows, expected, "empty-merge at {name}: {} lane {lane}", node.name());
            }
            let fed_lanes = owed.iter().filter(|owes| **owes).count();
            match merges {
                true => merges_fed += fed_lanes,
                false => sorts_fed += fed_lanes,
            }
        }
    }
    assert!(sorts_fed > 0, "no mode fed an accumulating sort a zero-row batch");
    assert!(merges_fed > 0, "no mode fed a merge a zero-row batch");
}
```

- [ ] **Step 4: Run them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend::tests::accumulate
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::an_empty_sort_and_merge_each_emit_one_zero_row_batch
```

  Expected: five unit cases FAIL with `one batch, not nothing` (left `0`, right `1`):
  `…over_one_zero_row_batch…`, `…over_two_zero_row_batches…`, `a_fetch_over_zero_rows…`,
  `a_merge_over_a_zero_row_batch_per_lane…`, `a_merge_over_one_zero_row_lane…`. Three PASS:
  `…that_received_nothing_emits_nothing`, `a_zero_row_batch_among_rows…` and
  `a_merge_whose_lanes_received_nothing…`, which guard what must not change. The end-to-end case
  passes its answer check (Task 1) and FAILS at the first mode with a fed
  `GpuAccumulateBatchesAndSort`, `left: [], right: [0]`. If instead it fails on
  `no mode fed a merge`, the plan scatters customer before the merge at every tp4 mode: record
  the tp4 plan text in the detail file and stop, since the merge half then has no integration
  case.

- [ ] **Step 5: Implement.** In `accumulate.rs`, add the helper after `first_rows`:

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

- [ ] **Step 6: Run the cpu backend and both end-to-end cases.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- executor::cpu_backend
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- tests::end_to_end::an_empty
```

  Expected: PASS, the eight unit cases and both `an_empty…` cases included.

- [ ] **Step 7: q17's tp1 cells.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_ --test-threads=2
```

  Expected: `cpu_tpcds_q17_tp1_single` and `cpu_tpcds_q17_tp1_rowgroup` FAIL on their `.cpu.txt`
  sections; the three tp4 cells PASS.

- [ ] **Step 8: Regenerate them.**

```bash
timeout 3600 env PCK_UPDATE_SECTIONS=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpcds_q17_tp1 --test-threads=2
git diff --stat testdata/
```

  Expected `--stat`: exactly `tpcds.sf1/tp1-single-mini.cpu.txt`, `tp1-single-mini.cost.txt`,
  `tp1-rowgroup-mini.cpu.txt` and `tp1-rowgroup-mini.cost.txt`, plus any section Task 1 Step 1
  added.

- [ ] **Step 9: Read the diff.**
  - `.cpu.txt`: q17's `GpuUnload` and `GpuAccumulateBatchesAndSort` stats lines go from
    `batch_rows=[[]] batch_bytes=[[]]` to `batch_rows=[[0]] batch_bytes=[[12]]`, and their
    `output_bytes` from 0 to 12, the byte size the `GpuSort` below already prints (three `Utf8`
    columns, one 4-byte offset each). The unload's `in_rows` stays `[[0]]`. Nothing else moves.
  - `.cost.txt`: `cuda_sort_bytes` 12 → 24, `vram_to_ram_bytes` 0 → 12, `peacockdb_cost` +24
    (293556446 → 293556470 at tp1-single).
  - A tp4 file in the list means the merge changed what it emits over nothing: go back to Step 5.

- [ ] **Step 10: The corpus line's comment and the registry.** In `corpus_cases.inc`, the T19
  batch 19 comment's clause `q17 answers zero rows, and the cpu's accumulating sort emits nothing
  where the device emits the zero-row batch (#205);` becomes `q17 (zero rows) waits on its first
  device run (#281);`. The line itself does not change: its device modes stay `none`. In
  `testdata/cost-registry.csv`, q17's tpcds row ends `…,ok,stddev_var avg top_n,281` (was `205`).
  A disabled cell must name a ticket (`registry.rs:236`), and #281 holds all five.

- [ ] **Step 11: Verify.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- tpcds_q17 registry --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
rustfmt --edition 2024 peacockdb-core/src/executor/cpu_backend/accumulate.rs peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs peacockdb-core/src/tests/end_to_end.rs
git status --short
```

  Expected: PASS; the cpu registry test is green in both directions. Restore with
  `git checkout --` any file rustfmt touched that this task did not.

- [ ] **Step 12: Commit.**

```bash
git add peacockdb-core/src/executor/cpu_backend/accumulate.rs peacockdb-core/src/executor/cpu_backend/tests/accumulate.rs peacockdb-core/src/tests/end_to_end.rs testdata/goldens/tpcds.sf1 peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv
git commit -F - <<'EOF'
#205: the cpu's accumulating sort and merge answer one zero-row batch over zero-row batches

Held batches with no row are concatenated rather than sorted, since DataFusion's sort answers
zero rows with no batch. q17's tp1 sections record the sort's batch; its device row names
#281. An end-to-end case reads both nodes' emissions off the run.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Step 13: The cost gate, locally.** The same diff CI's `cost-report` job runs
  (`pipeline.yml:886-939`), against the PR's base, `ENS-limits`, with its outputs outside the
  tree (`--html` defaults to `cost_diff.html` in the working directory):

```bash
base=$(git merge-base HEAD ENS-limits)
timeout 900 cargo run -q -p cost-report -- --cost-diff --base "$base" --html /tmp/empty-sorts/cost_diff.html --md-diff /tmp/empty-sorts/cost_diff.md; echo "rc=$?"
cat /tmp/empty-sorts/cost_diff.md
```

  Expected: stderr ends `cost-diff: N compared, 2 changed, 2 regression(s)`, `rc=1`, and the
  table's two 🔴 rows are `tpcds.sf1/q17 tp1-rowgroup-mini` and `tpcds.sf1/q17 tp1-single-mini`,
  plus one per extra section Task 1 Step 1 found. Any other row is a defect: find the commit that
  moved it. Then write into `llm-wiki/tasks/empty-sorts-detail.md` (create it if absent), under
  `## For the human`:
  - **The cost gate is red on this PR, by design.** Every 🔴 row of the table above with its base
    and branch `peacockdb_cost=` read from `git diff "$base" -- testdata/goldens/tpcds.sf1/*.cost.txt`
    (+24 bytes each: the sort and the unload each emit the 12-byte zero-row batch the device
    emits). CI fails `cost-report` on any rise (`cost-report/src/main.rs:1210`, no tolerance), so
    this task reaches `done` only once the human accepts these rows or the gate gains an allowance.
  - **`RunReport::batches` is never empty.** A documented guarantee on a `pub` field the CLI
    reads, with no signature change; the spec's Scope says "Component-level API: none".

---

### Task 3: The three device pins become agreement cases, and the mixed merge gets one

**Files:**
- Modify: `peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs` (~l.181, 204, 495)

**Interfaces:** none.

These are device-tier tests. They are built here, not run, and their first run is #281's.

- [ ] **Step 1: Replace the three `bug_` cases**, each with its `// #205 —` comment, in place.
  `bug_one_zero_row_batch_sorts_to_nothing_on_the_cpu` becomes:

```rust
// Zero rows: the lane received a batch, so both answer one batch of zero rows.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn one_zero_row_batch_sorts_to_zero_rows_on_both() {
        run_both(&sorted(None), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}
```

  `bug_a_fetch_over_zero_rows_is_nothing_on_the_cpu` becomes:

```rust
// The same with a fetch, which neither side has rows to apply.
operator_case! {
    GpuAccumulateBatchesAndSort,
    fn a_fetch_over_zero_rows_is_zero_rows_on_both() {
        run_both(&sorted(Some(5)), Script::Accumulate(vec![synthetic(0, 1)])).same(Order::AsEmitted);
    }
}
```

  `bug_every_lane_a_zero_row_batch_is_nothing_on_the_cpu` becomes these two, the second new:

```rust
// Every lane a zero-row batch: one zero-row batch at the last `Done` on both.
operator_case! {
    GpuMergeSortedPartitions,
    fn every_lane_a_zero_row_batch_merges_to_zero_rows_on_both() {
        let lanes = vec![vec![synthetic(0, 1)], vec![synthetic(0, 2)]];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}

// One lane a zero-row batch and the other nothing: something arrived, so both owe the batch.
// The device holds one batch and merges it (`gpu_backend/accumulate.rs`, `held.is_empty()`).
operator_case! {
    GpuMergeSortedPartitions,
    fn one_lane_a_zero_row_batch_and_one_nothing_merges_to_zero_rows_on_both() {
        let lanes = vec![vec![synthetic(0, 1)], Vec::new()];
        run_both(&merged(2, None), Script::Lanes(lanes)).same(Order::AsEmitted);
    }
}
```

  `each_answers` stays imported: `bug_a_fetch_over_one_sorted_batch_is_not_applied_on_the_device`
  (#204) and the descending-key `bug_` case still call it.

- [ ] **Step 2: Build the device tier, no run.** `cpp/build` as limits left it; if it is absent,
  first `timeout 3600 scripts/build.sh --configure --build --cudf_ROOT
  ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12`.

```bash
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 5400 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run
grep -c 'bug_.*_on_the_cpu' peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs
```

  Expected: both builds succeed with no new warning (the device corpus reads `report.batches`,
  whose type did not change). The grep prints `0`. rustfmt `accumulate_cases.rs` alone.

- [ ] **Step 3: Commit.**

```bash
git add peacockdb-core/src/tests/gpu_tests/accumulate_cases.rs
git commit -F - <<'EOF'
#205: the three accumulate pins are agreement cases, and the mixed merge has one

One lane a zero-row batch and the other nothing answers one zero-row batch on both. Built
with --features gpu, not run (#281).

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

---

### Task 4: DuckDB

What duckdb-oracle owes once #205 is archived is stated in the spec, `empty-sorts.md` §"The work"
item 3, which is archived with this task into `archive/archived-tasks.md`, where the helper who
merges duckdb-oracle reads it. This plan and `empty-sorts-detail.md` are deleted at archival
(`prompts.md`, "After a merge, archive the task specs"), so neither is that record. Task 5 Step 5
also writes the list into #235, duckdb-oracle's own ticket.

The list, as of `origin/ENS-duckdb-oracle` (0df60133); find each with
`rg -n '205|q17' peacockdb-core/src/test_support peacockdb-core/tests/common llm-wiki/build-test.md llm-wiki/tickets/testinfra.md`:

1. `corpus_cases.inc:279`: q17's tpcds line `duckdb_divergent(205)` → `duckdb_exact`, and its
   comment stops citing #205. Its "stopped diverging" check is red as soon as q17 renders fifteen
   columns.
2. `duckdb_oracle.rs:113-115`: the row-level comment drops its q17 example and keeps the rule: "A
   ROW-LEVEL divergence — `duckdb_divergent(<ticket>)`, no positions — says the two answer
   different row sets, and the width is part of what differs. Under every other oracle a width
   difference is an engine that dropped a column."
3. `duckdb_oracle/tests.rs:157-158`, the doc of `a_zero_column_answer_reports_no_columns`: "tpcds
   q17 is the one such section" is false once q17 renders its header. It becomes "No corpus
   section renders one since #205; a reader that met one would still have to say how wide it
   is." The test body stays.
4. `duckdb_oracle/tests.rs:270-278`, `a_row_level_divergence_admits_a_column_count_difference`:
   the doc loses "tpcds q17's shape … (#205)" and states the shape; `divergent(205, vec![])` →
   `divergent(251, vec![])`. `compare` does not read the ticket; `compare_sections` does.
5. `duckdb_oracle/tests.rs:322-323`, `an_archived_ticket_is_not_open_and_a_listed_one_is`: delete
   `assert!(ticket_is_open(205), …)`, red once #205 is archived; the comment becomes "#251 is what
   `corpus_cases.inc`'s divergent lines name."
6. `build-test.md`'s DuckDB tier: `93 lines are duckdb_exact` → 94, `4 duckdb_divergent` → 3
   (recount from `corpus_cases.inc`). `tickets/testinfra.md`: the CI-skips-docs ticket's "asserts
   `ticket_is_open(205)` and `ticket_is_open(251)`" → `ticket_is_open(251)`, and its "#205 is on
   the archival path already…" sentence goes. #252's "four `duckdb_divergent` cases —
   `duckdb_tpcds_q17`, `q58`, `q61`, `q66`" → three, without q17, and its "Six cases are in the
   class, not four" → "Five …, not three".

- [ ] **Step 1: Which case.**

```bash
test -f peacockdb-core/src/test_support/duckdb_oracle.rs && echo merged || echo not-merged
```

- [ ] **Step 2a: `merged`.** Make the six edits above, then:

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- duckdb_tpcds_q17 --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- test_support::duckdb_oracle
```

  Expected: PASS; both sides are fifteen columns over zero rows. Commit:

```bash
git add peacockdb-core/tests/common/corpus_cases.inc peacockdb-core/src/test_support llm-wiki/build-test.md llm-wiki/tickets/testinfra.md
git commit -F - <<'EOF'
#205: q17 meets DuckDB exactly, header and all; its divergent line goes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```

- [ ] **Step 2b: `not-merged`.** No code touches DuckDB, as the spec says. Nothing to commit here;
  Task 5 Step 5 writes the list into #235.

---

### Task 5: The wiki, #205 archived, and the full bar

**Files:**
- Modify: `llm-wiki/architecture.md` (~l.647-650, ~l.779-781), `llm-wiki/build-test.md`,
  `llm-wiki/tickets.md`, `llm-wiki/tickets/corpus-coverage.md`, `llm-wiki/archive/archived-tickets.md`,
  `llm-wiki/tasks/empty-sorts-detail.md`

- [ ] **Step 1: `architecture.md`.**
  - "Zero-row batches change no answer", the last bullet becomes: "- A producer that drops a
    zero-row batch exposes the breaks above: the limit
    ([#214](tickets/corpus-coverage.md#t214))."
  - The schedule section's "A Python model of these same rules — the scheduler, both drivers, the
    accountant, operators over pandas — is `scripts/exec_model/` …" gains, after its sentence:
    "Its driver answers nothing where the sink received nothing (`partitioned_driver.py`,
    `results`); the engine's answers one zero-row batch under the sink's input columns
    (`Driver::answer`)."

- [ ] **Step 2: `build-test.md`.** Recount from the code as limits left it, then apply these
  deltas:

| row | delta | description change |
|---|--:|---|
| CPU backend executors | +8 | "…and an accumulating sort or merge over zero-row batches answering one" |
| Drivers over a mock backend | +3 | "…a sink that received nothing answering one zero-row batch under its input's columns" |
| End to end | +2 | its count of cases no query list can carry +2: "a query answering zero rows answering them under its column at every mode, and the empty sort's and merge's own emissions" |
| cpu block `--lib`, cpu block | +13 each | — |
| Operator harness (gpu) | +1 | — |
| gpu block `--lib -- gpu_tests::`, gpu block | +1 each | — |
| Rust, grand total | +14 each | — |

  The device corpus row's "the device's own tickets (#57, #63, #205)" names #281 in place of
  #205, unless limits already added #281 there.

- [ ] **Step 3: #281 names q17 at every device mode.** In `corpus-coverage.md`, #281's
  "**Corpus queries:** `tpch/scan-limit` and `tpch/nested-limits` at every device mode, and
  `tpcds/q17` at `tp1-single`" → "`tpch/scan-limit`, `tpch/nested-limits` and `tpcds/q17` at every
  device mode": q17's registry row names #281 alone (Task 2 Step 10), for all five device cells.

- [ ] **Step 4: Archive #205.** Move its block, anchor included, from `corpus-coverage.md` to the
  top of `archive/archived-tickets.md`'s `## Done`, and append:

  > **Done.** The accumulating sort and merge answer held batches with no row as one zero-row
  > batch, unsorted (`sorted_and_cut`, `cpu_backend/accumulate.rs`). A query whose sink received
  > nothing answers one zero-row batch under the sink's input schema, made by the driver
  > (`Driver::answer`, `driver/partitioned.rs`). The corpus and end-to-end oracles give
  > DataFusion's empty answer its columns the same way. q17's result section carries its header.
  > The three `bug_` pins are agreement cases, and the mixed merge has one, built and not run
  > (#281).

  Remove #205 from `corpus-coverage.md`'s contents list (l.24) and from `tickets.md`'s
  corpus-coverage row. Decrement that row's count and the open total.

- [ ] **Step 5: Sentences #205's fix falsified.**
  - `corpus-coverage.md`, #199's fix: "Sources of nothing below the init (#214's limit, #205's
    sort, the joins)" → "(#214's limit, the joins)".
  - `corpus-coverage.md`, #235's expected divergences, only if #235 is still open (Task 4 took
    `not-merged`): the "**An empty answer.**" bullet becomes:

    > - **An empty answer** — none since 2026-10-08. empty-sorts (#205) answers every empty
    >   result under its declared columns, so `tpcds/q17` renders its header. duckdb-oracle owes,
    >   on rebasing over it (spec `empty-sorts.md` §"The work" 3, in
    >   `archive/archived-tasks.md`): q17's `duckdb_divergent(205)` → `duckdb_exact`
    >   (`corpus_cases.inc:279`); the q17 example in `duckdb_oracle.rs:113-115`; the doc of
    >   `a_zero_column_answer_reports_no_columns` (`duckdb_oracle/tests.rs:157-158`); `divergent(205,
    >   …)` and its doc in `a_row_level_divergence_admits_a_column_count_difference` (`:270-278`);
    >   `assert!(ticket_is_open(205))` (`:322-323`); the DuckDB tier's counts in `build-test.md`;
    >   #205 in `testinfra.md`'s CI-skips-docs ticket and in #252. Lines as of 0df60133.

  - "as #205 says" in #214 (`corpus-coverage.md`) and the cross-join ticket (`joins.md`) stay as
    they are. The number resolves in the archive.

- [ ] **Step 6: The full bar.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 7200 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_golden_format --test test_module_layout
git diff --stat "$(git merge-base HEAD ENS-limits)"..HEAD -- testdata/
```

  Expected: all green; the cpu registry test passes in both directions; the `testdata` diff is
  `mini.result.txt`, the four tp1 goldens and the registry row, plus any section Task 1 Step 1
  added. The device tier was built in Task 3 and is not run (#281).

- [ ] **Step 7: The cost gate over the whole branch.** Step 13 of Task 2's command again:

```bash
base=$(git merge-base HEAD ENS-limits)
timeout 900 cargo run -q -p cost-report -- --cost-diff --base "$base" --html /tmp/empty-sorts/cost_diff.html --md-diff /tmp/empty-sorts/cost_diff.md; echo "rc=$?"
```

  Expected: the same 🔴 rows Task 2 Step 13 recorded and no other. A new row goes into
  `empty-sorts-detail.md` beside them, with the commit that moved it.

- [ ] **Step 8: Commit.**

```bash
git add llm-wiki
git commit -F - <<'EOF'
#205 archived: the break struck from architecture.md; counts; #281 holds q17's modes

The Python model's empty answer noted; #199's sources of nothing; #235 told what
duckdb-oracle owes where it has not merged. The detail file lists the cost-gate rows.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
```
