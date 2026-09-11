# #186 — review of the fix proposal

Read at master 188c23ce (the proposal's c18e063a plus documentation-only commits; no code
moved). Paths relative to `/media/data/peacockdb`; `core/` = `peacockdb-core/src/`.

## 1. Verdict

**Needs changes.** The root cause and the four code edits (3a–3d) hold against the code; two of
the four tests are wrong as specified — the inline e2e case is answered by DataFusion's
`AggregateStatistics` and refused by the translator before it reaches a loader (#158), and the
restructured call-count claim fails at `tp4-single`.

## 2. Findings

### F1 — the inline e2e case does not plan (important)

`SELECT count(*) FROM (SELECT * FROM nation LIMIT 3)` never reaches the loader on this engine.
DataFusion's `AggregateStatistics` runs second in the physical optimizer, before `LimitPushdown`
(`datafusion-physical-optimizer-45.0.0/src/optimizer.rs:86` vs `:134`), so it sees
`AggregateExec(Final) → AggregateExec(Partial) → GlobalLimitExec(0,3) → ParquetExec`. It takes
`partial.input().statistics()` (`aggregate_statistics.rs:54`), which is
`GlobalLimitExec::statistics` → `Statistics::with_fetch(input, fetch=3, skip=0)`
(`datafusion-physical-plan-45.0.0/src/limit.rs:189-197`). The project's `read_table`
(`core/lib.rs:53`) builds `ListingOptions::new(format)`, whose default is `collect_stat: true`
(`datafusion-45.0.0/src/datasource/listing/table.rs:298`), so nation's `num_rows` is
`Exact(25)`; `with_fetch` takes the `nr - skip > fetch` arm and returns `Exact(3)`
(`datafusion-common-45.0.0/src/stats.rs:395-405`). `Count::value_from_stats` answers `3` for the
`COUNT_STAR_EXPANSION` literal (`datafusion-functions-aggregate-45.0.0/src/count.rs:338-344`),
and the whole aggregate becomes `ProjectionExec(lit 3) → PlaceholderRowExec`, which
`translator/nodes.rs:166-172` refuses naming #158. So the case panics at `planner::plan` at
every mode, today and after the fix — it is not "answers 25 today", and it cannot go green.

Same defect in §5: the "determined-count variant for an oracle comparison" is this query.

**Correction.** Aggregate something the statistics cannot answer while the limit still lives
only in the scan: `SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)`. A column count
needs `column_statistics[i].null_count` to be `Exact` (`count.rs:332-337`), and `with_fetch`
blanks every column statistic (`stats.rs:413`), so the rule does not fire and the aggregate
stays; `LimitPushdown` then erases the `GlobalLimitExec` (skip 0, `ParquetExec::with_fetch`,
`limit_pushdown.rs:259-265`) and the plan is `AggregateBatches(Aggregate(LoadParquet(limit=3)))`
at every mode. nation has no NULLs, so the answer is 3 whichever rows the scan returns; today
the CPU answers 25. Red before, green after, which is what the case is for.

### F2 — the restructured call-count claim fails at `tp4-single` (important)

The proposal moves `pulled == 2` and the `most_offered > 2` guard onto
`SELECT k FROM (SELECT p_partkey AS k FROM part WHERE p_size > 0 LIMIT 40 OFFSET 5) x, region
LIMIT 20 OFFSET 3`, so that the `part` scan stays unlimited and multi-batch. Two things it does
not say:

- At `tp4-single` the unlimited `part` scan is four lanes, not one: `lanes_for` returns
  `t.target_partitions` under `Batching::Off` whatever the bytes
  (`translator/nodes.rs:386`), and `balanced_chunks` over part's two row groups leaves two lanes
  non-empty (`scan_mapping/partition.rs:47-79`). The driver runs every lane of the chosen node
  (`architecture.md`, "The scheduling rule"), so both lanes are pulled once before the limit
  above can hold anything: `pulled` is 3 (region 1 + part 2), and the assertion is red for a
  reason that has nothing to do with the fix. At the two rowgroup modes and `tp4-sized` the
  small-table rule (`SMALL_TABLE_BYTES = 5 MiB`, `planner/mod.rs:37`; part's two projected
  columns read about 2.4 MB) puts part on one lane and the claim holds.
- The plan is not `GpuLimit(GpuFilter(GpuLoadParquet))`. With `skip = 5`, the non-pushdown arm
  of `pushdown_limit_helper` re-adds a `GlobalLimitExec(5, 23)` above the fetch-carrying
  `CoalesceBatchesExec(fetch=28)` (`limit_pushdown.rs:259-263`), and `translator/nodes.rs:85-102`
  lowers both: `GpuLimit(5,23) → GpuLimit(0,28) → [GpuMergePartitions at tp4-single] → GpuFilter
  → GpuLoadParquet`. Harmless to the `peak_queued` and `UnloadRange` claims — `limit_node` finds
  the outer one — but the developer should expect two limit nodes, not one.

**Correction.** Assert the pull count per mode as "every non-empty source lane was pulled exactly
once": `pulled == Σ lanes with a non-empty mapping`, computed off the tree beside
`batches_offered`, and keep the guard as `offered > pulled` at some mode (the rowgroup modes
offer 3 and pull 2). Alternatively run the `pulled` claim only at the modes where the filtered
scan is one lane, and say why.

### F3 — 3d is a guard nothing can reach, and it ships without a test (minor)

`EliminateLimit` turns `LIMIT 0` into an `EmptyRelation` before physical planning, so
`config.limit == Some(0)` never reaches `source()` and `node.limit == Some(0)` never reaches
`wire::scan()`; the proposal says so itself. `coding-style.md` forbids defensive code for
impossible scenarios unless a test reaches it. `scan()` takes a `&GpuLoadParquet`, so a
`wire/tests.rs` case that builds one with `limit: Some(0)` and asserts the refusal is a few
lines — add it, or drop 3d and record the unreachability in hacks-audit's item rather than in
code. Either is fine; the proposal lists neither.

### F4 — the translator test's multi-row-group table is a guess (minor)

The new `PerRowGroup` case guards against `part.parquet` being one row group but does not
know. `customer` in `tpch.minimal` is already proven multi-row-group by
`a_loader_is_priced_by_the_batches_its_mapping_makes` (`memory_estimation.rs:579-598`, which
needs `per_group.resident[0] < per_lane.resident[0]`). Use `SELECT * FROM customer LIMIT 3` and
the guard becomes a formality.

### F5 — the memory estimate for a limited scan grows 48× and the decision is deferred (minor)

3a maps every survivor into the one batch, so `scan-limit`'s `estimated_max_resident_size` at
the rowgroup modes goes 7.6 MB → 371 MB (the proposal states this) while the CPU decodes ten
rows. The prefix-of-survivors mapping the proposal recommends "for #188" is six lines in the
same `source()` arm — take `scan.groups` until the cumulative `rows` reach the limit, then
`partition(prefix, 1, Off)`. Doing it here makes the plan line say what the scan reads and the
estimate honest; doing it later means the five `scan-limit` plan sections move twice (the
three single/sized modes read `[[[0..48]]]` today and would become `[[[0]]]`). Not required for
correctness — decide before the branch, not after, because it changes which goldens the task
touches and it changes what #188's "drop the override when it names every group" strategy can
rely on.

### F6 — `limit_interval` is described short by one arm (minor, no change)

§2 says the unload gets an interval "only where a `GlobalLimitExec`/`LocalLimitExec` is the
root". `common.rs:137-147` also lowers a fetch-carrying `CoalesceBatchesExec` at the root
(hacks-audit "Shape problems" #4 says the arm is dead today). Nothing in the fix depends on it;
noted so the developer is not surprised by the third arm.

## 3. Claims verified

Opened and found true:

- `CpuSource::new`/`read_next` never read `node.limit`; the builder takes `with_row_groups` +
  `with_projection` only (`cpu_backend/source.rs:36-91`).
- `GpuLoadParquet.limit` (`plan/mod.rs:893-894`), rendered `limit=N` (`plan_text/node_text.rs:85`),
  written as `limit: node.limit.unwrap_or(0)` (`wire/node_writer.rs:93`), read as `> 0` →
  `set_num_rows` per call (`scan.cpp:61-63`), `set_row_groups` at `:78`; fbs doc `0 = unlimited`
  (`gpu_plan.fbs:336-337`).
- `lanes_for` returns 1 for a limit and has one caller; `source()` calls `partition(...,
  batching_for_source(t))`; `batching_for_source` advances `next_source` and must keep being
  called (`translator/nodes.rs:360-411`; `pipeline.rs:50-58` checks the count).
- `batches_of` knows no limit (`partition.rs:81-106`); the validator checks lanes non-empty,
  batches non-empty, mapped ⊆ survivors and nothing about a limit (`plan/source.rs:21-50`);
  `planner::plan` runs `validate` on every tree (`pipeline.rs:37,73`).
- Goldens: `tp1-rowgroup`/`tp4-rowgroup` `scan-limit` map 49 batches with `limit=10`;
  `nested-limits` part is `[[[0],[1]]]` with `limit=28`, region `[[[0]]]` with `limit=23` and
  `projections=[]`; the three other modes map one batch. Memory lines 7625472/371110838 and
  983071/1600062 as stated. Only these two sections in `tpch.sf1` carry a limited scan; tpcds
  carries none; neither query is in `recipe-payloads.txt`.
- `tp4-*` plans carry `GpuUnload: skip=0, fetch=10`; `tp1-*` a bare `GpuUnload`. The
  `.cpu.txt` sections show the loader reading 6,001,215 (single/sized) or 122,880 (rowgroup)
  rows for ten, and `early_exit=GpuUnload@1`.
- Why: DF 45's `pushdown_limit_helper` non-pushdown arm erases the limit node when skip = 0 and
  the source takes the fetch (`limit_pushdown.rs:231-268`); `CoalescePartitionsExec` supports
  pushdown but has no `with_fetch` in 45, so at tp4 a `GlobalLimitExec` is re-added above it —
  which is the interval the unload gets. `FilterExec` has neither `with_fetch` nor
  `supports_limit_pushdown` in 45 (`filter.rs`, no match).
- parquet 54.2.1: `with_limit` (`arrow_reader/mod.rs:221`) composes with `with_row_groups`;
  `build()` calls `apply_range(selection, reader.num_rows(), offset, limit)` over the selected
  groups (`:626`, `:839-874`), so the first `limit` rows of the named groups come back.
  DataFusion's own opener applies the fetch the same way (`parquet/opener.rs:251-252`). The
  zero-column path is `EmptyArrayReader`, whose `read_records`/`skip_records` honour a selection
  (`array_reader/empty_array.rs`), so nested-limits' `region` scan is not at risk.
- `a_scan_carrying_a_pushed_down_limit_plans_one_lane` asserts the lane count only
  (`translator/tests.rs:603-615`), at tp4 with `Sized`, over nation.
- `a_limit_slices_at_most_two_batches_and_stops_the_scan` (`test_cpu_end_to_end.rs:426-526`):
  `NextBatch` counts batches produced, not exhaustion calls (`CallKind::SourceExhausted` is
  separate), so `pulled == 2` survives 3a and the `most_offered > 2` guard is what fails.
- `a_loaders_batches_line_up_with_the_row_groups_that_made_them` holds
  (`test_corpus_goldens.rs:286-320`): equality under `early_exit=none`, prefix otherwise.
- `corpus_cases.inc:44-51` and `cost-registry.csv:135` as stated; `test_cpu_corpus.rs:93` is
  the device-needs-cpu-cell rule; the `.result.txt` `scan-limit` section is authored at
  `tp4-sized`, which stays the last declared mode.
- The injector never re-cuts a mapping: `drained` moves whole lanes (`injection.rs:558-572`);
  `every_kind()`'s two-lane `source(Some(7))` is compared, not validated
  (`test_layout_injection.rs:34-48`), so 3b breaks no fixture.
- The estimator reads a `GpuLimit`'s interval and never a loader's limit
  (`memory_estimation.rs:192-196`); the exec model's source has no limit.
- The cost gate fails on increases only (`cost-report/src/main.rs:1195`).
- `operator-cases-impl.md:1274-1286`'s `row_groups_and_a_limit_together` builds a four-batch
  limited scan through a harness that does not validate.
- The wiki sentence at `architecture.md:374-379` is quoted accurately and is false of the two
  rowgroup modes today.

## 4. Corrected proposal

Only the sections that change.

### 3. Localized fix — pinning tests

- `tests/test_cpu_end_to_end.rs`, inline case: `("tpch", "a limit erased into the scan",
  "SELECT count(n_name) FROM (SELECT * FROM nation LIMIT 3)", None, Coverage::ModesOnly)`. A
  column count is not answerable from statistics once `with_fetch` blanks the column stats, so
  the aggregate survives and the limit lives only in the loader at every mode. 3 after, 25
  today.
- `tests/test_cpu_end_to_end.rs:426-526`: keep nested-limits for the ≤ 2 ranged unloads,
  `satisfied` non-empty and `peak_queued ≤ 1`. For the filtered variant, expect
  `GpuLimit(5,23) → GpuLimit(0,28) → [merge] → GpuFilter → GpuLoadParquet` and assert per mode
  `pulled == non-empty source lanes` (a helper beside `batches_offered` summing lanes with a
  non-empty mapping), with the guard `offered > pulled` at some mode. At `tp4-single` part is
  two non-empty lanes, so `pulled` is 3 there and 2 elsewhere.
- `core/planner/translator/tests.rs`: the `PerRowGroup` plan over `customer`, not `part`.
- 3d: add a `wire/tests.rs` case building a loader with `limit: Some(0)` and asserting the
  refusal, or drop 3d.

### 5. Minimum corpus query

`SELECT * FROM nation LIMIT 3;` stands. The oracle-comparable form is `SELECT count(n_name) FROM
(SELECT * FROM nation LIMIT 3)`; `count(*)` over it is answered by DataFusion's statistics rule
and refused at plan time (#158).

## 5. Complexity

**S**, unchanged. The code is four files under fifteen lines each; the corrections above are
to test text, and the e2e restructure grows by one helper. The regeneration set is as the
proposal lists it — four plan sections, ten execution sections with their cost siblings, the
result golden expected unchanged — unless F5's prefix mapping is taken, which adds the three
single/sized `scan-limit` sections.
