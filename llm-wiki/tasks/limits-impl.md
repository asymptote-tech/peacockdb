# limits implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A limit DataFusion pushed into a scan stops being the readers' job: it becomes a
`GpuLimit` directly above a one-lane scan, or the unload's interval where nothing sits between
the scan and the root, and `GpuLoadParquet.limit`, `CudfScan.limit` and `set_num_rows` go
(#186). The driver judges a mid-plan limit by the rows it emitted, never by a second count of
its input (#234). scan-limit's two tp1 cpu cells turn on.

**Architecture:** `source()` (`planner/translator/nodes.rs`) builds the loader without a limit
and puts `GpuLimit { skip: 0, fetch: Some(n) }` over it. `Translator::translate` reaches the
root's input through a new `unload_input`, which hands the pushed cut of a scan with nothing
above it back to the unload, composed with any root limit by `RowInterval::over` — because the
validator refuses a `GpuLimit` feeding only the sink (`plan/validate.rs:40-49`), and a
root-adjacent interval is the unload's by the limit lowering rule. In the driver
(`executor/driver/partitioned.rs`), `settle_limit` reads a new `rows_emitted` for a mid-plan
limit and keeps `rows_seen` for the unload alone.

**Tech stack:** Rust over DataFusion 45 physical plans, cpu tier (`--features rust-only`);
FlatBuffers schema; C++ against cuDF 25.02, built and `ctest -L cpu` only; the gpu-feature Rust
lib built, never run.

**Spec:** [`limits.md`](limits.md). Tickets: [#186](../tickets/corpus-coverage.md#t186),
[#234](../tickets/corpus-coverage.md#t234), [#281](../tickets/corpus-coverage.md#t281).

## Global constraints

- **No GPU** (chain K's board header). No device run, no GPU cycle. The task is `done` when every
  CI job but the GPU tests is green. `scan.cpp`, the four device source cases and every other
  `gpu_tests` edit are **built locally and never run**; #281 holds them.
- **Base: master after chain K merged.** Line numbers below are from master at 64ced62e; K moves
  some (`build-test.md` counts, `gpu_plan.fbs`, `corpus_cases.inc`). Find by the quoted text.
- **The root-adjacent fold is a deliberate deviation from the spec.** The spec puts a
  `GpuLimit` above every limited scan. Where the scan is the unload's input — `tpch/scan-limit`
  at every mode, and the spec's own translator query — that node feeds only the sink, which
  `limit_positions` refuses. The cut becomes the unload's interval instead, so scan-limit's plan
  goldens gain no `GpuLimit`: they lose `limit=10` and, at the two tp1 modes, the unload gains
  `skip=0, fetch=10`. nested-limits gains two `GpuLimit`s, one over each scan.
- **No backend change.** Both `LimitStream`s keep their own count and every drop, forward and
  slice decision. `CpuSource` is untouched. The unload's handling is untouched.
- **Wire:** `CudfScan.limit` marked `(deprecated)`, no slot moved, as chain K did for
  `AggregateFuncNode.distinct`. `recipe-payloads.txt` must not move: no payload query has a
  limited scan, and a `0` at its default is not written.
- **Goldens:** only the `scan-limit` and `nested-limits` sections move, in the five tpch
  `.plans.txt`, `-mini.cpu.txt` and `-mini.cost.txt`. No tpcds golden, no
  `mini.result.txt`, no `recipe-payloads.txt`.
- Builds as `build-test.md` documents them: rust-only in `./target`; C++ in `cpp/build`
  (`scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2
  --gcc-version 12`; drop `--configure` once configured); cudf-feature cargo only through
  `CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 scripts/cargo-cudf.sh`. Build in a
  workspace; never share a cargo target dir across worktrees. Wrap every build and test in
  `timeout` (the developer's rule); the bounds below are generous.
- rustfmt only the leaf files touched (`rustfmt --edition 2024 <file>…`), never a `mod.rs`.
  rustfmt follows `mod` declarations, so restore any file it reformatted that the task did not
  touch (`git checkout -- <path>`).
- Commit messages at most 10 lines, ending `Co-Authored-By: Claude Opus 5.5
  <noreply@anthropic.com>`. #186 and #234 are archived at merge by the helper, not here.

## Review focus

1. **A root limit with an OFFSET over a scan DataFusion pushed `skip + fetch` into.** `LIMIT 3
   OFFSET 2` reaches the translator as a root `GlobalLimitExec(2, 3)` over a scan carrying
   `limit=5`. The fold must give the unload exactly `2..+3`: not `2..+5`, not `4..+3`. Task 2:
   the OFFSET query at every mode, and `RowInterval::over`'s table (an outer skip past the inner
   cut, a pure offset over a cut, a cut over a pure offset).
2. **A mid-plan limit with a skip.** The driver must compare the rows it emitted with
   `fetch`, never with `skip + fetch` — the slip that reuses `satisfied_by` on the new count.
   The mock forwards whole batches, so `limit(25, 5)` emits ten rows from the first batch:
   satisfied after one pull, where the old rule reads three. Task 1.
3. **`fetch = 0` with a skip, and no `fetch` at all, mid-plan.** `LIMIT 0 OFFSET 5` is satisfied
   before any pull (the old rule pulled one batch); a pure offset is never satisfied, so a
   `fetch.unwrap_or(0)` slip would stop the scan at the seed. Task 1, two cases.
4. **A limit over a zero-column scan.** nested-limits' region scan projects nothing
   (`projections=[]`) and now carries `GpuLimit 0..+23` with five rows under it: rows with no
   columns pass a limit that is never satisfied. Pinned at all five modes by the nested-limits
   corpus cells and the end-to-end limit test (Task 2); its device half is #281.
5. **A limited scan mapped to many batches.** At tp1-rowgroup lineitem is 49 batches on one
   lane. The cut must stop the scan after the first: Task 2's end-to-end case asserts one
   `NextBatch` at every mode, and that the answer is ten rows (6,001,215 today at the tp1 modes).

## File structure

| file | responsibility |
|---|---|
| `peacockdb-core/src/plan/mod.rs`, `plan/interval.rs` | `RowInterval::satisfied_by_emitted`, `RowInterval::over`; `GpuLoadParquet.limit` gone |
| `peacockdb-core/src/executor/driver/partitioned.rs` | `rows_emitted`; `settle_limit` per carrier |
| `peacockdb-core/src/executor/driver/tests/mock.rs`, `tests/limit.rs` | `with_limit`, `AccRule::Trimming`; four cases |
| `peacockdb-core/src/planner/translator/nodes.rs`, `translator/mod.rs` | `source`/`loader`/`pushed_limit`/`unload_input`; `translate` folds |
| `peacockdb-core/src/planner/translator/tests.rs` | three cases replace one |
| `peacockdb-core/src/tests/end_to_end/limits.rs` | scan-limit at every mode; every limit in nested-limits |
| `peacockdb-core/src/plan/source.rs`, `plan_text/node_text.rs`, `wire/node_writer.rs`, `wire/fb_text.rs`, `wire/generated.rs` (comment) | the loader's limit gone |
| every other `GpuLoadParquet::new` caller (13, listed in Task 3) | one argument fewer |
| `flatbuffers/gpu_plan.fbs`, `cpp/src/operators/scan.cpp` | `CudfScan.limit` deprecated; `set_num_rows` gone |
| `peacockdb-core/src/tests/gpu_tests/source_cases.rs`, `source_schema_cases.rs` | the four `bug_` cases become agreement cases |
| `testdata/goldens/tpch.sf1/*`, `peacockdb-core/tests/common/corpus_cases.inc`, `testdata/cost-registry.csv` | scan-limit, nested-limits |
| `llm-wiki/architecture.md`, `build-test.md`, `tickets/corpus-coverage.md` (#281), `tickets/memory.md` (#229) | docs, counts |

---

### Task 1: The driver judges a mid-plan limit by the rows it emitted (#234)

Independent of the plan change, and moves no golden: on today's plans a mid-plan limit is
satisfied at the same step under either count.

**Files:**
- Modify: `peacockdb-core/src/plan/mod.rs` (`impl RowInterval`, ~l.1040-1058)
- Modify: `peacockdb-core/src/executor/driver/partitioned.rs` (fields ~l.69-73, `new` ~l.173,
  `run_lane_scoped` ~l.284-360, `settle_limit` ~l.613-625)
- Modify: `peacockdb-core/src/executor/driver/tests/mock.rs` (`AccRule` ~l.65, `Script`
  ~l.111-165, `MockAcc` ~l.408-445, `executors_for` ~l.634-650)
- Test: `peacockdb-core/src/executor/driver/tests/limit.rs`

**Interfaces:**
- Produces: `RowInterval::satisfied_by_emitted(&self, emitted: u64) -> bool`;
  `Script::with_limit(self, AccRule) -> Script`; `AccRule::Trimming(usize)`.

- [ ] **Step 1: The mock's limit gets a rule of its own.** In `mock.rs`, a variant on `AccRule`
  after `Streaming`:

```rust
    /// Emits each batch cut to at most this many rows and holds nothing — a limit letting
    /// through less than it was handed.
    Trimming(usize),
```

  A field on `Script`, after `accumulate`, defaulting to `AccRule::Streaming` in `Default`:

```rust
    /// What a `GpuLimit`'s executor does with each batch. Apart from `accumulate`, because a
    /// limit streams whatever the other accumulators do, and the rows it lets through are
    /// what the driver judges it by.
    pub limit: AccRule,
```

  A builder after `with_accumulator`, whose doc's last sentence becomes "A `GpuLimit` follows
  `with_limit` instead.":

```rust
    pub(crate) fn with_limit(mut self, limit: AccRule) -> Self {
        self.limit = limit;
        self
    }
```

  In `MockAcc::accumulate_and_fetch`, an arm before `_`:

```rust
            AccRule::Trimming(most) => (
                vec![MockBatch {
                    rows: batch.rows.min(most),
                    bytes: batch.bytes,
                }],
                stats,
            ),
```

  and in `mark_done_and_fetch` the first arm becomes `AccRule::Streaming | AccRule::Trimming(_)
  => Vec::new(),`. In `executors_for`, `script.accumulate = AccRule::Streaming;` becomes
  `script.accumulate = script.limit;`, and its comment's first sentence "a limit follows the
  script's `limit` rule whatever the other accumulators do".

- [ ] **Step 2: Write the failing tests**, in `limit.rs` after
  `a_satisfied_limit_reports_done_so_the_node_above_it_can_finish`. Its import line becomes
  `use super::mock::{AccRule, Script, spec};`.

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
    let report = mid_plan(5, Some(0), AccRule::Streaming);
    assert!(!report.satisfied.is_empty(), "satisfied at the seed");
    assert_eq!(count(&report, CallKind::NextBatch), 0);
    assert_eq!(rows_returned(&report), 0);
    assert_eq!(report.in_flight_bytes, 0);
}
```

  `RunReport`, `count`, `run`, `rows_returned` and `assert_accounted` come in through `use
  super::*;`, as the file's other cases use them. In
  `a_mid_plan_limit_stops_its_own_subtree_and_holds_nothing`, "the driver counts the rows
  going past it" becomes "the driver reads the rows it emitted", and its assert message "once
  twelve rows had gone past the limit" becomes "once twelve rows had left the limit".

- [ ] **Step 3: Run them; three fail.**

```bash
timeout 1200 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver::tests::limit
```

  Expected: `…whatever_its_skip` FAILS (3 pulls, expected 1), `…keeps_the_driver_pulling`
  FAILS (2, expected 3), `…of_no_rows…` FAILS (1, expected 0); `…with_no_fetch…` PASSES — it
  is the guard against a `None` read as `0`. Every older case PASSES.

- [ ] **Step 4: The predicate.** In `plan/mod.rs`, `satisfied_by`'s doc becomes "True once
  `seen` rows of this node's input leave no later row able to change the answer — what an
  unload is judged by.", and after it:

```rust
    /// True once a limit has emitted `fetch` rows, whatever its skip: what it let through is
    /// counted, never what it was handed. `None` is a pure offset and never satisfies.
    pub(crate) fn satisfied_by_emitted(&self, emitted: u64) -> bool {
        self.fetch.is_some_and(|fetch| emitted >= fetch)
    }
```

- [ ] **Step 5: The driver.** In `partitioned.rs`, the field doc and a second field:

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

  `rows_emitted: vec![0; nodes],` beside `rows_seen` in `new`. In `run_lane_scoped`, only the
  unload peeks its input:

```rust
            let arriving = match (interval, unloading && consuming) {
                (Some(_), true) => self.peek_rows(node, lane, call),
                _ => 0,
            };
            // Only an unload's decision is made here, its range being an argument of the
            // driver's own call. A mid-plan limit makes the same three-way choice inside its
            // executor, and the driver reads what it emitted.
```

  In the `LaneOutputs::Device(batches)` loop, right after `self.record_emitted(node, lane,
  &batch);`:

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
  `indexed.lanes` into a local first. The file must stay under 1000 lines (947 today).

- [ ] **Step 6: Run the driver tests and the two limit queries.**

```bash
timeout 1200 cargo test --features rust-only -p peacockdb-core --lib -- executor::driver tests::end_to_end::limits
timeout 1800 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_nested_limits cpu_tpch_scan_limit
git status --short testdata
```

  Expected: all PASS with no golden written — nested-limits' `GpuLimit 5..+23` is handed
  200,000 rows and emits 23 in the same step, so `early_exit=` and every count stand. `git
  status` prints nothing.

- [ ] **Step 7: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/executor/driver/partitioned.rs peacockdb-core/src/executor/driver/tests/mock.rs peacockdb-core/src/executor/driver/tests/limit.rs
git add peacockdb-core/src/plan/mod.rs peacockdb-core/src/executor/driver
git commit -m "#234: the driver judges a mid-plan limit by the rows it emitted" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: A scan's limit is a `GpuLimit` above it, or the unload's interval (#186, planner)

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
- Produces: `RowInterval::over(&self, inner: &RowInterval) -> RowInterval` — this interval
  applied to what `inner` emits, as one interval over `inner`'s input.
- Produces: `nodes::unload_input(t, plan) -> Result<(Box<dyn GpuNode>, Option<RowInterval>),
  PlanError>`. After this task no planned loader carries a limit (it is built with `None` until
  Task 3 deletes the field), so plan text already prints no `limit=`.

- [ ] **Step 1: The translator tests.** Replace `a_scan_carrying_a_pushed_down_limit_plans_one_lane`
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
async fn a_scan_limit_under_another_node_is_a_limit_over_a_one_lane_scan_at_every_mode() {
    // An expression DataFusion cannot push into the scan keeps the limit off the unload, so
    // the cut is a node of its own directly above the loader.
    for (mode, tree) in planned_at_every_mode("SELECT n_nationkey + 1 AS k FROM nation LIMIT 3").await
    {
        let limit = find(tree.as_ref(), &is_limit_over_a_loader).unwrap_or_else(|| {
            panic!("at {mode}: no limit over a loader in {}", shape(tree.as_ref()))
        });
        assert_eq!(
            limit.row_interval(),
            Some(RowInterval { skip: 0, fetch: Some(3) }),
            "at {mode}"
        );
        assert_eq!(limit.children()[0].kind().layout().unwrap().n, 1, "at {mode}");
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
  breaks them over lines, let it.

- [ ] **Step 2: The end-to-end cases**, in `tests/end_to_end/limits.rs`. A new test after the
  existing one:

```rust
/// A limit DataFusion pushed into the scan answers its count at every mode, and from one
/// read: the scan's cut stops it whatever the mode maps — forty-nine batches over lineitem at
/// the rowgroup modes. At the tp1 modes the cpu reader ignored the scan's limit and answered
/// all 6,001,215 rows (#186).
#[tokio::test]
async fn a_scan_limit_answers_its_count_from_one_read_at_every_mode() {
    use crate::executor::CallKind;

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
```

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

- [ ] **Step 3: Run them; they fail.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::translator::tests tests::end_to_end::limits
```

  Expected: does not compile — `RowInterval::over` does not exist. With Step 4's `over` in
  place and nothing else: `…nothing_above_it…` FAILS at tp1-single (`row_interval()` is `None`);
  `…under_another_node…` FAILS (no limit over a loader); `…from_one_read…` FAILS at tp1-single
  (6,001,215 rows); the nested-limits case FAILS (`limits.len()` is 1); the `over` table
  PASSES.

- [ ] **Step 4: `RowInterval::over`.** In `plan/interval.rs`, the module doc becomes
  "[`RowInterval`](super::RowInterval)'s arithmetic: which rows of a batch it wants, and one
  interval over another's output." and, after `range_of`:

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

- [ ] **Step 5: The translator.** In `nodes.rs`, `source` becomes three functions:

```rust
/// A scan, and above it the cut DataFusion pushed into it. The loader is one lane under the
/// mode's own mapping; the limit cuts the batch that straddles `n`, and once it is satisfied
/// the driver's hold on its subtree stops the scan, so a small limit reads one batch.
pub(crate) fn source(t: &Translator, parquet: &ParquetExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let loader = loader(t, parquet)?;
    Ok(match pushed_limit(parquet) {
        Some(interval) => Box::new(GpuLimit::new(loader, interval)),
        None => loader,
    })
}

/// The loader alone, whatever DataFusion pushed into the scan.
fn loader(t: &Translator, parquet: &ParquetExec) -> Result<Box<dyn GpuNode>, PlanError> {
    let config = parquet.base_config();
    let scan = survivor_metadata(parquet)?;
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
/// a `GpuLimit` feeding only the sink is refused by the validator. The descent passes only
/// what DataFusion puts between a root limit and a bare scan: nothing over one partition, a
/// `CoalescePartitionsExec` over several, which is no node above a one-lane scan. Any other
/// shape translates as usual, and a limit it leaves under the unload is refused at plan time.
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
    Ok((node(t, plan)?, None))
}
```

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
  passes reach the same sources (`planner/pipeline.rs`).

- [ ] **Step 6: Run the planner's own tests.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner:: plan_text:: plan:: tests::end_to_end::limits
```

  Expected: PASS, the plan goldens excepted (`planner::tests::plan_goldens` fails on the two
  queries' sections until Step 7). `a_root_adjacent_limit_is_not_a_node_at_all` and
  `schema_tests::a_limit_inside_a_limit_plans` pass unchanged — the second may now carry a
  `GpuLimit` over nation's scan under the cross join, where DataFusion pushes the root limit
  into both sides; that limit is mid-plan and valid.

- [ ] **Step 7: The plan goldens.**

```bash
UPDATE_CANONICAL=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
git status --short testdata/goldens
```

  Expected: the five `tpch.sf1/*.plans.txt` modified; nothing under `tpcds.sf1`; and
  `recipe-payloads.txt` **not** modified — the payload test verifies its digests under plain
  `UPDATE_CANONICAL` and passes, since no payload query has a limited scan. If it fails, stop:
  the wire bytes moved. If instead
  `the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce` names a new call
  shape, add `("tpch", "nested-limits")` to `PAYLOAD_QUERIES` (`planner/tests/plan_goldens.rs`
  ~l.172) with a one-line reason and run

```bash
UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
```

  — the test points `/tmp/peacock-plan-bytes-root` at this workspace's `testdata` itself, so
  the buffers embed the same paths on every machine. Read the `recipe-payloads.txt` diff: one
  new section and no other line moved.

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

  Expected: each file prints `['nested-limits', 'scan-limit']`. At tp1-single the two read:

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
          GpuLoadParquet: table=part, projections=[p_partkey@0], partition_groups=[[[0,1]]], lanes=1, batches=multiple, schema=[p_partkey:Int64]
```

  with two more `GpuLimit` lines in `--- recipes ---` (`calling_lanes=1, per straddling batch:
  slice_handle(batch, row range)`) and `--- memory ---`; and scan-limit's tree is `GpuUnload:
  skip=0, fetch=10` over its loader with no `limit=`. At the tp4 modes scan-limit's unload
  already carried `skip=0, fetch=10`; only `limit=10` leaves its loader line.
  `grep -c 'GpuLoadParquet.*limit=' testdata/goldens/tpch.sf1/*.plans.txt` prints `0` for each.

- [ ] **Step 8: The execution sections.** Merge only, never prune, for a filtered run
  (`build-test.md`, Golden files):

```bash
PCK_UPDATE_SECTIONS=1 timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*-mini.cpu.txt testdata/goldens/tpch.sf1/*-mini.cost.txt
git status --short testdata/goldens/tpch.sf1/mini.result.txt testdata/goldens/tpcds.sf1
```

  Expected: every tp4 file prints `['nested-limits', 'scan-limit']` and every tp1 file
  `['nested-limits']` — scan-limit's tp1 sections stay `skipped: not enabled at this mode`
  until Task 4. The `git status` prints nothing. Read the nested-limits diff: two new
  `GpuLimit` nodes; `early_exit=` names `GpuUnload@8` and the two part-side limits (`@5`,
  `@4`), in the renderer's order, and never the region limit (`@1`, five rows under a cut of
  23); the outer limit's `in_rows=[[28]]`; the root still `output_rows=20`.

- [ ] **Step 9: The tiers that read them.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib -- planner:: tests::end_to_end
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
```

  Expected: all PASS. `tests::end_to_end` includes nested-limits under the injected layouts.

- [ ] **Step 10: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/plan/interval.rs peacockdb-core/src/planner/translator/nodes.rs peacockdb-core/src/planner/translator/tests.rs peacockdb-core/src/tests/end_to_end/limits.rs
git add peacockdb-core/src/plan peacockdb-core/src/planner peacockdb-core/src/tests/end_to_end/limits.rs testdata/goldens/tpch.sf1
git commit -m "#186: a scan's limit is a GpuLimit above it, or the unload's interval" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
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

- [ ] **Step 1: Delete the field.** In `plan/mod.rs`, the `limit` field and its doc, and the
  `limit` parameter of `GpuLoadParquet::new` and its forwarding; the same parameter and the
  `limit,` initializer in `plan/source.rs`; the `None,` in `nodes.rs`'s `loader`. In
  `plan_text/node_text.rs`, the three-line `if let Some(limit) = load.limit { … }`.

- [ ] **Step 2: Build; fix every caller the compiler names.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib --no-run 2>&1 | grep -E '^error|-->' | head -40
```

  Each `None,` (or `limit,`, `load.limit,`, `Some(3),`) argument in the list above goes. In
  `tests/rebuild.rs`, the fixture splits so two children of one schema still differ — the
  joins and unions paired `source(None)` with `source(Some(7))`, and with the limit gone
  those would be identical and a swap of them invisible to the identity check:

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

  Then in `rebuild.rs` every `source(None)` becomes `source()` and every `source(Some(7))`
  `second_source()` (`sed -i 's/source(None)/source()/g; s/source(Some(7))/second_source()/g'
  peacockdb-core/src/tests/rebuild.rs`); `other_source` loses its `Some(3),`. In
  `tests/injection.rs`, the two `source(None)` become `source()`.

- [ ] **Step 3: The wire.** In `node_writer.rs`, delete `limit: node.limit.unwrap_or(0) as
  u64,`. In `fb_text.rs`, delete the `if scan.limit() > 0 { … }` block. In `gpu_plan.fbs`:

```
  /// Never written: a scan's limit is a `GpuLimit` above it, or the unload's interval
  /// (#186). The slot stays, so no later field moves.
  limit: uint64 (deprecated);
```

  flatc then generates neither the Rust accessor nor the `CudfScanArgs` field, so a reader
  left behind is a compile error.

- [ ] **Step 4: The C++.** In `scan.cpp`, delete:

```cpp
  if (scan->limit() > 0) {
    opts.set_num_rows(static_cast<cudf::size_type>(scan->limit()));
  }
```

  No `CreateCudfScan` call passes a limit: every call in `cpp/tests/gpu/test_plan_executor.cpp`
  stops at the projection (`grep -n 'CreateCudfScan(' cpp/tests cpp/src -r`), so the generated
  signature losing a later parameter moves none of them.

- [ ] **Step 5: The four device cases.** In `tests/gpu_tests/source_cases.rs`, `scan` and
  `read_both` lose their `limit` parameter (`scan(path, schema)`, `read_both(name, batch,
  rows_per_group)`), the two existing callers drop their `None`, and `source_schema_cases.rs:16`
  becomes `scan(&path, batch.schema())`. Delete `gpu_refuses_with`, `cpu_answered`,
  `four_sixteens`, `GROUPS_AND_LIMIT`, the comment block above the `bug_` cases and the four
  `bug_` cases. Imports become:

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
  batches a scan emits, which is what a scan's limit is". In their place:

```rust
// A scan's limit is a `GpuLimit` over the batches the scan emits (#186): the reader reads
// its row groups whole, and the limit cuts. Each limit case's stream is a scan's own output,
// `synthetic(64, 1)` in groups of `rows_per_group`.

/// `GpuLimit 0..+fetch` over the batches a per-row-group scan of `synthetic(64, 1)` emits.
fn limit_over_scan(fetch: u64, rows_per_group: usize) -> (GpuLimit, Vec<RecordBatch>) {
    let whole = synthetic(64, 1);
    let batches = (0..64 / rows_per_group)
        .map(|i| whole.slice(i * rows_per_group, rows_per_group))
        .collect();
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
        let (node, batches) = limit_over_scan(10, 64);
        // One slot for the batch, one for the finish.
        let expected = [vec![batches[0].slice(0, 10)], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_inside_the_first_of_four_row_groups_drops_the_rest_on_both() {
        let (node, batches) = limit_over_scan(10, 16);
        let expected = [vec![batches[0].slice(0, 10)], vec![], vec![], vec![], vec![]];
        let outcome = run_both(&node, Script::Accumulate(batches));
        each_answers(&outcome, &expected, &expected);
    }
}

operator_case! {
    GpuLimit,
    fn a_limit_across_a_row_group_boundary_slices_the_second_group_on_both() {
        let (node, batches) = limit_over_scan(20, 16);
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

  If `Given` lives elsewhere, take its path from `harness_cases.rs`'s imports. These are built
  in Step 7 and **not run**: chain K has no GPU, and #281 runs them.

- [ ] **Step 6: The rust-only tiers.**

```bash
timeout 1800 cargo test --features rust-only -p peacockdb-core --lib
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_module_layout
git status --short testdata
```

  Expected: PASS, including `plan::tests::layout_injection` over the split fixture and
  `planner::tests::plan_goldens` with no golden written; `git status` prints nothing —
  `recipe-payloads.txt` unchanged, since a scan's `limit` was `0` and unwritten in every
  payload.

- [ ] **Step 7: The C++ and the FFI rungs — built, the device never run.**

```bash
timeout 3600 scripts/build.sh --configure --build --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run
CUDF_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2 timeout 3600 scripts/cargo-cudf.sh test -p peacockdb-core --test test_gpu_corpus --no-run
```

  Expected: the C++ build compiles `scan.cpp` and `peacock_plan_tests` (never run here: a
  device); `ctest -L cpu` PASSES; the four `ffi_tests::` PASS (no device); both `--no-run`
  builds compile. If the `ffi_tests::` binary exits 127, prepend the FFI `OUT_DIR`'s `lib` and
  the cuDF root's `lib` to `LD_LIBRARY_PATH` (`build-test.md`, "A cudf-shape binary needs
  `LD_LIBRARY_PATH`"). Then the generated file's length, which `wire/generated.rs`'s comment
  states (7,336 lines at 64ced62e; chain K may have moved it):

```bash
wc -l "$(ls -t target/debug/build/peacockdb-core-*/out/gpu_plan_generated.rs | head -1)"
```

  If it differs from the comment, update the comment's figure.

- [ ] **Step 8: Commit.**

```bash
rustfmt --edition 2024 peacockdb-core/src/plan/source.rs peacockdb-core/src/plan_text/node_text.rs peacockdb-core/src/wire/node_writer.rs peacockdb-core/src/wire/fb_text.rs peacockdb-core/src/tests/rebuild.rs peacockdb-core/src/tests/injection.rs peacockdb-core/src/tests/gpu_tests/source_cases.rs peacockdb-core/src/tests/gpu_tests/source_schema_cases.rs
git clang-format --diff HEAD -- cpp/src/operators/scan.cpp
git add flatbuffers/gpu_plan.fbs cpp/src/operators/scan.cpp peacockdb-core/src
git commit -m "#186: GpuLoadParquet.limit and set_num_rows go; CudfScan.limit deprecated" -m "The four bug_ source cases become agreement cases, built and not run (#281)." -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: scan-limit's tp1 cpu cells on

**Files:**
- Modify: `peacockdb-core/tests/common/corpus_cases.inc` (l.32-42, l.62-73)
- Modify: `testdata/cost-registry.csv` (the `nested_limits` and `scan_limit` rows)
- Regenerate: `testdata/goldens/tpch.sf1/tp1-single-mini.{cpu,cost}.txt`,
  `tp1-rowgroup-mini.{cpu,cost}.txt` (the `scan-limit` sections)
- Modify: `llm-wiki/tickets/corpus-coverage.md` (#281)

- [ ] **Step 1: The corpus line**, in the shape the file's lines have when this task builds
  (chain J's duckdb-oracle may add a field; copy a neighbour's). scan-limit's line gains the
  two tp1 modes:

```
corpus_query!(tpch, 1, scan_limit, tp1_single | tp1_rowgroup | tp4_single | tp4_rowgroup | tp4_sized, none, data_fusion_subset, live_cpu, schema_validation_enabled);
```

  The comment above it: delete the three lines from "scan-limit runs at the tp4 modes only"
  through "where ten were asked for, #186.", and the last two lines become "on #220's join
  batching, and scan-limit on #281: its cut, the unload's interval, has not run on a device."
  At l.67, "nested-limits on #186 (two intervals on one path, scan-limit's refusal)" becomes
  "nested-limits on #281 (the limits over its scans have not run on a device)".

- [ ] **Step 2: Run; the two new cells fail on a missing section.**

```bash
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit
```

  Expected: `cpu_tpch_scan_limit_tp1_single` and `…_tp1_rowgroup` FAIL — their sections say
  `skipped: not enabled at this mode`; the tp4 three PASS.

- [ ] **Step 3: Write them.**

```bash
PCK_UPDATE_SECTIONS=1 timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit
python3 -I /tmp/limits-sections.py testdata/goldens/tpch.sf1/*-mini.cpu.txt testdata/goldens/tpch.sf1/*-mini.cost.txt
git status --short testdata/goldens/tpch.sf1/mini.result.txt
```

  (`/tmp/limits-sections.py` is Task 2's; recreate it from there if gone. It diffs against
  `HEAD`, which is Task 3's commit.) Expected: the two tp1 files print `['scan-limit']`, the
  tp4 files `[]`; `mini.result.txt` unchanged. Each new section opens `early_exit=GpuUnload@1`,
  its unload `skip=0, fetch=10, output_rows=10`, its loader `batch_rows` one batch.

- [ ] **Step 4: The registry.** In `cost-registry.csv`, `scan_limit`'s `cpu_tp1_single` and
  `cpu_tp1_rowgroup` become `enabled`, and in both rows the `186` in `tickets` becomes `281`.
  At 64ced62e's columns:

```
tpch,1,nested_limits,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,limit_offset cross_join,281
tpch,1,scan_limit,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,enabled,disabled,disabled,disabled,disabled,disabled,ok,top_n,281
```

  If K or J changed the columns, edit those cells by header name. The gpu cells do not move,
  so the device registry test, which only the GPU job runs, cannot move either.

- [ ] **Step 5: Run.**

```bash
timeout 2400 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- cpu_tpch_scan_limit cpu_tpch_nested_limits registry
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 900 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
timeout 900 cargo test -p cost-report
```

  Expected: all PASS — the registry both ways, the plan goldens' registry meta, and
  cost-report's comment-size assertion over the real registry.

- [ ] **Step 6: #281.** Its "`scan.cpp` no longer applies a scan's limit, which a `GpuLimit`
  above the scan now does (#186)" becomes "…which a `GpuLimit` above the scan, or the unload's
  interval where nothing sits between, now does (#186)". It already names scan-limit and
  nested-limits at every device mode.

- [ ] **Step 7: Commit.**

```bash
git add peacockdb-core/tests/common/corpus_cases.inc testdata/cost-registry.csv testdata/goldens/tpch.sf1 llm-wiki/tickets/corpus-coverage.md
git commit -m "#186: scan-limit's tp1 cpu cells on; its device cells and nested-limits' on #281" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The docs, the counts, the bar

**Files:**
- Modify: `llm-wiki/architecture.md`, `llm-wiki/build-test.md`, `llm-wiki/tickets/memory.md` (#229)

- [ ] **Step 1: `architecture.md`.** Short sentences; this page is read for one fact at a time.
  - The node table (l.182), `GpuLoadParquet`: "reads survivor row groups per the mapping;
    `next_batch()`. Carries no limit: one DataFusion pushed into the scan is a `GpuLimit` above
    it, or the unload's interval". `GpuLimit` (l.195): append "; satisfied once it has emitted
    `fetch` rows".
  - "The limit lowering rule", the paragraph at l.392-397 becomes:

    > **A scan carrying a pushed-down limit plans one lane, and the cut is a node.** DataFusion
    > pushes `skip + fetch` into the scan and, over one partition, erases the limit node above
    > it, so the scan's limit can be the whole cut. Rows from several lanes have no order a cut
    > could follow, so the scan is one lane, under the mode's own mapping. The cut is a
    > `GpuLimit` directly above it — or, with nothing between the scan and the root, the
    > unload's interval, composed with any root limit DataFusion kept for an offset. It slices
    > the batch that straddles the cut, and its satisfaction holds the scan, so a small limit
    > reads one batch. No reader applies a limit.

  - "Intervals nest" (l.415-417): append "A scan's cut under a kept offset is the other:
    `tpch/nested-limits` runs a `GpuLimit` 5..+23 over the part scan's own 0..+28."
  - "Early exit at a limit" (l.664-667): after "…a pure offset has no such point and is never
    satisfied.", add: "A mid-plan `GpuLimit` is held the same way, but its input is one lane,
    so its executor keeps the only count of it and makes every cut. The driver counts the rows
    the limit emitted and holds it once they reach `fetch`; a second count of its input would
    be a second computation of one decision, free to drift from the first."
  - The wire table (l.872), `CudfScan`: drop `limit` from the steering fields and add "`limit`
    is deprecated: a scan's limit is a `GpuLimit` or the unload's interval"; drop
    "`set_num_rows(limit)`" from the cuDF column. The options table (l.1119): drop
    "`, set_num_rows(limit)`". The field table (l.1149): delete the `CudfScan.limit` row.
  - `grep -n 'pushed\|set_num_rows\|limit=' llm-wiki/architecture.md` afterwards: every hit
    still true.

- [ ] **Step 2: #229** (`tickets/memory.md` ~l.101-106). "#186's fix caps the scan inside
  `scan.cpp` and leaves this path alone." becomes "Every limit DataFusion pushes into a scan
  below the root reaches it too, as a `GpuLimit` over the scan (#186)." Its **Corpus queries**
  line: "`tpch/nested-limits` slices `part(p_partkey)`, an Int64, at both of its part-side
  limits."

- [ ] **Step 3: `build-test.md` counts.** Recount each row this task touches from the code at
  the branch head, then set every header the rows sum into and the grand total to the sums, so
  the page adds up. This task's deltas, against whatever the base says after chain K:

| row | delta | reason |
|---|--:|---|
| Corpus, cpu (`test_cpu_corpus`) | +2 | scan-limit at tp1-single and tp1-rowgroup; "551 cells" +2 |
| End to end | +1 | `a_scan_limit_answers_its_count_from_one_read_at_every_mode`; "Two of the N … so N−2 run" moves with it |
| Drivers over a mock backend | +4 | Task 1 |
| Translator, one rule at a time | +2 | three cases replace one |
| Operator harness (gpu) | 0 | four `bug_` cases out, four agreement cases in |
| cpu block: `--lib` +7, `test_cpu_corpus` +2 | +9 | |
| Rust header, grand total | +9 | |

  At 64ced62e these read 554→556, 29→30, 109→113, 29→31, `--lib` 605→612, `test_cpu_corpus`
  555→557, cpu block 1189→1198, Rust 1856→1865, grand total 2334→2343. Descriptions: the cpu
  corpus row drops "`tpch/scan-limit` two by [#186](…)," from its disabled list; the end-to-end
  row's "ten cases no query list can carry" becomes eleven and gains "a scan's limit answering
  its count from one read at every mode"; the driver row's "both limit lowerings by the calls
  not made" gains "and a mid-plan limit judged by the rows it emitted, never its input".

- [ ] **Step 4: The full verification bar.**

```bash
timeout 3600 cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2
timeout 7200 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus -- --test-threads=2
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_corpus_goldens
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_cost_model
timeout 600 cargo test --features rust-only -p peacockdb-core --test test_module_layout
timeout 600 ctest --test-dir cpp/build -L cpu --output-on-failure
git status --short testdata
```

  Expected: all green; `git status` prints nothing (no run rewrote a golden). The device tier
  is not run: chain K has no GPU.

- [ ] **Step 5: Commit.**

```bash
git add llm-wiki/architecture.md llm-wiki/build-test.md llm-wiki/tickets/memory.md
git commit -m "#186, #234: the limit lowering rule and the driver's limit count as built; counts" -m "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```
