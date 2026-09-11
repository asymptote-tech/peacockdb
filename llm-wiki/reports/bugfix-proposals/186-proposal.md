# #186 — the CPU backend ignores a limit pushed into the scan

Read at master c18e063a. Paths relative to `/media/data/peacockdb`; `core/` = `peacockdb-core/src/`.

## 1. Issue

`SELECT * FROM lineitem LIMIT 10` returns 6,001,215 rows on `CpuBackend` wherever DataFusion
erased the limit node and left the bound on the scan alone. A wrong answer, not a refusal.

- `core/executor/cpu_backend/source.rs:36-77` — `CpuSource::new` copies `file`, `projection`,
  `partition_groups`, `schema` off the node and never `node.limit`; `read_next` (`:80-109`) builds
  the reader with `with_row_groups` + `with_projection` only (`:86-91`). The plan node carries the
  field (`core/plan/mod.rs:893-894`, rendered `limit=10` by `core/plan_text/node_text.rs:85`) and
  the recipe writer sends it to the device (`core/wire/node_writer.rs:93`), so the two engines
  disagree on this plan by construction.
- Where the interval also lands on the unload — the three tp4 modes, `GpuUnload: skip=0,
  fetch=10` in `testdata/goldens/tpch.sf1/tp4-*.plans.txt` — the driver's range/early-exit hides
  it: the answer is right and the loader reads 6,001,215 rows (`tp4-single-mini.cpu.txt`,
  `== scan-limit`: `GpuLoadParquet … batch_rows=[[6001215]] batch_bytes=[[1048881054]]`) to hand
  over ten. At the two tp1 modes DataFusion's `LimitPushdown` removes the `GlobalLimitExec`
  (skip = 0, source takes the fetch: `datafusion-physical-optimizer-45.0.0/src/limit_pushdown.rs`,
  the non-pushdown arm at the end of `pushdown_limit_helper`), so the plan is a bare
  `GpuUnload` over `GpuLoadParquet(limit=10)` and nothing trims.
