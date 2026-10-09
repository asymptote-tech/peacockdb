# limits implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A limit DataFusion pushed into a scan stops being the readers' job. It becomes a
`GpuLimit` directly above a one-lane scan, or the unload's interval where nothing sits between
the scan and the root. The scan maps only the shortest prefix of row groups whose rows reach the
cut, so a small limit reads one row group at every mode. `GpuLoadParquet.limit`,
`CudfScan.limit` and `set_num_rows` go (#186). The driver judges a mid-plan limit by the rows
it emitted, never by a second count of its input (#234). scan-limit's two tp1 cpu cells turn on,
and `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)` answers 3 at every mode, not 25.

**Architecture:** `source()` (`planner/translator/nodes.rs`) builds the loader without a limit
and puts `GpuLimit { skip: 0, fetch: Some(n) }` over it. The loader's survivors are first cut to
`covering_prefix(groups, n)`, the shortest prefix whose parquet-metadata row counts reach `n`,
before `lanes_for` and `partition` see them. `Translator::translate` reaches the root's input
through a new `unload_input`. It hands the pushed cut of a scan with nothing above it back to the
unload, composed with any root limit by `RowInterval::over`. The reason is the validator: it
refuses a `GpuLimit` feeding only the sink (`plan/validate.rs:40-49`), and the spec puts that cut
in the unload's interval. In the driver (`executor/driver/partitioned.rs`), `settle_limit` reads
a new `rows_emitted` for a mid-plan limit and keeps `rows_seen` for the unload alone.

**Tech stack:** Rust over DataFusion 45 physical plans, cpu tier (`--features rust-only`);
FlatBuffers schema; C++ against cuDF 25.02, built and `ctest -L cpu` only; the gpu-feature Rust
lib built, never run.

**Spec:** [`limits.md`](limits.md). Tickets: [#186](../archive/archived-tickets.md#t186),
[#234](../archive/archived-tickets.md#t234), [#281](../tickets/corpus-coverage.md#t281).

## Global constraints

- **No GPU** (chain K's board header, `tasks.md`). No device run and no GPU cycle. The board's
  "no device build" means nothing is built or run on a GPU host. The local cuDF 25.02 builds in
  Task 3 Step 7 are allowed: the C++ build, and the cudf-feature cargo `--no-run` builds. They are
  the "C++ build against cuDF 25.02" the header lists, and CI's `cpp-build-2502` compiles the same
  thing. The task is `done` when every CI job but the GPU tests is green. `scan.cpp`, the four
  device source cases and every other `gpu_tests` edit are **built locally and never run**; #281
  holds them.
- **Base: chain K's task 2's branch, `ENS-distinct-companions`.** limits is chain K's task 3. It
  comes after guard-checks and distinct-companions, on a chain whose base is master. Line numbers
  below are from master at 0753f8c9, whose code is 64ced62e's. The two earlier tasks move some of
  them:
  - guard-checks: `driver/tests/mock.rs`, `plan/validate.rs`, the `build-test.md` counts.
  - distinct-companions: `tests/end_to_end.rs`, which gains `sql_answers_match_oracle`;
    `gpu_plan.fbs`; `corpus_cases.inc`; `cost-registry.csv`; the counts.

  Find each place by the quoted text.
- **The CI cost-regression gate is a CPU job and is waited on.** It is `cost-report
  --cost-diff`, in `pipeline.yml`'s cost-report job (~l.886-939). On every PR it diffs each
  `.cost.txt` section's `peacockdb_cost=` against the PR's base SHA. Any increase fails the job:
  an exact integer compare with no tolerance (`cost-report/src/main.rs`, `DiffRow::is_regression`).
  - This task moves cost both ways. The covering prefix lowers scan-limit's cost at tp4-single
    and tp4-sized, and nested-limits' at tp1-single, tp4-single and tp4-sized: each now reads one
    row group where it read the file.
  - The `GpuLimit` over the part scan adds its 28 rows of output (228 bytes). At tp1-rowgroup and
    tp4-rowgroup nothing offsets that, since part was already read one row group at a time, so
    nested-limits' cost rises there.
  - Tasks 2, 4 and 5 run the gate locally (`cargo run -q -p cost-report -- --cost-diff --base
    "$BASE" …`) and check its rows against the expected list.
  - Task 5 records every rising section in `limits-detail.md` for the human. Until the human
    accepts those exact rows, the PR's cost-report job is red, and the task cannot reach `done`.
    Say so in the final message.
- **No backend change.** Both `LimitStream`s keep their own count and every drop, forward and
  slice decision. `CpuSource` is untouched, and so is the unload's handling. The covering prefix
  is the planner's: a reader reads whole row groups, as now.
- **Wire:** `CudfScan.limit` marked `(deprecated)`, no slot moved, as distinct-companions did
  for `AggregateFuncNode.distinct`. `recipe-payloads.txt` must not move: no payload query has a
  limited scan, and a `0` at its default is not written.
- **Goldens:** only the `scan-limit` and `nested-limits` sections move. That covers the five
  tpch `.plans.txt`, `-mini.cpu.txt` and `-mini.cost.txt`. No tpcds golden moves, and no
  `mini.result.txt` or `recipe-payloads.txt`.
- Builds as `build-test.md` documents them:
  - rust-only in `./target`;
  - C++ in `cpp/build` (`scripts/build.sh --configure --build --cudf_ROOT
    ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12`; drop `--configure` once
    configured);
  - cudf-feature cargo only through `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2
    scripts/cargo-cudf.sh`.

  Build in a workspace; never share a cargo target dir across worktrees. Wrap every build and
  test in `timeout` (the developer's rule); the bounds below are generous.
- rustfmt only the leaf files touched (`rustfmt --edition 2024 <file>…`), never a `mod.rs`.
  rustfmt follows `mod` declarations, so restore any file it reformatted that the task did not
  touch (`git checkout -- <path>`).
- `partitioned.rs` is 947 lines at master and must stay under 1000. Chain J's join-backend and
  chain L's keyless-identity edit the same file, and `driver/tests/mock.rs`, after this task. The
  conflicts are textual, and the later chain resolves them.
- Commit messages at most 10 lines, ending `Co-Authored-By: Claude Opus 5.5
  <noreply@anthropic.com>`.
- **Ticket archival.** The helper archives #186 and #234 at merge. guard-checks and
  distinct-companions do the same, and it is the house convention, though the spec's scope row
  says `tickets/`.

## Review focus

1. **A root limit with an OFFSET over a scan DataFusion pushed `skip + fetch` into.** `LIMIT 3
   OFFSET 2` reaches the translator as a root `GlobalLimitExec(2, 3)` over a scan carrying
   `limit=5`. The fold must give the unload exactly `2..+3`: not `2..+5`, not `4..+3`.
   - The scan's covering prefix is for `5`, the rows the cut can need.
   - Task 2 checks it with the OFFSET query at every mode, and with `RowInterval::over`'s table:
     an outer skip past the inner cut, a pure offset over a cut, a cut over a pure offset.
2. **The covering prefix's boundary.** `n` equal to the first group's rows maps that group
   alone, one row more maps two, and a scan with no limit maps every survivor.
   - `n = 0` keeps one group, so `partition` is never handed nothing.
   - The prefix is exact only because the readers return every row of a group they read. Nothing
     filters inside the scan (`pushdown_filters` is off), and DataFusion pushes a limit into a
     scan only with nothing filtering between.
   - `can_be_null` stays computed over every survivor. A NULL in a group the prefix drops only
     makes it conservative.
   - Task 2's prefix case covers this at every mode.
3. **A mid-plan limit with a skip.** The driver must compare the rows the limit emitted with
   `fetch`, never with `skip + fetch`. The slip is reusing `satisfied_by` on the new count. The
   mock forwards whole batches, so `limit(25, 5)` emits ten rows from the first batch: satisfied
   after one pull, where the old rule reads three. Task 1.
4. **`fetch = 0` with a skip, and no `fetch` at all, mid-plan.** `LIMIT 0 OFFSET 5` is satisfied
   before any pull. Under the old rule it pulled one batch, but only because of the skip:
   `satisfied_by(0)` already holds at the seed when the skip is 0 (`plan/mod.rs:1047-1049`), so
   the spec's "today it pulls one batch first" is true only with a skip. A pure offset is never
   satisfied, so a `fetch.unwrap_or(0)` slip would stop the scan at the seed. Task 1, two cases.
5. **A limit over a zero-column scan.** nested-limits' region scan projects nothing
   (`projections=[]`) and now carries `GpuLimit 0..+23` with five rows under it. Rows with no
   columns pass a limit that is never satisfied. The nested-limits corpus cells and the end-to-end
   limit test (Task 2) pin it at all five modes; its device half is #281.
6. **Where a limit's stop is still proved.** With the prefix, every pushed limit's mapping offers
   only the batches it needs, so nested-limits no longer has a mode where stopping saves a read.
   - Its end-to-end case now asserts that the mapping offers exactly the two batches pulled.
   - The hold that stops a scan is proved by the driver's mock cases (Task 1, and
     `a_mid_plan_limit_stops_its_own_subtree_and_holds_nothing`).
   - At the corpus scale it is proved by the mid-plan limits that are not a scan's, in tpcds
     q16, q32, q92, q94, q95 and q97.

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/mod.rs`, `plan/interval.rs` | `RowInterval::satisfied_by_emitted`, `RowInterval::over`; `GpuLoadParquet.limit` gone |
| `peacockdb-core/src/executor/driver/partitioned.rs` | `rows_emitted`; `settle_limit` per carrier; module doc |
| `peacockdb-core/src/executor/driver/tests/mock.rs`, `tests/limit.rs` | `with_limit`, `AccRule::Trimming`; four cases |
| `peacockdb-core/src/planner/translator/nodes.rs`, `translator/mod.rs` | `source`/`loader`/`covering_prefix`/`pushed_limit`/`unload_input`; `translate` folds |
| `peacockdb-core/src/planner/translator/tests.rs` | four cases replace one |
| `peacockdb-core/src/tests/end_to_end/limits.rs` | scan-limit at every mode; the limited subquery; every limit in nested-limits |
| `peacockdb-core/src/plan/source.rs`, `plan_text/node_text.rs`, `wire/node_writer.rs`, `wire/fb_text.rs`, `wire/generated.rs` (comment) | the loader's limit gone |
| every other `GpuLoadParquet::new` caller (13, listed in Task 3) | one argument fewer |
| `flatbuffers/gpu_plan.fbs`, `cpp/src/operators/scan.cpp` | `CudfScan.limit` deprecated; `set_num_rows` gone |
| `peacockdb-core/src/tests/gpu_tests/source_cases.rs`, `source_schema_cases.rs` | the four `bug_` cases become agreement cases over a real scan's output |
| `testdata/goldens/tpch.sf1/*`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | scan-limit, nested-limits |
| `testdata/tpch-queries/nested-limits.sql` (comment only) | "the scan reads 28 rows in all" was never true |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/corpus-coverage.md` (#281), `tickets/memory.md` (#229), `tasks/limits-detail.md` (the cost gate's rows) | docs, counts |

---

### Task 1: The driver judges a mid-plan limit by the rows it emitted (#234)

Independent of the plan change, and moves no golden: on today's plans a mid-plan limit is
satisfied at the same step under either count.

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs` (`impl RowInterval`, ~l.1040-1058)
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs` (module doc l.1-9, fields
  ~l.69-73, `new` ~l.173, `run_lane_scoped` ~l.284-360, `settle_limit` ~l.613-625)
- Modify: `peacockdb-core/src/executor/driver/tests/mock.rs` (`AccRule` ~l.65, `Script`
  ~l.111-165, `MockAcc` ~l.408-445, `executors_for` ~l.634-650)
- Test: `peacockdb-core/src/executor/driver/tests/limit.rs`

**Interfaces:**
- Produces: `RowInterval::satisfied_by_emitted(&self, emitted: u64) -> bool`;
  `Script::with_limit(self, AccRule) -> Script`; `AccRule::Trimming(usize)`.

- [x] **Step 1: Give the mock's limit a rule of its own.** In `mock.rs`, add a variant on
  `AccRule` after `Streaming`:

```rust
    /// Emits each batch cut to at most this many rows and holds nothing — a limit letting
    /// through less than it was handed.
    Trimming(usize),
```

  Add a field on `Script` after `accumulate`, defaulting to `AccRule::Streaming` in `Default`:

```rust
    /// What a `GpuLimit`'s executor does with each batch. Apart from `accumulate`, because a
    /// limit streams whatever the other accumulators do, and the rows it lets through are
    /// what the driver judges it by.
    pub limit: AccRule,
```

  Add a builder after `with_accumulator`. That builder's doc's last sentence becomes "A
  `GpuLimit` follows `with_limit` instead.":

```rust
    pub(crate) fn with_limit(mut self, limit: AccRule) -> Self {
        self.limit = limit;
        self
    }
```

  In `MockAcc::accumulate_and_fetch`, add an arm before `_`:

```rust
            AccRule::Trimming(most) => (
                vec![MockBatch {
                    rows: batch.rows.min(most),
                    bytes: batch.bytes,
                }],
                stats,
            ),
```

  The other edits:
  - In `mark_done_and_fetch`, the first arm becomes `AccRule::Streaming |
    AccRule::Trimming(_) => Vec::new(),`.
  - In `executors_for`, `script.accumulate = AccRule::Streaming;` becomes `script.accumulate =
    script.limit;`. Its comment's first sentence becomes "a limit follows the script's `limit`
    rule whatever the other accumulators do".
  - guard-checks edits `MockUnload` in this file, not these places. Find each by its text.

- [x] **Step 2: Write the failing tests** in `limit.rs`, after
  `a_satisfied_limit_reports_done_so_the_node_above_it_can_finish`. The file's import line
  becomes `use super::mock::{AccRule, Script, spec};`.

```rust
/// A mid-plan limit over the six ten-row batches, its executor following `rule`.
fn mid_plan(skip: u64, fetch: Option<u64>, rule: AccRule) -> RunReport {
    let plan = unload(filter(limit(merge(source("part", 1)), skip, fetch)));
    run(plan.as_ref(), &six_batches().with_limit(rule))
}

#[test]
fn a_mid_plan_limit_is_satisfied_by_fetch_rows_emitted_whatever_its_skip() {
    // The mock forwards each batch whole, so ten rows leave the limit with the first one.
    // Judged by what it emitted, a limit wanting five is done after one pull; judged by its
    // input against skip + fetch it would read three batches.
    let report = mid_plan(25, Some(5), AccRule::Streaming);
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        1,
        "the limit's own output reached its fetch with the first batch"
    );
    assert_accounted(&report);
}

#[test]
fn a_limit_emitting_fewer_rows_than_it_read_keeps_the_driver_pulling() {
    // Four rows out of every ten: twelve have left the limit after the third batch and not
    // before, where its input reached twelve after the second.
    let report = mid_plan(0, Some(12), AccRule::Trimming(4));
    assert!(!report.satisfied.is_empty());
    assert_eq!(
        count(&report, CallKind::NextBatch),
        3,
        "the driver stopped on rows the limit never let through"
    );
    assert_accounted(&report);
}

#[test]
fn a_mid_plan_limit_with_no_fetch_is_never_satisfied() {
    let report = mid_plan(5, None, AccRule::Streaming);
    assert!(
        report.satisfied.is_empty(),
        "no count of rows emitted determines a pure offset"
    );
    assert_eq!(count(&report, CallKind::NextBatch), 6, "every batch was read");
    assert_accounted(&report);
}

#[test]
fn a_mid_plan_limit_of_no_rows_is_satisfied_before_any_pull_whatever_its_skip() {
    // With a skip of 0 the old rule held at the seed too; the skip is what made it pull.
    let report = mid_plan(5, Some(0), AccRule::Streaming);
    assert!(!report.satisfied.is_empty(), "satisfied at the seed");
    assert_eq!(count(&report, CallKind::NextBatch), 0);
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(report.in_flight_bytes, 0);
}
```

  `RunReport`, `count`, `run`, `rows_returned` and `assert_accounted` come in through `use
  super::*;`, as the file's other cases use them. In
  `a_mid_plan_limit_stops_its_own_subtree_and_holds_nothing`:
  - "the driver counts the rows going past it" becomes "the driver reads the rows it emitted";
  - its assert message "once twelve rows had gone past the limit" becomes "once twelve rows had
    left the limit".

- [x] **Step 3: Run the tests and confirm three fail.**

```bash
timeout 1200 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver::tests::limit
```

  Expected:
  - `…whatever_its_skip` FAILS (3 pulls, expected 1).
  - `…keeps_the_driver_pulling` FAILS (2, expected 3).
  - `…of_no_rows…` FAILS (1, expected 0).
  - `…with_no_fetch…` PASSES. It is the guard against a `None` read as `0`.
  - Every older case PASSES.

- [x] **Step 4: Add the predicate.** In `plan/mod.rs`, `satisfied_by`'s doc becomes "True once
  `seen` rows of this node's input leave no later row able to change the answer — what an
  unload is judged by." After it, add:

```rust
    /// True once a limit has emitted `fetch` rows, whatever its skip: what it let through is
    /// counted, never what it was handed. `None` is a pure offset and never satisfies.
    pub(crate) fn satisfied_by_emitted(&self, emitted: u64) -> bool {
        self.fetch.is_some_and(|fetch| emitted >= fetch)
    }
```

- [x] **Step 5: Change the driver.** In `partitioned.rs`, the module doc's last clause, "along
  with the one node the driver special-cases, a `GpuUnload` carrying a limit.", becomes:

```rust
//! to [`super::single_partition`]; the three cross-lane categories are here, along with
//! the hold on a satisfied limit — an unload's interval, whose count only the driver can
//! keep, and a mid-plan `GpuLimit`, judged by the rows it emitted.
```

  Then the field doc, and a second field:

```rust
    /// Per unload carrying an interval, rows of its input stream seen so far, summed over
    /// every lane; zero for every other node. Only the driver can hold this: an unload
    /// instance is one lane's and the count is not.
    rows_seen: Vec<u64>,
    /// Per mid-plan limit, rows it has emitted. Its executor keeps the only count of its
    /// one-lane input and makes every cut; this is what those cuts let through, read off
    /// its outputs rather than computed a second time (#234).
    rows_emitted: Vec<u64>,
```

  Add `rows_emitted: vec![0; nodes],` beside `rows_seen` in `new`. In `run_lane_scoped`, only
  the unload peeks its input:

```rust
            let arriving = match (interval, unloading && consuming) {
                (Some(_), true) => self.peek_rows(node, lane, call),
                _ => 0,
            };
            // Only an unload's decision is made here, its range being an argument of the
            // driver's own call. A mid-plan limit makes the same three-way choice inside its
            // executor, and the driver reads what it emitted.
```

  In the `LaneOutputs::Device(batches)` loop, add this right after `self.record_emitted(node,
  lane, &batch);`:

```rust
                        // Device outputs carrying an interval are a mid-plan limit's: the
                        // unload's are host batches.
                        if interval.is_some() {
                            self.rows_emitted[node] += batch.rows();
                        }
```

  `settle_limit` becomes:

```rust
    /// Enough rows have passed this node that no later one can change its answer: an
    /// unload's input has reached `skip + fetch`, or a mid-plan limit has emitted `fetch`.
    /// It is marked done as it is held, or the hold would stop it reporting and strand its
    /// parent — `LIMIT 0` is the case that forces it.
    fn settle_limit(&mut self, node: usize) {
        let indexed = &self.index.nodes[node];
        let Some(interval) = indexed.interval else {
            return;
        };
        let satisfied = match indexed.category {
            ExecutorCategory::Unload => interval.satisfied_by(self.rows_seen[node]),
            // The other interval carrier is a mid-plan limit.
            _ => interval.satisfied_by_emitted(self.rows_emitted[node]),
        };
        if !satisfied {
            return;
        }
        self.scheduler.satisfy(node);
        self.states[node].out_done = vec![true; indexed.lanes];
    }
```

  If the borrow of `indexed` across `self.scheduler.satisfy` does not compile, copy
  `indexed.lanes` into a local first. Check the length with `wc -l
  peacockdb-core/src/executor/driver/partitioned.rs`: it must stay under 1000.

- [x] **Step 6: Run the driver tests and every corpus query with a mid-plan limit.** The tpcds
  six carry `GpuLimit`s at `skip=0, fetch=100`. Under both rules such a limit is satisfied at
  the same step. Running them checks that claim instead of reasoning it.

```bash
timeout 1200 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver tests::end_to_end::limits
timeout 3600 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_nested_limits cpu_tpch_scan_limit cpu_tpcds_q16_ cpu_tpcds_q32_ cpu_tpcds_q92_ cpu_tpcds_q94_ cpu_tpcds_q95_ cpu_tpcds_q97_
git status --short testdata
```

  Expected: all PASS, with no golden written. nested-limits' `GpuLimit 5..+23` is handed one
  batch, 200,000 rows (122,880 at the rowgroup modes), and emits 23 in the same step. So
  `early_exit=` and every count stand. `git status` prints nothing.

- [ ] **Step 7: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/executor/driver/partitioned.rs peacockdb-core/src/executor/driver/tests/mock.rs peacockdb-core/src/executor/driver/tests/limit.rs
git add peacockdb-core/src/plan/mod.rs peacockdb-core/src/executor/driver
git commit -m "#234: the driver judges a mid-plan limit by the rows it emitted" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: A scan's limit is a `GpuLimit` above it, or the unload's interval, over the row groups it needs (#186, planner)

**Files:**
- Modify: `peacockdb-core/src/plan/interval.rs`, `peacockdb-core/src/plan/mod.rs` (`impl RowInterval`)
- Modify: `peacockdb-core/src/planner/translator/nodes.rs` (`node`'s parquet arm l.53-55,
  `lanes_for` l.374-390, `source` l.392-412)
- Modify: `peacockdb-core/src/planner/translator/mod.rs` (`use nodes::node`, `translate` l.82-96)
- Test: `peacockdb-core/src/planner/translator/tests.rs`,
  `peacockdb-core/src/tests/end_to_end/limits.rs`
- Regenerate: `testdata/goldens/tpch.sf1/*.plans.txt`, `*-mini.cpu.txt`, `*-mini.cost.txt`
  (the `scan-limit` and `nested-limits` sections)

**Interfaces:**
- Produces: `RowInterval::over(&self, inner: &RowInterval) -> RowInterval`: this interval
  applied to what `inner` emits, as one interval over `inner`'s input.
- Produces: `nodes::unload_input(t, plan) -> Result<(Box<dyn GpuNode>, Option<RowInterval>),
  PlanError>`.
- Produces: a limited scan's `GpuLoadParquet.survivors` and `partition_groups` cover only the
  prefix of row groups that reaches its limit.
- After this task no planned loader carries a limit. It is built with `None` until Task 3
  deletes the field, so plan text already prints no `limit=`.

- [x] **Step 1: Write the translator tests.** Replace `a_scan_carrying_a_pushed_down_limit_plans_one_lane`
  (l.602-614) with:

```rust
/// `sql` planned at every mode over the minimal dataset, through the planner the corpus
/// uses — the estimator's second pass and the validator included, so a tree the validator
/// refuses fails here.
async fn planned_at_every_mode(sql: &str) -> Vec<(String, Box<dyn GpuNode>)> {
    let mut planned = Vec::new();
    for mode in &crate::test_support::MODES {
        let plan = plan_at(sql, mode.target_partitions).await;
        let (tree, _memory) = crate::planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("{sql} at {}: {error}", mode.name));
        planned.push((mode.name.to_string(), tree));
    }
    planned
}

fn is_limit_over_a_loader(node: &dyn GpuNode) -> bool {
    matches!(as_node_ref(node), NodeRef::Limit(_))
        && node
            .children()
            .first()
            .is_some_and(|child| matches!(as_node_ref(*child), NodeRef::LoadParquet(_)))
}

/// The interval of every limit directly over a loader, parents before children.
fn limits_over_loaders(node: &dyn GpuNode, found: &mut Vec<RowInterval>) {
    if is_limit_over_a_loader(node) {
        found.extend(node.row_interval());
    }
    for child in node.children() {
        limits_over_loaders(child, found);
    }
}

/// The row groups the tree's first loader maps, in mapping order.
fn mapped_groups(tree: &dyn GpuNode) -> Vec<u32> {
    let loader = find(tree, &|node| matches!(as_node_ref(node), NodeRef::LoadParquet(_)))
        .expect("a loader");
    let NodeRef::LoadParquet(load) = as_node_ref(loader) else {
        unreachable!("find accepted only a loader");
    };
    let survivors: Vec<u32> = load.survivors.iter().map(|group| group.index).collect();
    let mapped: Vec<u32> = load.partition_groups.iter().flatten().flatten().copied().collect();
    assert_eq!(mapped, survivors, "the mapping addresses exactly the survivors");
    mapped
}

#[tokio::test]
async fn a_scan_limit_with_nothing_above_it_is_the_unloads_interval_at_every_mode() {
    // DataFusion pushes the bound into the scan and, over one partition, erases the limit
    // node above it. A GpuLimit here would feed only the sink, which the validator refuses:
    // a root-adjacent interval is the unload's. The scan is one lane, since rows from
    // several have no order a cut could follow.
    for (mode, tree) in planned_at_every_mode("SELECT n_nationkey FROM nation LIMIT 3").await {
        assert_eq!(shape(tree.as_ref()), "Unload(LoadParquet)", "at {mode}");
        assert_eq!(
            tree.row_interval(),
            Some(RowInterval { skip: 0, fetch: Some(3) }),
            "at {mode}"
        );
        assert_eq!(descend(tree.as_ref(), 1).kind().layout().unwrap().n, 1, "at {mode}");
    }
    // With an offset DataFusion pushes skip + fetch into the scan and keeps a limit above
    // for the skip; the two fold into the one cut the query meant.
    for (mode, tree) in
        planned_at_every_mode("SELECT n_nationkey FROM nation LIMIT 3 OFFSET 2").await
    {
        assert_eq!(shape(tree.as_ref()), "Unload(LoadParquet)", "at {mode}");
        assert_eq!(
            tree.row_interval(),
            Some(RowInterval { skip: 2, fetch: Some(3) }),
            "at {mode}"
        );
    }
}

#[tokio::test]
async fn a_limited_scan_under_a_subquery_or_a_join_is_a_limit_over_a_one_lane_scan_at_every_mode() {
    let cut = |fetch| RowInterval { skip: 0, fetch: Some(fetch) };
    // Under an aggregate: DataFusion's pushdown erases the limit node and leaves the cut in
    // the scan alone, so until #186 no reader applied it and the count was of all 25 rows.
    let subquery = "SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)";
    for (mode, tree) in planned_at_every_mode(subquery).await {
        assert!(
            shape(tree.as_ref()).contains("Aggregate(Limit(LoadParquet))"),
            "at {mode}: {}",
            shape(tree.as_ref())
        );
        let mut found = Vec::new();
        limits_over_loaders(tree.as_ref(), &mut found);
        assert_eq!(found, vec![cut(3)], "at {mode}");
        let limit = find(tree.as_ref(), &is_limit_over_a_loader).expect("found above");
        assert_eq!(limit.children()[0].kind().layout().unwrap().n, 1, "at {mode}");
    }
    // Under a cross join: nested-limits, whose two scans each carry a pushed cut.
    let sql = std::fs::read_to_string(
        crate::test_support::queries_dir_for("tpch").join("nested-limits.sql"),
    )
    .expect("the corpus query");
    for (mode, tree) in planned_at_every_mode(&sql).await {
        let mut found = Vec::new();
        limits_over_loaders(tree.as_ref(), &mut found);
        assert_eq!(found, vec![cut(23), cut(28)], "at {mode}: {}", shape(tree.as_ref()));
    }
    // A scan DataFusion pushed nothing into gains no node.
    for (mode, tree) in planned_at_every_mode("SELECT n_nationkey + 1 AS k FROM nation").await {
        assert!(
            find(tree.as_ref(), &|node| matches!(as_node_ref(node), NodeRef::Limit(_))).is_none(),
            "at {mode}: {}",
            shape(tree.as_ref())
        );
    }
}

#[tokio::test]
async fn a_limited_scan_maps_only_the_row_groups_that_reach_its_limit_at_every_mode() {
    // The minimal part is two row groups, 122,880 rows and 77,120. At the single-batch
    // modes a scan of every survivor would be one batch of the whole file whatever the cut.
    let cases = [
        ("SELECT p_partkey FROM part LIMIT 10", vec![0]),
        ("SELECT p_partkey FROM part LIMIT 122880", vec![0]),
        ("SELECT p_partkey FROM part LIMIT 122881", vec![0, 1]),
        ("SELECT p_partkey FROM part", vec![0, 1]),
    ];
    for (sql, expected) in cases {
        for (mode, tree) in planned_at_every_mode(sql).await {
            assert_eq!(mapped_groups(tree.as_ref()), expected, "{sql} at {mode}");
        }
    }
}

#[test]
fn an_interval_over_another_keeps_the_rows_both_keep() {
    let interval = |skip, fetch| RowInterval { skip, fetch };
    // (outer, inner, the one interval over inner's input)
    let cases = [
        // scan-limit at tp4: the root limit and the scan's are the same cut
        (interval(0, Some(10)), interval(0, Some(10)), interval(0, Some(10))),
        // LIMIT 3 OFFSET 2 over the scan's 0..+5
        (interval(2, Some(3)), interval(0, Some(5)), interval(2, Some(3))),
        // the inner cut is the tighter one
        (interval(0, Some(10)), interval(0, Some(4)), interval(0, Some(4))),
        // the outer skip passes the whole inner cut: nothing
        (interval(5, Some(10)), interval(0, Some(3)), interval(5, Some(0))),
        // a pure offset over a cut, and a cut over a pure offset
        (interval(3, None), interval(0, Some(8)), interval(3, Some(5))),
        (interval(0, Some(4)), interval(2, None), interval(2, Some(4))),
        (interval(1, None), interval(2, None), interval(3, None)),
    ];
    for (outer, inner, expected) in cases {
        assert_eq!(outer.over(&inner), expected, "{outer:?} over {inner:?}");
    }
}
```

  `RowInterval`'s fields are `pub(crate)`, so the struct literals compile here; if rustfmt
  breaks them over lines, let it. `queries_dir_for` is `pub` in `test_support`.

- [x] **Step 2: Write the end-to-end cases** in `tests/end_to_end/limits.rs`. Add two new tests
  after the existing one:

```rust
/// A limit DataFusion pushed into the scan answers its count at every mode, from one row
/// group: the scan maps only the prefix its cut needs, so the single-batch modes no longer
/// read all forty-nine. At the tp1 modes the cpu reader ignored the scan's limit and
/// answered all 6,001,215 rows (#186).
#[tokio::test]
async fn a_scan_limit_answers_its_count_from_one_read_at_every_mode() {
    use crate::executor::CallKind;
    use crate::plan::{NodeRef, as_node_ref};

    let data_dir = data_dir_for("tpch", "1");
    for mode in &MODES {
        let name = mode.name;
        let ctx = crate::register_tables_for(
            crate::build_session_state(mode.target_partitions),
            &data_dir,
        )
        .await
        .expect("register the tables");
        let plan = ctx
            .sql("SELECT * FROM lineitem LIMIT 10")
            .await
            .expect("the query plans")
            .create_physical_plan()
            .await
            .expect("the query has a physical plan");
        let (tree, _memory) = planner::plan(&plan, mode.knobs())
            .unwrap_or_else(|error| panic!("scan-limit at {name}: {error}"));
        let NodeRef::LoadParquet(load) = as_node_ref(tree.children()[0]) else {
            panic!("scan-limit at {name}: the unload reads a loader");
        };
        assert_eq!(
            load.partition_groups,
            vec![vec![vec![0]]],
            "scan-limit at {name}: ten rows need lineitem's first row group alone"
        );
        let report = run::<CpuBackend>(tree.as_ref(), &ctx.task_ctx(), None)
            .unwrap_or_else(|error| panic!("scan-limit at {name}: {error}"));
        let rows: usize = report
            .batches
            .iter()
            .map(|batch| batch.record_batch().num_rows())
            .sum();
        assert_eq!(rows, 10, "scan-limit at {name} answered {rows} rows for LIMIT 10");
        let pulled = report.trace.iter().filter(|e| e.call == CallKind::NextBatch).count();
        assert_eq!(
            pulled, 1,
            "scan-limit at {name}: the scan was pulled {pulled} times, and its first batch \
             holds the ten rows"
        );
        assert_eq!(report.in_flight_bytes, 0, "scan-limit at {name} ended holding batches");
    }
}

/// A limited subquery under an aggregate. DataFusion's pushdown erases the limit node and
/// leaves the cut in the scan alone, so before #186 no reader applied it and every mode
/// counted all 25 rows. `count(*)` would not show it: statistics answer that, and the
/// translator refuses the plan (#158).
#[tokio::test]
async fn a_limited_subquery_under_an_aggregate_counts_only_its_cut_at_every_mode() {
    super::sql_answers_match_datafusion(
        "tpch",
        "limited subquery",
        "SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)",
        None,
        super::Coverage::ModesOnly,
    )
    .await;
}
```

  `sql_answers_match_datafusion` and `Coverage` are private to `tests/end_to_end.rs`, and a
  child module sees them through `super::`. distinct-companions moves the body into
  `sql_answers_match_oracle`; the wrapper may have gone. If so, call
  `super::sql_answers_match_oracle("tpch", "limited subquery", sql, sql, None,
  super::Coverage::ModesOnly)`, with `sql` bound to the query. The engine's SQL is then the
  oracle's too.

  In `a_limit_slices_at_most_two_batches_and_stops_the_scan`, the doc's last sentence becomes
  "…and the mid-plan one as a `GpuLimit` over the part scan, with each scan's pushed cut a
  `GpuLimit` beneath — so one query holds every count below." Replace the `limit_node` block
  and its helper with:

```rust
        // No limit holds anything whatever its offset, which is what the slice symbol buys:
        // its queue never carries more than the one batch it was handed. Three of them: the
        // cut over each scan, and the offset above part's.
        let limits = limit_nodes(tree.as_ref());
        assert_eq!(limits.len(), 3, "nested-limits at {name}: limits at {limits:?}");
        for limit in limits {
            assert!(
                report.peak_queued[limit] <= 1,
                "nested-limits at {name}: the limit at {limit} queued {} batches",
                report.peak_queued[limit]
            );
        }
        // Each scan maps only the row groups its cut needs, one apiece, so the plan offers
        // exactly the batches pulled at every mode: the read is bounded by the mapping.
        // Stopping a scan that offers more is the driver's mock cases' to prove.
        assert_eq!(
            offered, pulled,
            "nested-limits at {name}: the scans map {offered} batches for {pulled} pulls"
        );
```

```rust
    /// Every limit's index in the driver's pre-order numbering, which is the tree walked
    /// children-after-self.
    fn limit_nodes(root: &dyn crate::plan::GpuNode) -> Vec<usize> {
        fn walk(node: &dyn crate::plan::GpuNode, next: &mut usize, found: &mut Vec<usize>) {
            if matches!(as_node_ref(node), NodeRef::Limit(_)) {
                found.push(*next);
            }
            *next += 1;
            for child in node.children() {
                walk(child, next, found);
            }
        }
        let mut found = Vec::new();
        walk(root, &mut 0, &mut found);
        found
    }
```

  Delete `most_offered` with everything that reads it: its `let mut`, the `most_offered =
  most_offered.max(offered);` line, and the `assert!(most_offered > 2, …)` after the loop with
  its comment. The comment above `let pulled` changes. "the scan under it never reads a second
  one whatever the batching" becomes "and the scan under it maps no second one".

- [x] **Step 3: Run the tests and confirm they fail.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator::tests tests::end_to_end::limits
```

  Expected first: the build fails, because `RowInterval::over` does not exist.

  With Step 4's `over` in place and nothing else:
  - `…nothing_above_it…` FAILS at tp1-single: `row_interval()` is `None`.
  - `…under_a_subquery_or_a_join…` FAILS at tp1-single, on the shape (no `Limit`).
  - `…reach_its_limit…` FAILS on `LIMIT 10` at tp1-single, which maps `[0, 1]`.
  - `…from_one_read…` FAILS at tp1-single, on the mapping (all 49 groups).
  - `…counts_only_its_cut…` FAILS at tp1-single: 25 rows counted, where DataFusion counts 3.
  - The nested-limits case FAILS at tp1-single, because `limits.len()` is 1.
  - The `over` table PASSES.

- [x] **Step 4: Add `RowInterval::over`.** In `plan/interval.rs`, the module doc becomes
  "[`RowInterval`](super::RowInterval)'s arithmetic: which rows of a batch it wants, and one
  interval over another's output." After `range_of`, add:

```rust
pub(crate) fn over(outer: &RowInterval, inner: &RowInterval) -> RowInterval {
    // What the inner cut leaves once the outer skip is spent in it; `None` is unbounded.
    let left = inner.fetch.map(|fetch| fetch.saturating_sub(outer.skip));
    RowInterval {
        skip: inner.skip + outer.skip,
        fetch: match (outer.fetch, left) {
            (Some(outer), Some(left)) => Some(outer.min(left)),
            (outer, left) => outer.or(left),
        },
    }
}
```

  In `plan/mod.rs`, after `range_of`:

```rust
    /// This interval applied to what `inner` emits, as one interval over `inner`'s input:
    /// how a root limit and the cut DataFusion pushed into the scan beneath it become the
    /// unload's one interval.
    pub(crate) fn over(&self, inner: &RowInterval) -> RowInterval {
        interval::over(self, inner)
    }
```

- [x] **Step 5: Change the translator.** In `nodes.rs`, `source` becomes four functions:

```rust
/// A scan, and above it the cut DataFusion pushed into it. The loader is one lane over the
/// row groups the cut needs, under the mode's own mapping; the limit cuts the batch that
/// straddles `n`, and once it is satisfied the driver's hold on its subtree stops the scan.
pub(crate) fn source(t: &Translator, parquet: &ParquetExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let loader = loader(t, parquet)?;
    Ok(match pushed_limit(parquet) {
        Some(interval) => Box::new(GpuLimit::new(loader, interval)),
        None => loader,
    })
}

/// The loader alone, whatever DataFusion pushed into the scan: a limited scan's survivors
/// cut to the prefix that reaches the limit, before the mapping sees them.
fn loader(t: &Translator, parquet: &ParquetExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let config = parquet.base_config();
    let mut scan = survivor_metadata(parquet)?;
    if let Some(n) = config.limit {
        scan.groups = covering_prefix(scan.groups, n as u64);
    }
    let lanes = lanes_for(t, &scan.groups, config.limit);
    let partition_groups = partition(&scan.groups, lanes, batching_for_source(t))?;

    let projection = match &config.projection {
        Some(columns) => columns.iter().map(|c| *c as u32).collect(),
        None => (0..config.file_schema.fields().len() as u32).collect(),
    };
    Ok(Box::new(GpuLoadParquet::new(
        parquet_table_name(parquet).unwrap_or_default(),
        projection,
        partition_groups,
        &scan,
        None,
        Schema::new(parquet.schema()),
    )))
}

/// The shortest prefix of `groups` whose rows reach `n`, and never fewer than one group, so
/// the mapping has a group to address when `n` is 0. A scan's cut is its first `n` rows,
/// and the metadata says where they are; the readers return every row of a group they read,
/// so the groups after the prefix hold none of them. Without this a single-batch mode reads
/// the whole file for `LIMIT 10`.
fn covering_prefix(mut groups: Vec<RowGroupMeta>, n: u64) -> Vec<RowGroupMeta> {
    let mut rows = 0;
    let keep = groups
        .iter()
        .position(|group| {
            rows += group.rows;
            rows >= n
        })
        .map_or(groups.len(), |last| last + 1);
    groups.truncate(keep);
    groups
}

/// The limit DataFusion pushed into the scan, as the cut it means: the first `n` rows. It
/// pushes `skip + fetch` and keeps a limit node above for any skip, so the cut carries none.
fn pushed_limit(parquet: &ParquetExec) -> Option<RowInterval> {
    parquet.base_config().limit.map(|n| RowInterval {
        skip: 0,
        fetch: Some(n as u64),
    })
}

/// What the unload reads, and the cut DataFusion pushed into a scan with nothing between it
/// and the root. That cut is root-adjacent, so it is the unload's interval and not a node —
/// a `GpuLimit` feeding only the sink is refused by the validator. The descent passes what
/// `node` translates to no node above a one-lane scan: a `CoalescePartitionsExec`, a
/// `CoalesceBatchesExec` without a fetch, and a round-robin `RepartitionExec`. Any other
/// shape translates as usual.
pub(crate) fn unload_input(
    t: &Translator,
    plan: &Arc<dyn ExecutionPlan>,
) -> Result<(Box<dyn GpuNode>, Option<RowInterval>), PlanError> {
    let any = plan.as_any();
    if let Some(parquet) = any.downcast_ref::<ParquetExec>() {
        return Ok((loader(t, parquet)?, pushed_limit(parquet)));
    }
    if let Some(coalesce) = any.downcast_ref::<CoalescePartitionsExec>() {
        let (input, pushed) = unload_input(t, coalesce.input())?;
        return Ok((merged(input), pushed));
    }
    if let Some(coalesce) = any.downcast_ref::<CoalesceBatchesExec>() {
        if coalesce.fetch().is_none() {
            return unload_input(t, coalesce.input());
        }
    }
    if let Some(repartition) = any.downcast_ref::<RepartitionExec>() {
        if matches!(repartition.partitioning(), Partitioning::RoundRobinBatch(_)) {
            return unload_input(t, repartition.input());
        }
    }
    Ok((node(t, plan)?, None))
}
```

  The two pass-through arms are exactly what `node` does for those operators: it returns its
  input's translation with nothing added. No DataFusion 45 plan is known to put either between
  a root limit and a limited scan. They are here so that such a plan folds instead of being
  refused at plan time. If clippy asks for `if let … && …` chains instead of the nested `if`,
  follow it.

  `node`'s parquet arm keeps calling `source`. `lanes_for`'s comment (l.375-379) becomes:

```rust
    // A limit DataFusion pushed into the scan is a cut over the scan's rows in order, and
    // rows from several lanes have no order a cut could follow: one lane, under the mode's
    // own mapping. The `GpuLimit` above it, or the unload, makes the cut.
```

  In `translator/mod.rs`, `use nodes::node;` becomes `use nodes::unload_input;`, and:

```rust
    /// The root, with the limit lowering rule applied: a root-adjacent limit is not a
    /// node at all — its interval becomes the unload's, because a limit over a stream
    /// about to leave the device is a statement about which rows are worth moving. The cut
    /// DataFusion pushed into a scan with nothing above it is root-adjacent too, and folds
    /// into any limit DataFusion kept at the root for an offset.
    pub(crate) fn translate(
        &self,
        root: &Arc<dyn ExecutionPlan>,
    ) -> Result<Box<dyn GpuNode>, PlanError> {
        let (input, interval) = match limit_interval(root) {
            Some((input, interval)) => (input, Some(interval)),
            None => (Arc::clone(root), None),
        };
        let (input, pushed) = unload_input(self, &input)?;
        let interval = match (interval, pushed) {
            (Some(root_limit), Some(pushed)) => Some(root_limit.over(&pushed)),
            (root_limit, pushed) => root_limit.or(pushed),
        };
        Ok(Box::new(GpuUnload::new(input, interval)))
    }
```

  `batching_for_source` is still called once per scan, in the same order, so tp4-sized's two
  passes reach the same sources (`planner/pipeline.rs`). Both passes see the trimmed survivors,
  so the first pass sizes the batches the second maps.

- [x] **Step 6: Run the planner's own tests.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner:: plan_text:: plan:: tests::end_to_end::limits
```

  Expected: PASS, except the plan goldens. `planner::tests::plan_goldens` fails on the two
  queries' sections until Step 7.
  - `a_root_adjacent_limit_is_not_a_node_at_all` passes unchanged.
  - `schema_tests::a_limit_inside_a_limit_plans` passes unchanged. It may now carry a `GpuLimit`
    over nation's scan under the cross join, where DataFusion pushes the root limit into both
    sides; that limit is mid-plan and valid.

- [x] **Step 7: Regenerate the plan goldens.** First confirm that no other golden has a limited
  scan:

```bash
grep -l 'GpuLoadParquet.*limit=' testdata/goldens/*/*.plans.txt
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git status --short testdata/goldens
```

  Expected:
  - The `grep` lists only the five `tpch.sf1/*.plans.txt`.
  - After the run, the same five are modified, and nothing under `tpcds.sf1`.
  - `recipe-payloads.txt` is **not** modified. The payload test verifies its digests under plain
    `UPDATE_CANONICAL` and passes, since no payload query has a limited scan. If it fails, stop:
    the wire bytes moved.

  If instead `the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce` names a new
  call shape:
  1. Add `("tpch", "nested-limits")` to `PAYLOAD_QUERIES` (`planner/tests/plan_goldens.rs`
     ~l.172), with a one-line reason.
  2. Run:

```bash
UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

  3. Read the `recipe-payloads.txt` diff: one new section and no other line moved. The test
     points `/tmp/peacock-plan-bytes-root` at this workspace's `testdata` itself, so the buffers
     embed the same paths on every machine.

  Then check that only the two sections moved, in every file this task regenerates:

```bash
cat > /tmp/limits-sections.py <<'EOF'
import subprocess, sys
def sections(text):
    out, name = {}, None
    for line in text.splitlines():
        if line.startswith('== '):
            name = line[3:]
            out[name] = []
        elif name is not None:
            out[name].append(line)
    return out
for path in sys.argv[1:]:
    old = subprocess.run(['git', 'show', f'HEAD:{path}'], capture_output=True, text=True, check=True).stdout
    with open(path) as f:
        new = f.read()
    a, b = sections(old), sections(new)
    print(path, sorted(n for n in a.keys() | b.keys() if a.get(n) != b.get(n)))
EOF
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*.plans.txt
```

  Expected: each file prints `['nested-limits', 'scan-limit']`. At tp1-single, nested-limits'
  tree reads:

```
== nested-limits
GpuUnload: skip=3, fetch=20
  GpuCrossJoin: lanes=1, batches=multiple, schema=[k:Int64]
    GpuCoalesceAllBatches: lanes=1, batches=single, schema=[]
      GpuLimit: skip=0, fetch=23, lanes=1, batches=multiple, schema=[]
        GpuLoadParquet: table=region, projections=[], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[]
    GpuProject: exprs=[p_partkey@0 as k], lanes=1, batches=multiple, schema=[k:Int64]
      GpuLimit: skip=5, fetch=23, lanes=1, batches=multiple, schema=[p_partkey:Int64]
        GpuLimit: skip=0, fetch=28, lanes=1, batches=multiple, schema=[p_partkey:Int64]
          GpuLoadParquet: table=part, projections=[p_partkey@0], partition_groups=[[[0]]], lanes=1, batches=multiple, schema=[p_partkey:Int64]
```

  The rest of what moves:
  - nested-limits gains two more `GpuLimit` lines in `--- recipes ---` (`calling_lanes=1, per
    straddling batch: slice_handle(batch, row range)`) and in `--- memory ---`.
  - The part loader's `source_bytes` falls from 1,600,062 to its first row group's bytes.
  - The part loader maps `[[[0]]]` at every mode, the rowgroup modes included, where it mapped
    `[[[0],[1]]]`.
  - scan-limit's tree is `GpuUnload: skip=0, fetch=10` over a loader with `partition_groups=[[[0]]]`
    and no `limit=`, at every mode. Its memory section prices that one row group, not
    371,110,838 bytes.
  - At the tp4 modes scan-limit's unload already carried `skip=0, fetch=10`.

  `grep -c 'GpuLoadParquet.*limit=' testdata/goldens/tpch.sf1/*.plans.txt` prints `0` for each.

- [x] **Step 8: Regenerate the execution sections.** For a filtered run, merge only and never
  prune (`build-test.md`, Golden files):

```bash
PCK_UPDATE_SECTIONS=1 timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*-mini.cpu.txt
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*-mini.cost.txt
git status --short testdata/goldens/tpch.sf1/mini.result.txt testdata/goldens/tpcds.sf1
```

  A cost section is numbers only, with no node text. A section moves there only where a node's
  output bytes moved. Expected:

  | file | prints | why |
  |---|---|---|
  | `tp1-single-mini.cpu.txt`, `tp1-rowgroup-mini.cpu.txt` | `['nested-limits']` | scan-limit's tp1 sections stay `skipped: not enabled at this mode` until Task 4 |
  | `tp4-single-mini.cpu.txt`, `tp4-rowgroup-mini.cpu.txt`, `tp4-sized-mini.cpu.txt` | `['nested-limits', 'scan-limit']` | the loader line loses `limit=10` and maps `[[[0]]]` |
  | `tp1-single-mini.cost.txt`, `tp1-rowgroup-mini.cost.txt`, `tp4-rowgroup-mini.cost.txt` | `['nested-limits']` | scan-limit is `skipped` at tp1. At tp4-rowgroup it already read one row group, so its bytes do not move |
  | `tp4-single-mini.cost.txt`, `tp4-sized-mini.cost.txt` | `['nested-limits', 'scan-limit']` | scan-limit reads one row group where it read 49 |

  `git status` prints nothing.

  Read the nested-limits `.cpu.txt` diff at each mode:
  - The new limit over part is `in_rows=[[122880]]` and emits 28 rows, 228 bytes.
  - The outer limit's `in_rows=[[28]]`, and the root still has `output_rows=20`.
  - The part loader is `partition_groups=[[[0]]]`, `output_rows=122880`, `output_bytes=998400`.
  - `early_exit=` names `GpuUnload@8` and the two part-side limits (`@5`, `@4`), in the
    renderer's order. It never names the region limit (`@1`, five rows under a cut of 23).

  In the cost sections:
  - nested-limits reads `cuda_limit_bytes=415` (187 + 228), `storage_read_bytes=998400` and
    `peacockdb_cost=1000100` at all five modes. That is down from 1,626,472 at tp1-single,
    tp4-single and tp4-sized, and up from 999,872 at tp1-rowgroup and tp4-rowgroup.
  - scan-limit at tp4-single and tp4-sized reads `peacockdb_cost=21476312`, tp4-rowgroup's
    unchanged figure. That is down from 1,048,882,882.

  If a figure differs, read why before going on.

  Then run the cost gate as CI runs it, against this branch's base: the parent task's branch.

```bash
BASE=$(git merge-base HEAD ENS-distinct-companions)
cargo run -q -p cost-report -- --cost-diff --base "$BASE" --html /tmp/limits-cost-diff.html --md-diff /tmp/limits-cost-diff.md; echo "rc=$?"
grep '🔴' /tmp/limits-cost-diff.md
```

  Expected:
  - `rc=1`.
  - Exactly two 🔴 rows, nested-limits at tp1-rowgroup and at tp4-rowgroup, each 999,872 →
    1,000,100.
  - Five 🟢 rows: nested-limits at the three other modes, and scan-limit at tp4-single and
    tp4-sized.

  The exit is 1 because of those two rows. The command is not failing; CI's gate will fail the
  PR on them until the human accepts them (Task 5 records them). Any other 🔴 row is a
  regression this task did not expect: stop and find it. These goldens are uncommitted at this
  point, which is what the diff reads: the working tree against `$BASE`.

- [x] **Step 9: Run the tiers that read them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner:: tests::end_to_end
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
```

  Expected: all PASS. `tests::end_to_end` includes nested-limits under the injected layouts and
  the limited subquery at the five modes.

- [ ] **Step 10: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/plan/interval.rs peacockdb-core/src/planner/translator/nodes.rs peacockdb-core/src/planner/translator/tests.rs peacockdb-core/src/tests/end_to_end/limits.rs
git add peacockdb-core/src/plan peacockdb-core/src/planner peacockdb-core/src/tests/end_to_end/limits.rs testdata/goldens/tpch.sf1
git commit -m "#186: a scan's limit is a GpuLimit above it, or the unload's interval" -m "A limited scan maps only the row groups that reach its limit." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The loader, the wire and the device lose the limit (#186)

A removal: the compiler names every reader. No golden moves.

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs` (`GpuLoadParquet` l.888-922), `plan/source.rs`
  (`new_load_parquet` l.57-82), `planner/translator/nodes.rs` (`loader`),
  `plan_text/node_text.rs` (l.85-87), `wire/node_writer.rs` (l.93), `wire/fb_text.rs`
  (l.28-30), `wire/generated.rs` (the line count in its comment)
- Modify: `flatbuffers/gpu_plan.fbs` (`CudfScan.limit`, l.334-335), `cpp/src/operators/scan.cpp` (l.61-63)
- Modify (one argument fewer): `wire/tests.rs:598`, `tests/injection.rs:570`,
  `tests/rebuild.rs:48,407,430`, `planner/tests/null_analysis.rs:45`,
  `executor/cpu_backend/tests/source.rs:70,206`, `executor/gpu_backend/gpu_tests/mod.rs:141`,
  `executor/driver/tests/plans.rs:45`, `plan/tests/mod.rs:285,652`,
  `tests/gpu_tests/source_cases.rs:59`
- Modify: `tests/rebuild.rs` (the `source` fixture), `tests/injection.rs:36,553,787`
- Test: `peacockdb-core/src/tests/gpu_tests/source_cases.rs`, `source_schema_cases.rs:16`

**Interfaces:**
- Consumes: Task 2 — no planned loader carries a limit.
- Produces: `GpuLoadParquet::new(table, projection, partition_groups, scan, schema)`;
  `tests::rebuild::source()` with no argument, and a private `second_source()`.

- [x] **Step 1: Delete the field.**
  - `plan/mod.rs`: the `limit` field and its doc; the `limit` parameter of
    `GpuLoadParquet::new` and its forwarding.
  - `plan/source.rs`: the same parameter, and the `limit,` initializer.
  - `nodes.rs`'s `loader`: the `None,`.
  - `plan_text/node_text.rs`: the three-line `if let Some(limit) = load.limit { … }`.

- [x] **Step 2: Build, and fix every caller the compiler names.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib --no-run 2>&1 | grep -E '^error|-->' | head -40
```

  Remove the limit argument (`None,`, `limit,`, `load.limit,` or `Some(3),`) at each place in
  the list above.

  In `tests/rebuild.rs`, split the fixture so that two children of one schema still differ. The
  joins and unions paired `source(None)` with `source(Some(7))`. With the limit gone those two
  would be identical, and a swap of them would be invisible to the identity check:

```rust
/// Two lanes, three row groups between them, and a column that holds a NULL beside one
/// that does not — the loader's fields that no plan line prints.
pub(crate) fn source() -> Box<dyn GpuNode> {
    source_named("part")
}

/// [`source`] under another table and file, so a node with two children of one schema has
/// children a rebuild could tell apart, and a swap of them shows.
fn second_source() -> Box<dyn GpuNode> {
    source_named("partsupp")
}

fn source_named(table: &str) -> Box<dyn GpuNode> {
    let groups: Vec<RowGroupMeta> = (0..3)
        .map(|index| RowGroupMeta {
            index,
            rows: 100 + u64::from(index),
            bytes: 800,
        })
        .collect();
    let scan = ScanMetadata {
        file: format!("/{table}.parquet"),
        groups,
        can_be_null: vec![false, true],
    };
    Box::new(GpuLoadParquet::new(
        table.to_string(),
        vec![0, 1],
        vec![vec![vec![0], vec![1]], vec![vec![2]]],
        &scan,
        columns(),
    ))
}
```

  Then, in `rebuild.rs`:
  - every `source(None)` becomes `source()`, and every `source(Some(7))` becomes
    `second_source()` (`sed -i 's/source(None)/source()/g; s/source(Some(7))/second_source()/g'
    peacockdb-core/src/tests/rebuild.rs`);
  - `other_source` loses its `Some(3),`.

  In `tests/injection.rs`, the two `source(None)` become `source()`.

- [x] **Step 3: Change the wire.**
  - In `node_writer.rs`, delete `limit: node.limit.unwrap_or(0) as u64,`.
  - In `fb_text.rs`, delete the `if scan.limit() > 0 { … }` block.
  - In `gpu_plan.fbs`:

```
  /// Never written: a scan's limit is a `GpuLimit` above it, or the unload's interval
  /// (#186). The slot stays, so no later field moves.
  limit: uint64 (deprecated);
```

  flatc then generates neither the Rust accessor nor the `CudfScanArgs` field, so a reader
  left behind is a compile error.

- [x] **Step 4: Change the C++.** In `scan.cpp`, delete:

```cpp
  if (scan->limit() > 0) {
    opts.set_num_rows(static_cast<cudf::size_type>(scan->limit()));
  }
```

  No `CreateCudfScan` call passes a limit. Every call in `cpp/tests/gpu/test_plan_executor.cpp`
  stops at the projection (`grep -n 'CreateCudfScan(' cpp/tests cpp/src -r`), so the generated
  signature losing a later parameter moves none of them.

- [x] **Step 5: Turn the four `bug_` device cases into agreement cases.** In
  `tests/gpu_tests/source_cases.rs`:
  - `scan` and `read_both` lose their `limit` parameter: `scan(path, schema)` and
    `read_both(name, batch, rows_per_group)`.
  - Every existing caller drops its `None`: `both_backends_read_the_same_batches_per_row_group`,
    `a_decimal_column_is_exported_at_its_declared_precision` and
    `a_parquet_of_zero_rows_reads_as_nothing_on_both`.
  - `source_schema_cases.rs:16` becomes `scan(&path, batch.schema())`.
  - Delete `gpu_refuses_with`, `cpu_answered`, `four_sixteens`, `GROUPS_AND_LIMIT`, the comment
    block above the `bug_` cases, and the four `bug_` cases.

  The imports become:

```rust
use super::script::{Outcome, Script, each_answers, run_both};
use crate::plan::{
    BatchLayout, GpuLimit, GpuLoadParquet, RowGroupMeta, RowInterval, ScanMetadata, Schema,
};
use crate::tests::compare::Order;
use crate::tests::given::Given;
use crate::tests::synthetic::{decimals, synthetic};
```

  The module doc's "with and without a pushed-down limit" becomes "and a limit over the
  batches a scan emitted, which is what a scan's limit is". Put these in place of the deleted
  cases:

```rust
// A scan's limit is a `GpuLimit` over the batches the scan emits (#186): the reader reads
// its row groups whole, and the limit cuts. Each limit case's stream is a real scan's
// output — the file both backends just read, as the cpu emitted it — so the cut is over
// what a scan hands it, not batches sliced by hand as `harness_cases::limit_over`'s are.

/// `GpuLimit 0..+fetch` over the batches a per-row-group scan of `synthetic(64, 1)` in
/// groups of `rows_per_group` emitted, once both backends were seen to emit the same ones.
fn limit_over_scan(name: &str, fetch: u64, rows_per_group: usize) -> (GpuLimit, Vec<RecordBatch>) {
    let whole = synthetic(64, 1);
    let read = read_both(name, &whole, rows_per_group);
    read.same(Order::AsEmitted);
    let batches: Vec<RecordBatch> = read
        .cpu
        .expect("the cpu read the file")
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(batches.len(), 64 / rows_per_group, "one batch per row group");
    let node = GpuLimit::new(
        Given::of(Schema::new(whole.schema()), BatchLayout::MultipleBatches),
        RowInterval {
            skip: 0,
            fetch: Some(fetch),
        },
    );
    (node, batches)
}

operator_case! {
    GpuLoadParquet,
    fn a_scan_of_one_row_group_reads_it_whole_on_both() {
        read_both("one-group", &synthetic(64, 1), 64).same(Order::AsEmitted);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_over_a_scan_of_one_row_group_keeps_its_first_rows_on_both() {
        let (node, batches) = limit_over_scan("limit-one-group", 10, 64);
        // One slot for the batch, one for the finish.
        let expected = [vec![batches[0].slice(0, 10)], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_inside_the_first_of_four_scanned_row_groups_drops_the_rest_on_both() {
        let (node, batches) = limit_over_scan("limit-inside-first", 10, 16);
        let expected = [vec![batches[0].slice(0, 10)], vec![], vec![], vec![], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_across_a_scanned_row_group_boundary_slices_the_second_group_on_both() {
        let (node, batches) = limit_over_scan("limit-across-groups", 20, 16);
        let expected = [
            vec![batches[0].clone()],
            vec![batches[1].slice(0, 4)],
            vec![],
            vec![],
            vec![],
        ];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}
```

  `RecordBatch` is already imported. If `Given` lives elsewhere, take its path from
  `harness_cases.rs`'s imports. These are built in Step 7 and **not run**: chain K has no GPU,
  and #281 runs them.

- [x] **Step 6: Run the rust-only tiers.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_module_layout
git status --short testdata
```

  Expected: PASS. That includes `plan::tests::layout_injection` over the split fixture, and
  `planner::tests::plan_goldens` with no golden written. `git status` prints nothing.
  `recipe-payloads.txt` is unchanged, since a scan's `limit` was `0` and unwritten in every
  payload.

- [x] **Step 7: Build the C++ and the FFI rungs locally against cuDF 25.02; never run the device.**

```bash
timeout 3600 scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run
```

  Expected:
  - The C++ build compiles `scan.cpp` and `peacock_plan_tests`, which is never run here because
    it needs a device.
  - `ctest -L cpu` PASSES.
  - The four `ffi_tests::` PASS; they need no device.
  - Both `--no-run` builds compile.

  If the `ffi_tests::` binary exits 127, prepend the FFI `OUT_DIR`'s `lib` and the cuDF root's
  `lib` to `LD_LIBRARY_PATH` (`build-test.md`, "A cudf-shape binary needs `LD_LIBRARY_PATH`").

  Then check the generated file's length, which `wire/generated.rs`'s comment states. It was
  7,336 lines at 64ced62e; distinct-companions' deprecation may have moved it.

```bash
wc -l "$(ls -t target/debug/build/peacockdb-core-*/out/gpu_plan_generated.rs | head -1)"
```

  If it differs from the comment, update the comment's figure.

- [ ] **Step 8: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/plan/source.rs peacockdb-core/src/plan_text/node_text.rs peacockdb-core/src/wire/node_writer.rs peacockdb-core/src/wire/fb_text.rs peacockdb-core/src/tests/rebuild.rs peacockdb-core/src/tests/injection.rs peacockdb-core/src/tests/gpu_tests/source_cases.rs peacockdb-core/src/tests/gpu_tests/source_schema_cases.rs
git clang-format --diff HEAD -- cpp/src/operators/scan.cpp
git add flatbuffers/gpu_plan.fbs cpp/src/operators/scan.cpp peacockdb-core/src
git commit -m "#186: GpuLoadParquet.limit and set_num_rows go; CudfScan.limit deprecated" -m "The four bug_ source cases become agreement cases over a real scan's output, built and not run (#281)." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: scan-limit's tp1 cpu cells on

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (l.32-42, l.62-73)
- Modify: `testdata/cost-registry.csv` (the `nested_limits` and `scan_limit` rows)
- Regenerate: `testdata/goldens/tpch.sf1/tp1-single-mini.{cpu,cost}.txt`,
  `tp1-rowgroup-mini.{cpu,cost}.txt` (the `scan-limit` sections)
- Modify: `llm-wiki/tickets/corpus-coverage.md` (#281)

- [x] **Step 1: Turn on the corpus line.** Use the shape the file's lines have when this task
  builds. distinct-companions, or chain J's duckdb-oracle, may have added a field; copy a
  neighbour's. scan-limit's line gains the two tp1 modes:

```
corpus_query!(tpch, 1, scan_limit, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_subset, live_cpu, schema_validation_enabled);
```

  The comments:
  - Above the line, delete the three lines from "scan-limit runs at the tp4 modes only" through
    "where ten were asked for, #186.". The last two lines become "on #220's join batching, and
    scan-limit on #281: its cut, the unload's interval, has not run on a device."
  - At l.67, "nested-limits on #186 (two intervals on one path, scan-limit's refusal)" becomes
    "nested-limits on #281 (the limits over its scans have not run on a device)".

- [x] **Step 2: Run the two new cells and confirm they fail on a missing section.**

```bash
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit
```

  Expected: `cpu_tpch_scan_limit_tp1_single` and `…_tp1_rowgroup` FAIL, because their sections
  say `skipped: not enabled at this mode`. The tp4 three PASS.

- [x] **Step 3: Write the two sections.**

```bash
PCK_UPDATE_SECTIONS=1 timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*-mini.cpu.txt testdata/goldens/tpch.sf1/*-mini.cost.txt
git status --short testdata/goldens/tpch.sf1/mini.result.txt
```

  `/tmp/limits-sections.py` is Task 2's; recreate it from there if it is gone. It diffs against
  `HEAD`, which is Task 3's commit.

  Expected:
  - The two tp1 files print `['scan-limit']`, and the tp4 files print `[]`.
  - `mini.result.txt` is unchanged.
  - Each new `.cpu.txt` section opens `early_exit=GpuUnload@1`, with its unload at `skip=0,
    fetch=10, output_rows=10`.
  - Its loader maps `partition_groups=[[[0]]]`, with `batch_rows=[[122880]]`.
  - Each new `.cost.txt` section reads `peacockdb_cost=21476312`, as at the tp4 modes.

- [x] **Step 4: Update the registry.** In `cost-registry.csv`, `scan_limit`'s `cpu_tp1_single`
  and `cpu_tp1_rowgroup` become `enabled`. In both rows, the `186` in `tickets` becomes `281`.
  At master's columns:

```
tpch,1,nested_limits,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,limit_offset cross_join,281
tpch,1,scan_limit,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,top_n,281
```

  If K or J changed the columns, edit those cells by header name. The gpu cells do not move,
  so the device registry test, which only the GPU job runs, cannot move either.

- [x] **Step 5: Run the tests and the cost gate.**

```bash
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits registry
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
timeout 900 cargo test -p cost-report
BASE=$(git merge-base HEAD ENS-distinct-companions)
cargo run -q -p cost-report -- --cost-diff --base "$BASE" --html /tmp/limits-cost-diff.html --md-diff /tmp/limits-cost-diff.md; echo "rc=$?"
grep '🔴' /tmp/limits-cost-diff.md
```

  Expected:
  - All tests PASS: the registry both ways, the plan goldens' registry meta, and cost-report's
    comment-size assertion over the real registry.
  - The cost gate prints the same two 🔴 rows as Task 2 Step 8, and `rc=1`.
  - scan-limit's two new tp1 sections have a `skipped` base, so the gate omits them, never
    counting them as a regression.

- [x] **Step 6: Update #281.** Its "`scan.cpp` no longer applies a scan's limit, which a
  `GpuLimit` above the scan now does (#186)" becomes "…which a `GpuLimit` above the scan, or
  the unload's interval where nothing sits between, now does (#186)". It already names
  scan-limit and nested-limits at every device mode.

- [ ] **Step 7: Commit.**

```bash
git add peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv testdata/goldens/tpch.sf1 llm-wiki/tickets/corpus-coverage.md
git commit -m "#186: scan-limit's tp1 cpu cells on; its device cells and nested-limits' on #281" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The docs, the counts, the cost gate's rows, the bar

**Files:**
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`, `llm-wiki/tickets/memory.md` (#229)
- Modify: `testdata/tpch-queries/nested-limits.sql` (the header comment only)
- Modify: `llm-wiki/tasks/limits-detail.md` (a `## Cost gate` section; create the file if the
  coordinator has not)

- [x] **Step 1: Update `architecture.md`.** Use short sentences; this page is read for one fact
  at a time.
  - **The node table (l.182), `GpuLoadParquet`:** "reads survivor row groups per the mapping;
    `next_batch()`. Carries no limit: one DataFusion pushed into the scan is a `GpuLimit` above
    it, or the unload's interval, and the survivors are cut to the row groups it needs".
  - **The node table, `GpuLimit` (l.195):** append "; satisfied once it has emitted `fetch`
    rows".
  - **"The limit lowering rule", the paragraph at l.392-397**, becomes:

    > **A scan carrying a pushed-down limit plans one lane, over the row groups the cut
    > needs, and the cut is a node.** DataFusion pushes `skip + fetch` into the scan and, over
    > one partition, erases the limit node above it, so the scan's limit can be the whole cut.
    > Rows from several lanes have no order a cut could follow, so the scan is one lane, under
    > the mode's own mapping. Its survivors are first cut to the shortest prefix whose
    > parquet-metadata row counts reach the limit, so a small limit reads one row group at
    > every mode. The cut is a `GpuLimit` directly above the scan. With nothing between the
    > scan and the root, it is the unload's interval instead, composed with any root limit
    > DataFusion kept for an offset. It slices the batch that straddles the cut, and its
    > satisfaction holds the scan. No reader applies a limit.

  - **"Intervals nest" (l.415-417):** its last sentence, "DataFusion's `combine_limit` merges
    the adjacent form, so only a limited subquery under a join, or a limit at the root, reaches
    this layer as two intervals.", becomes:

    > DataFusion's `combine_limit` merges the adjacent form. Two intervals reach this layer only
    > in two cases. One is a limited subquery under a join or under a root limit. The other is
    > a scan's pushed cut under the limit DataFusion keeps for an offset: `tpch/nested-limits`
    > runs a `GpuLimit` 5..+23 over the part scan's own 0..+28. Where that scan sits directly
    > under the root, the two fold into the unload's one interval.

  - **"Early exit at a limit" (l.659):** the opening sentence, "A `GpuUnload` carrying a
    root-adjacent interval is the one node the driver special-cases.", becomes "The driver holds
    the subtree of every satisfied interval. It keeps the count itself for one carrier, a
    `GpuUnload` with a root-adjacent interval." After "…a pure offset has no such point and is
    never satisfied." (l.664-667), add:

    > A mid-plan `GpuLimit` is held the same way, but its input is one lane, so its executor
    > keeps the only count of it and makes every cut. The driver counts the rows the limit
    > emitted and holds it once they reach `fetch`. A second count of its input would be a
    > second computation of one decision, free to drift from the first.

  - **The Python model sentence (l.647-650):** after "…build-test.md says what it runs.", add
    "One rule differs: the model still judges a mid-plan limit by its input
    (`partitioned_driver.py`), where the driver now reads what the limit emitted (#234)."
  - **The wire table (l.872), `CudfScan`:** drop `limit` from the steering fields, and add
    "`limit` is deprecated: a scan's limit is a `GpuLimit` or the unload's interval". Drop
    "`set_num_rows(limit)`" from the cuDF column.
  - **The options table (l.1119):** drop "`, set_num_rows(limit)`".
  - **The field table (l.1149):** delete the `CudfScan.limit` row.

  Afterwards, run `grep -n 'pushed\|set_num_rows\|limit=\|special-cases' llm-wiki/architecture.md`.
  Every hit must still be true.

- [x] **Step 2: Fix the false header comment in nested-limits.sql.** In
  `testdata/tpch-queries/nested-limits.sql`, "the outer interval narrows the inner fetch to 23,
  and the scan reads 28 rows in all." becomes "the outer interval narrows the inner fetch to 23,
  and the part scan maps only its first row group, the one holding the 28 rows the cut needs."
  The SQL itself does not change. No golden carries the comment.

- [x] **Step 3: Update #229** (`tickets/memory.md` ~l.101-106).
  - "#186's fix caps the scan inside `scan.cpp` and leaves this path alone." becomes "Every
    limit DataFusion pushes into a scan below the root reaches it too, as a `GpuLimit` over
    the scan (#186)."
  - Its **Corpus queries** line becomes "`tpch/nested-limits` slices `part(p_partkey)`, an Int64,
    at both of its part-side limits."

- [x] **Step 4: Update the `build-test.md` counts.** Recount each row this task touches from the
  code at the branch head. Then set every header the rows sum into, and the grand total, to the
  sums, so the page adds up. This task's deltas, against whatever the base says after
  guard-checks and distinct-companions:

| row | delta | reason |
|---|--:|---|
| Corpus, cpu (`test_cpu_corpus`) | +2 | scan-limit at tp1-single and tp1-rowgroup; "N cells" +2 |
| End to end | +2 | `a_scan_limit_answers_its_count_from_one_read_at_every_mode`, `a_limited_subquery_under_an_aggregate_counts_only_its_cut_at_every_mode`; "Two of the N … so N−2 run" moves with it |
| Drivers over a mock backend | +4 | Task 1 |
| Translator, one rule at a time | +3 | four cases replace one |
| Operator harness (gpu) | 0 | four `bug_` cases out, four agreement cases in |
| cpu block: `--lib` +9, `test_cpu_corpus` +2 | +11 | |
| Rust header, grand total | +11 | |

  The descriptions change too:
  - The cpu corpus row drops "`tpch/scan-limit` two by [#186](…)," from its disabled list.
  - The end-to-end row's count of cases no query list can carry rises by two (ten at master,
    then whatever distinct-companions left). The row gains "a scan's limit answering its count
    from one row group at every mode, and a limited subquery under an aggregate".
  - The driver row's "both limit lowerings by the calls not made" gains "and a mid-plan limit
    judged by the rows it emitted, never its input".

- [x] **Step 5: Run the full verification bar and the cost gate.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 7200 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_module_layout
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
git status --short testdata
BASE=$(git merge-base HEAD ENS-distinct-companions)
cargo run -q -p cost-report -- --cost-diff --base "$BASE" --html /tmp/limits-cost-diff.html --md-diff /tmp/limits-cost-diff.md; echo "rc=$?"
cat /tmp/limits-cost-diff.md
```

  Expected:
  - All green. `git status` prints only the `nested-limits.sql` comment edit; no run rewrote a
    golden.
  - The cost gate prints `rc=1` with the two 🔴 rows of Task 2 Step 8.
  - The device tier is not run: chain K has no GPU.

- [x] **Step 6: Record the gate's rows for the human.** Append to `llm-wiki/tasks/limits-detail.md`
  the section below, with the figures as `/tmp/limits-cost-diff.md` printed them. Put every 🔴
  row in the table, and list the 🟢 rows after it:

```markdown
## Cost gate

`cargo run -q -p cost-report -- --cost-diff --base <merge-base with ENS-distinct-companions>`
at the branch head: 2 regressions, 5 improvements. CI's cost-report job fails this PR on the
regressions until the human accepts them.

| section | base | branch | why |
|---|--:|--:|---|
| tpch/nested-limits, tp1-rowgroup | 999,872 | 1,000,100 | the `GpuLimit 0..+28` over the part scan emits 28 Int64 rows (228 bytes); part was already read one row group at a time here, so nothing offsets them |
| tpch/nested-limits, tp4-rowgroup | 999,872 | 1,000,100 | the same |

Improvements: nested-limits at tp1-single, tp4-single and tp4-sized, 1,626,472 → 1,000,100
(the part scan maps one row group, not two). scan-limit at tp4-single and tp4-sized,
1,048,882,882 → 21,476,312 (lineitem maps one row group, not 49). scan-limit's tp1 sections are
new against a `skipped` base and omitted.
```

- [ ] **Step 7: Commit.**

```bash
git add llm-wiki/architecture.md llm-wiki/build-test.md llm-wiki/tickets/memory.md llm-wiki/tasks/limits-detail.md testdata/tpch-queries/nested-limits.sql
git commit -m "#186, #234: the limit lowering rule and the driver's limit count as built; counts" -m "The cost gate's two nested-limits rows recorded in limits-detail.md for the human." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

  The task's final message names the two 🔴 rows. CI's cost-report job is red on them, and the
  task waits at `completeness approved` until the human accepts them.

## As built — round 1

Every step is done except the five **Commit** steps: this developer does not mutate git state,
so the work is left in the working tree for the coordinator. Three places where the plan and the
tree disagreed, all recorded in [`limits-detail.md`](limits-detail.md#round-1-evidence):

- **Task 4's Steps 3 and 4 run in the opposite order.** The `skipped: not enabled at this mode`
  placeholder is written from `cost-registry.csv`, not from `corpus_cases.inc`, so with the
  registry still `disabled` a `PCK_UPDATE_SECTIONS=1` run republishes the placeholder and the
  cell passes against it — green, with the section never written. The registry edit goes first.
- **`Empties::fires` needed one change** (`tests/injection.rs`, not in the plan's file list).
  The injected empty-batch stamp is a function of the node's post-order, so the two new
  `GpuLimit`s moved nested-limits' part source to a stamp under which none of its two calls
  fired, and `run_and_check`'s `empty_batches() > 0` went red at
  `tp1-single rebatch=sources/empties=50%`. `fires` now fires every source's first call, which
  removes the class rather than this instance; the guard was shown red-green afterwards.
- **`wire/generated.rs`'s line count was already stale.** The comment said 7,336; the base fbs
  generates 7,268 and this branch's 7,252. Corrected to 7,252.

One caller outside the plan's list: `executor/gpu_backend/gpu_tests/mod.rs:141`, which only the
`--features gpu --no-run` build can see.