- Disabled cells (00-tickets.md row #186, confirmed): `tpch/scan_limit` cpu × `tp1-single`,
  `tp1-rowgroup` — `tests/common/corpus_cases.inc:44-46,51`, `testdata/cost-registry.csv:135`
  (`cpu_tp1_single,cpu_tp1_rowgroup = disabled`, tickets `186 188`). The gpu cells of the same
  row are #188's, and a gpu cell needs its cpu cell (`tests/test_cpu_corpus.rs:93`).
- Not a corpus cell but the same defect: `tpch/nested_limits` runs green at all five modes only
  because a `GpuLimit(skip=5, fetch=23)` sits over `GpuLoadParquet(part, limit=28)`; the loader
  still reads a whole row group (122,880 or 200,000 rows) for 28 — `tp1-single-mini.cpu.txt`,
  `== nested-limits`, `GpuLoadParquet: table=part … output_rows=200000`.

The 00-tickets row is right about the cells. One correction to its "code paths" cell: the
contrast is not only `node_writer.rs:93` — the wire's own doc already fixes the semantics
(`flatbuffers/gpu_plan.fbs:336-337`: `limit: uint64 /// Maximum rows to read (0 = unlimited)`)
and the CPU has a one-line twin of it, `ArrowReaderBuilder::with_limit`, that nothing calls.

## 2. Root cause

Two halves of one design were never written, and the ticket names only the first.

**The executor half.** The engine's rule for a scan limit is per read call, on both sides: the
device's `CudfScan.limit` becomes `parquet_reader_options::set_num_rows` on every
`execute_scan_rowgroups` call (`cpp/src/operators/scan.cpp:61-63`), and the CPU reader has the
same knob, `ParquetRecordBatchReaderBuilder::with_limit` (`parquet-54.2.1/src/arrow/arrow_reader/
mod.rs:221`), which composes with `with_row_groups`: `build()` turns it into a `RowSelection` of
the first `limit` rows over the selected groups (`mod.rs:600-627`, `apply_range` at `:840-870`).
`CpuSource::read_next` never calls it. That is the whole of the wrong answer at tp1-single.

**The planner half.** `architecture.md:374-379` ("The limit lowering rule"): "**A scan carrying a
pushed-down limit plans one lane and one batch.** … `CudfScan.limit` becomes `set_num_rows` on
every scan call, so B batches would answer with B × limit. One lane and one batch make the
loader's own limit the whole answer." The code delivers the lane and not the batch:
`core/planner/translator/nodes.rs:374-390` `lanes_for` returns 1 for a limit; `source()` at
`:392-411` then calls `partition(&scan.groups, lanes, batching_for_source(t))`, and
`core/planner/translator/scan_mapping/partition.rs:81-92` `batches_of` cuts the one lane per row
group under `PerRowGroup` and by bytes under `Sized`, knowing nothing of a limit. The goldens show
it: `tp1-rowgroup.plans.txt` `== scan-limit` has `partition_groups=[[[0],[1],…,[48]]], limit=10`
— 49 calls, each bounded to 10 on the device. Nothing validates the rule
(`core/plan/source.rs:21-50` checks lanes non-empty, batches non-empty, mapped ⊆ survivors).
**This is drift in `architecture.md`: the sentence is false of today's code at the two rowgroup
modes**, and `a_scan_carrying_a_pushed_down_limit_plans_one_lane`
(`core/planner/translator/tests.rs:603-615`) asserts the lane count only.

So a CPU fix that only wires `with_limit` per call would be right at `tp1-single` and wrong at
`tp1-rowgroup` (49 × 10 = 490 rows); a fix that counts across batches in the executor would
contradict the per-call semantics the wire carries and force a device-side count. The root cause
is the missing planner half plus the missing reader call, and the fix is both, each where the
design already put it.

Why the other lowering does not cover it: `limit_interval` (`core/planner/translator/common.rs:
111-149`) puts an interval on the unload only where a `GlobalLimitExec`/`LocalLimitExec` is the
root; DataFusion erases that node whenever `skip == 0` and the source takes the fetch — at tp1 for
any single-partition scan, and at tp4 too for a one-partition file (`SELECT * FROM nation LIMIT 3`
is a bare scan at tp4, which the translator test above proves). A limited scan under a join
(`(SELECT * FROM part LIMIT 40) x, region`) has no unload interval at any mode.

## 3. Localized fix

One statement, owned by the plan: **a scan carrying a pushed-down limit is one lane and one
batch, and each backend bounds that one read with its reader's own per-call limit.** No new
symbol, no wire change, no counter, no driver change.

### 3a. Planner — `core/planner/translator/nodes.rs`

`source()` (`:392-411`): after `let scan = survivor_metadata(parquet)?;`

```rust
    // `batching_for_source` also advances the source counter the estimator's second pass
    // pairs by, so it is called whether or not its answer is used.
    let batching = batching_for_source(t);
    let (lanes, batching) = match config.limit {
        // A limit DataFusion pushed into the scan is the whole answer wherever it erased the
        // limit node above it, and both readers bound one CALL — `with_limit` here,
        // `set_num_rows` on the device — so it is the answer only over one call: one lane and
        // one batch, whatever the mode. `GpuLoadParquet::validate_schemas_and_partitions` pins it.
        Some(_) => (1, Batching::Off),
        None => (lanes_for(t, &scan.groups), batching),
    };
    let partition_groups = partition(&scan.groups, lanes, batching)?;
```

`lanes_for` (`:374-390`) loses its `limit` parameter and its first comment block (`:375-381`);
its body becomes the small-table rule alone. It has one caller.

### 3b. Validator — `core/plan/source.rs`

In `validate_schemas_and_partitions` (`:21-50`), after the `partition_groups.is_empty()` check:

```rust
        if self.limit.is_some() {
            let batches: usize = self.partition_groups.iter().map(Vec::len).sum();
            if self.partition_groups.len() != 1 || batches != 1 {
                return Err(PlanError::Invalid(format!(
                    "{}: a limit pushed into the scan bounds one read, so its mapping is one \
                     lane and one batch — this one is {} lanes and {batches} batches; the \
                     translator's `source` plans it so",
                    self.table,
                    self.partition_groups.len()
                )));
            }
        }
```

One owner of the rule, checked on every plan `validate` sees, including the injected trees
(`tests/common/injection.rs` never re-cuts a loader's mapping — `drained` at `:558` moves whole
lanes and a limited scan has one — so no injected plan trips it).

### 3c. CPU source — `core/executor/cpu_backend/source.rs`

Struct (`:25-34`): add `limit: Option<usize>` with the doc `/// The pushed-down limit, applied by
the reader per call; the plan makes a limited scan one call.` `new` (`:67-76`): `limit:
node.limit,`. `read_next` (`:86-91`):

```rust
        let mut builder =
            ParquetRecordBatchReaderBuilder::new_with_metadata(file, self.metadata.clone())
                .with_row_groups(groups)
                .with_projection(self.projection.clone());
        // The reader's own bound, the twin of cuDF's `set_num_rows`: a row selection over the
        // groups named, so nothing past the limit is decoded.
        if let Some(limit) = self.limit {
            builder = builder.with_limit(limit);
        }
        let reader = builder
            .build()
            .map_err(|error| BackendError::new(format!("reading {}: {error}", self.file)))?;
```

Nothing else in the file moves: `concat_batches` still folds the reader's chunks into the one
batch the plan promised, `as_declared` still casts to the declared types.

### 3d. Writer guard — `core/wire/node_writer.rs`

`scan()` (`:80-101`) writes `limit: node.limit.unwrap_or(0)`, and `scan.cpp:61` reads `> 0`, so
`Some(0)` and `None` are one value (hacks-audit, "Production bugs" #2). Refuse what the wire
cannot say, at the top of `scan()`:

```rust
    if node.limit == Some(0) {
        return Err(PlanError::Unsupported(
            "a scan limit of 0 has no wire form: CudfScan.limit reads 0 as no limit".into(),
        ));
    }
```

DataFusion's `EliminateLimit` turns `LIMIT 0` into an empty relation before physical planning, so
this is a loud edge rather than a path; it costs four lines and closes the audit item honestly.
(Alternative: leave the wire alone and note the unreachability — see §4.)

### What it deliberately does not touch

- `cpp/src/operators/scan.cpp`, `core/executor/gpu_backend/source.rs`, `CudfScan.limit`, the
  recipe text, `recipe-payloads.txt`: the device already implements the per-call bound the plan
  now makes sufficient. That cuDF refuses `set_num_rows` beside `set_row_groups` is #188.
- `scan_mapping/partition.rs`: the mapping policy stays pure; the limit decision is beside the
  lane decision in `source()`.
- The driver, `GpuLimit`/`LimitStream` on either backend, the unload interval rule,
  `limit_interval`, the estimator (`memory_estimation.rs`), the exec model (which has no scan
  limit at all).

### How CPU and GPU stay one engine

The plan constrains a limited scan to one call (3a, pinned by 3b). Each backend bounds that call
with its reader's native knob — `with_limit` (3c) and `set_num_rows` (`scan.cpp:61-63`, already
there). The fbs doc `Maximum rows to read (0 = unlimited)` is the shared contract and needs no
edit. **Constraint this places on #188:** `execute_scan` must return at most `limit` rows from
the row groups it is handed in one call, by any means — not calling `set_num_rows` when a
row-group list is set and `cudf::slice`-ing the read to `limit` (stats are taken after, so the
batch is priced right), or dropping the override when it names every group of the file. No Rust
device-source change is needed under this design; under the executor-count alternative (§4) it
would be, plus a slice with the hacks-audit #1 pricing hole.

Recommended companion for #188, not part of this fix: map a limited scan's one batch to the
shortest prefix of survivors whose rows cover the limit (`[[[0]]]` for `LIMIT 10` over lineitem).
It bounds the device's decode when #188 slices after the read, and makes the estimator's number
honest (see §7).

### Pinning tests, goldens, registry, comments that change

Tests (red before, green after):
- `core/executor/cpu_backend/tests/source.rs`: the `loader` helper (`:53-78`) hardcodes
  `None`; add a `limit` argument (or a `limited_loader`) and one case,
  `a_limited_lane_reads_no_more_than_its_limit`: mapping `[[[0,1,2]]]`, limit 3 over the six-row
  fixture → `lane(&node, 0) == vec![vec![1, 2, 3]]`. Today: `[[1..6]]`.
- `core/plan/tests/mod.rs`: `loading` (`:625-647`) gains a limit; `a_limited_scan_over_two_
  batches_names_the_rule`: `[[[0], [1]]]` with `Some(5)` → `invalid(…, "one lane and one batch")`.
- `core/planner/translator/tests.rs:603`: rename to `…_plans_one_lane_and_one_batch`; add a
  `Translator::new(1, Batching::PerRowGroup)` plan of `SELECT * FROM part LIMIT 3` asserting
  `partition_groups.iter().map(Vec::len).sum::<usize>() == 1`, guarded by the unlimited
  `SELECT * FROM part` at the same batching having more than one batch (so the claim cannot be
  vacuous if `tpch.minimal/part.parquet` turns out to be one row group — pick another table then).
- `tests/test_cpu_end_to_end.rs`: one inline case through `sql_answers_match_datafusion`
  (`:~397` shows the form): `("tpch", "a limit erased into the scan", "SELECT count(*) FROM
  (SELECT * FROM nation LIMIT 3)", None, Coverage::ModesOnly)` — the count is fixed by the SQL, the
  limit lives only in the loader at every mode, and today it answers 25.
- `tests/test_cpu_end_to_end.rs:426-524` `a_limit_slices_at_most_two_batches_and_stops_the_scan`:
  its guard `most_offered > 2` (`:486-489`) fails after 3a — both of nested-limits' scans carry a
  limit and so offer one batch at every mode, which is exactly what the guard exists to catch. Keep
  the nested-limits pass for the ≤2-ranged-unloads, `satisfied` non-empty and `peak_queued ≤ 1`
  claims, and run the `pulled == 2` claim and the guard over a second, inline SQL whose mid-plan
  limit sits above a filter — `SELECT k FROM (SELECT p_partkey AS k FROM part WHERE p_size > 0
  LIMIT 40 OFFSET 5) x, region LIMIT 20 OFFSET 3`. DF 45's `FilterExec` has neither `with_fetch`
  nor `supports_limit_pushdown` (`datafusion-physical-plan-45.0.0/src/filter.rs`), so the `part`
  scan stays unlimited and multi-batch at the rowgroup modes and the driver's hold is what stops
  the second pull. Restructure the loop body into a helper taking the SQL; ~25 lines.

Goldens:
- Plan goldens (rust-only, `UPDATE_CANONICAL=1 … --test test_plan_goldens`):
  `testdata/goldens/tpch.sf1/tp1-rowgroup.plans.txt` and `tp4-rowgroup.plans.txt`, sections
  `scan-limit` (`partition_groups` 49 batches → `[[[0,1,…,48]]]`; `--- memory ---`
  `estimated_max_resident_size` 7625472 → 371110838 on the two lines) and `nested-limits`
  (part `[[[0],[1]]]` → `[[[0,1]]]`; memory 983071 → 1600062 and the lines above it). The
  single and sized modes already map one batch. tpcds carries no limited scan (grep over
  `*.plans.txt`). `recipe-payloads.txt` unchanged — `CudfScan` carries no mapping.
- Execution goldens (`test_cpu_corpus`, `UPDATE_CANONICAL=1`, scope with `PCK_TEST_FILTER`):
  `tpch.sf1/<mode>-mini.cpu.txt` — `scan-limit` at all five modes (two new sections at tp1 with
  `early_exit=none`, loader `batch_rows=[[10]]`; three rewritten at tp4: loader 6001215/122880 →
  10 rows, unload `in_rows=[[10]]`, and the unload's call is whole rather than ranged) and
  `nested-limits` at all five (part loader → `batch_rows=[[28]]`, `GpuLimit in_rows=[[28]]`);
  the sibling `.cost.txt` re-derive. `mini.result.txt`: `scan-limit` authored at `tp4-sized`
  as before and the same first ten rows of row group 0 (today the unload's range takes rows
  0..10 of the same batch), so expected byte-identical; verify.
- `tests/test_corpus_goldens.rs:285-320` (loader emits its mapping, or a prefix under an early
  exit): holds unchanged — at tp1 the mapping is one batch and one is emitted.

Declarations:
- `tests/common/corpus_cases.inc:51`: `scan_limit` cpu modes → all five; comment `:44-46`
  deleted, `:49-50` reworded so #188 stands alone ("the plan puts the bound in the scan and cuDF
  refuses row groups beside it").
- `testdata/cost-registry.csv:135`: `cpu_tp1_single,cpu_tp1_rowgroup` → `enabled`; `tickets` →
  `188`. `the_registry_matches_the_cpu_corpus_in_both_directions` holds both.

Wiki:
- `architecture.md:374-379`: true again; add the clause "… whatever the batching — `source()`
  plans it and the loader's validator refuses any other mapping". `:181` node row stands.
- `build-test.md` "Corpus, cpu" row: N 447 → 449 (grand total 1569 → 1571).
- `tasks/active-tickets.md`: #186 out (archived at merge, helper's job); #188's second paragraph
  (`:156-159`) no longer has "the CPU ignores it" to lean on — one sentence.

### Hacks-audit scaffolding

Removes none — the audit excluded #186 and found nothing grown around it. Respects and closes
its "Production bugs" #2 (`Some(0)`/`None` on the wire) via 3d. Adds no counter, so it does not
widen "Shape problems" #8 (the driver/`LimitStream` double count). Closes the "Tests" item
"`GpuLoadParquet.limit` is covered on neither backend" on the CPU side (the unit case, the inline
e2e case, the two cells); the device side waits on #188. Note for the `operator-cases` task
(board #9, `tasks/operator-cases-impl.md:1274-1286`): its planned
`row_groups_and_a_limit_together` builds a four-batch limited scan — after 3b a shape no plan may
carry; the harness does not validate, so the case measures the per-call contract (≤ 10 per call
on both) and should say so, or map one batch.

## 4. Alternatives rejected

- **Count across batches in the executors** (a `remaining` on `CpuSource` and `GpuSource`,
  `Exhausted` at zero, slice the straddler): correct at any batching and matches DataFusion's
  fetch-per-partition, but it makes `CudfScan.limit → set_num_rows` a partial bound rather than
  the answer, needs a device-side `slice_handle` with the hacks-audit #1 pricing hole, adds a
  third copy of the count-and-slice rule, and needs an exception arm in
  `a_loaders_batches_line_up_with_the_row_groups_that_made_them` for a loader that stops short
  with `early_exit=none`. Fallback if moving four plan-golden sections is unwanted.
- **Synthesize the unload's interval when the root is a limited scan** (translator `translate`):
  covers the bare-scan shape only; a limited scan under a project or a join with the limit node
  erased stays wrong. Symptom fix.
- **Make `GpuLoadParquet::row_interval()` return the limit and let the driver stop it**: the
  driver counts rows *arriving* at a consuming call (`driver/partitioned.rs:251-267`); a source
  consumes nothing, and a batch straddling the limit still needs a backend slice the driver has
  no source call for. Driver change for no gain.
- **Bound the CPU reader per call and leave the mapping**: 490 rows at tp1-rowgroup.
- **Leave 3d out and rely on `EliminateLimit`**: fine today, but the collapse is then a fact about
  a DataFusion rule nobody pins; four lines make it a refusal that names its reason.

## 5. Minimum corpus query

```sql
SELECT * FROM nation LIMIT 3;
```

tpch sf1 (or tpch.minimal). Plans at all five modes as `GpuUnload` over `GpuLoadParquet(table=
nation, partition_groups=[[[0]]], limit=3, lanes=1)` with no interval anywhere — nation is one
DataFusion partition, so `LimitPushdown` erases the `GlobalLimitExec` at tp4 as well as tp1
(`core/planner/translator/tests.rs:603` pins the tp4 shape). Nothing refuses it. CPU, every
mode: 25 rows for 3. Device: `execute_scan_rowgroups` fails in `scan.cpp:78`, cuDF's
`set_row_groups` refusing to sit beside `set_num_rows` — #188. Smaller than `scan-limit`
(lineitem, 49 row groups, five string columns); its corpus form is `tpch/scan_limit`, and the
determined-count variant for an oracle comparison is `SELECT count(*) FROM (SELECT * FROM nation
LIMIT 3)`.

## 6. Cells re-enabled

- Back on: `tpch/scan_limit` cpu × `tp1-single`, `tp1-rowgroup` (2 cases). `tpch/nested_limits`
  was never off; its ten sections change and its loaders stop over-reading.
- Stay off, behind #188: `tpch/scan_limit` gpu × all five; `tpch/nested_limits` gpu × all five
  (registry rows 131, 135). Under this design #188 is a `scan.cpp` change with no Rust device
  work.

## 7. Risks and unknowns

- `with_limit` over a zero-column projection: nested-limits' `region` scan has `projections=[]`
  and `limit=23`; arrow-rs's empty-projection path with a `RowSelection` was read, not run.
  nested-limits at every mode is the test that tells.
- `with_limit` + `with_row_groups` composition: read in parquet 54.2.1 (`arrow_reader/mod.rs:
  600-627, 840-870`), not executed. The selection is over the selected groups' rows, so the
  first `limit` rows of the first named group are what comes back — the same rows the unload's
  range takes today, which is what keeps `mini.result.txt` byte-identical; verify.
- The e2e filtered variant's DataFusion shape (`GpuLimit(GpuFilter(GpuLoadParquet))`, loader
  unlimited) rests on DF 45's `FilterExec` not taking a fetch — read from source, not planned.
- `tpch.minimal/part.parquet` row-group count for the translator test's guard — unverified;
  the guard makes a wrong pick loud.
- The `--- memory ---` estimate for a limited scan at the two rowgroup modes becomes the whole
  chunk (7.6 MB → 371 MB for scan-limit): an overestimate in the safe direction, and the
  estimator does not read the limit. The prefix mapping (§3, companion for #188) repairs it.
- Cost report: scan-limit's peacock cost drops from ~1 GB to a few KB at the tp4 modes; the
  regression gate fails on increases only.
- Behaviour on a limited scan whose lane is split by a future adaptive rebatcher (#142): the
  validator refuses the plan by name rather than answering B × limit — intended, and worth
  knowing before that task.

## 8. Complexity

**S.** Four source files, each under fifteen lines changed (`translator/nodes.rs`, `plan/source.rs`,
`cpu_backend/source.rs`, `wire/node_writer.rs`); four test files (~70 lines, the e2e restructure
the largest); two corpus declarations; no frozen surface — no ABI symbol, no fbs field, no wire
bytes, no declared-schema contract. Goldens regenerated: four plan sections at two modes
(rust-only, local) and ten execution sections with their cost siblings (corpus tier, filtered);
the result golden expected unchanged. Three wiki edits, one of which corrects a sentence that is
false today.
