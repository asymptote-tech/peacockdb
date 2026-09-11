# #189 review — the shuffle cannot hash a rollup's grouping-set id

Read-only against master c18e063a (HEAD 188c23ce at the time of reading; no code differs on
the paths cited). Paths relative to `/media/data/peacockdb`.

## 1. Verdict

**Sound.** The root cause is right and complete, the fix is one decision in the one function
that already knows where the gid sits, every frozen surface it says it leaves alone it does leave
alone, and no enabled cell, pinning test or golden other than the ones it lists moves. Six minor
findings, none of which changes the shape of the fix.

## 2. Findings

1. **`grouping_sets` is moved before the site the proposal names as the latest legal one.**
   The proposal says to put the rebinding "after `grouping_sets` is built (`:272-276`) and before
   the shuffle is consumed (`:370`)". `grouping_sets` is moved into an `AggregateBody` at
   `planner/translator/aggregate.rs:304` (the single-node shortcut) and `:326` (the init
   node), so a rebinding placed anywhere after `:326` — e.g. right above the `tree = match
   shuffle` at `:370`, the natural spot — fails to compile on `grouping_sets.is_empty()`.
   Severity: minor (rustc says so in the first minute). Correction: guard on
   `!group.is_single()` instead, which is the same fact (`:272-276` derives `grouping_sets`
   from it) and is a borrow of `partial` that never moves; then the rebinding can sit
   directly above `:370` where the reader expects it.

2. **Line cites off by one.** `tp4-single.plans.txt:2737` is the final `GpuAggregateBatches`
   line; the `GpuEmitPartitions: hash=[…, __grouping_id@2]` line is `:2738`. The `.inc`
   comment ranges are `:234-237` (q77) and `:244-247` (q80), not `:234-238` / `:244-248`
   (`:238` and `:248` are `corpus_query!` lines). Severity: minor. Correction: as stated.

3. **Section count wording.** "six sections each" per tp4 golden reads as six per file; it is
   one section in each `tpch.sf1/tp4-*.plans.txt` and five in each `tpcds.sf1/tp4-*.plans.txt`
   (q18, q22, q5, q77, q80), 18 sections over the six files. Likewise "three per tp4
   `<mode>-mini.cpu.txt` in `tpcds.sf1/` for q5 and q80" is two per file. Counted:
   `grep -c "GpuEmitPartitions: hash=.*__grouping_id"` is 1 and 5; the `skipped:` sections are
   2+2+2 (tpcds) and 1+1+1 (tpch), nine total as the proposal says. Severity: minor.

4. **The `SinglePartitioned` risk is closable as a fact, not a reasoning.** The proposal lists
   "DataFusion's `SinglePartitioned` grouping-set aggregate was reasoned about, not planned".
   Read: `datafusion-45.0.0/src/physical_planner.rs:725-750` emits only `Partial` plus
   `Final`/`FinalPartitioned`; `Single`/`SinglePartitioned` arise only from
   `datafusion-physical-optimizer-45.0.0/src/combine_partial_final_agg.rs:132`, whose
   `can_combine` requires `input_group_by.groups() == final_group_by.groups()`, and
   `as_final()` (`datafusion-physical-plan-45.0.0/src/aggregates/mod.rs:296-310`) always
   builds `groups: vec![vec![false; n+1]]` against the partial's k masks — so a grouping-set
   pair is never combined. The `nodes.rs:180-198` arm cannot see a gid on this DataFusion.
   Severity: minor. Correction: move the item from "Risks and unknowns" to the fix's
   justification and drop the "if a future DataFusion…" clause, which is defensive prose.

5. **q5's registry row carries `152` and not `183`.** Section 6 says the gpu cells of "all
   three" stay off on #152 and #183; `cost-registry.csv:6` (q5) lists `65 97 152 189`, and `#97`
   is archived-stale. After removing `189` the row still names a live ticket for its disabled
   cells (`registry.rs:274-285` only requires one), so nothing fails; the sentence is just
   inexact. Severity: minor. Correction: "q5 on #152; q80 and rollup_over_join on #152/#183".

6. **Two first-hour surprises not named.** (a) The cost-report widget reads each query at its
   last enabled cpu mode (`cost-report/src/main.rs:565-580`), so these three queries' ratio
   moves from the tp1-rowgroup section to the tp4-sized one, which has more nodes (merge,
   coalesce, emit) and a larger Σ `output_bytes`; the PR comment may show them redder. Not a
   gate — `cost_diff` (`:1215-1222`) omits a label with no base total, and a `skipped:` section
   has no `peacockdb_cost=` footer (`entry_total`, `:1335`). (b) `architecture.md:294-295` ("those
   rows hash on nothing and land in the single lane `pmod(seed, N)`") is also false today —
   with the gid hashed the grand-total row lands on `hash(gid)` — and becomes true with `:273`.
   The signoff's drift note should name both. Severity: minor.

Nothing blocking, nothing important. Specifically checked and found not to apply:

- **#180** (a shuffled `count(*)` merging to nullable): `rollup_over_join` is a grouped
  `count(*)`. A lane the hash misses reaches `cpu_backend/accumulate.rs:372` with `grouped`
  set and answers nothing rather than a NULL identity row; the state merge `sum(count(*))` over
  ≥1 non-null rows is non-null. q96/q88 are keyless, which is the shape #180 names.
- **Empty lanes** after hashing fewer columns: the CPU emitter builds a typed empty batch
  (`cpu_backend/emit.rs:74`), the driver drops it (`driver/partitioned.rs:383`), and a grouped
  final merge, a per-batch project and an unload all cope; this is the same shape
  `aggregate-groupby` (3 groups into 4 lanes) already runs at every tp4 mode.
- **Claim truth after the fix**: the final merge's `hashed_on=[keys…]` and the project's and
  sort's derived copies are true, because the masked keys are already typed NULLs at the
  emit's input (DataFusion's partial evaluates `null_expr` on the CPU; C++ uses
  `null_placeholders`), and both hashers skip NULLs (comet `hash_array*` macros;
  `spark_hash_partition.cu:76,93`) — re-hashing an output row lands it where it is.
- **No downstream consumer acts on the new claim**: in all six rollup sections the rollup is
  the root's subtree top (Unload > [MergeSorted > Sort >] Project > AggregateBatches > Emit),
  so `co_partitioned` and the subset rule never read it; only text moves.
- **No enabled execution cell has a gid-hashing emit**: the three carriers are off at tp4,
  q18/q22/tpcds q14 are out entirely, q77 is off at tp4 on #175, tpch q14/q18 are not rollups.
- **Other readers of `hash_keys`**: `wire/node_writer.rs:277`, `plan_text/node_text.rs:171`,
  `cpu_backend/emit.rs:36`, `translator/common.rs:37` — none derives a key; `memory_estimation.rs`
  reads only `layout.n`, so `--- memory ---` does not move.
- **`test_gpu_recipe_walk`'s ROLLUP** runs at `ONE_LANE` (`:640-641`, `:749`); no repartition.
  `executor_cases.inc` has no scatter over a gid. `test_cpu_end_to_end` has no rollup.

## 3. Claims verified

- `shuffle_below` `:39-67`, the `ByHash` record at `:58`; `aggregate_sequence` `:242-389`;
  `key_columns` at `:264`; `grouping_sets` at `:272-276`; the shuffle consumed at `:370-371`;
  `shuffle` is a by-value parameter.
- DataFusion 45: `output_exprs()` appends `Column(INTERNAL_GROUPING_ID, expr.len())` when not
  single (`aggregates/mod.rs:225-243`); `as_final()` uses it (`:296-310`);
  `required_input_distribution` for `FinalPartitioned` is `HashPartitioned(group_by.input_exprs())`
  (`:817-818`); `grouping_id_type` is `UInt8` ≤ 8 keys (`datafusion-expr/…/plan.rs:3223`); the
  gid fold is `(acc << 1) | is_null`, MSB-first (`:1273`), so a two-key rollup is 0, 1, 3.
- comet 0.6.0 `create_hashes_internal!` (`hash_funcs/utils.rs:163-372`): Boolean, Int8..Int64,
  Float32/64, four Timestamp units, Date32/64, Utf8/LargeUtf8, Binary/LargeBinary/FixedSizeBinary,
  Decimal128, Dictionary; the `_` arm is "Unsupported data type in hasher"; every primitive
  macro skips nulls.
- CPU path: `CpuEmitter::new` `:36-44` builds `Column` per ordinal; `emit` `:60-62` →
  `rows_per_lane` → comet; `hash_keys` `:56-70` casts only the view types.
- Device path: `node_writer.rs:270-296` writes `hash_exprs` as column refs; `node_session.cpp:389-397`
  reads them; `spark_hash_partition.cu:143-185` normalises INT8/16→INT32 and TIMESTAMP_DAYS,
  then dispatches STRING/INT32/INT64 and `CUDF_FAIL`s otherwise; `aggregate.cpp:384-392,414`
  materialises an INT32 gid with bit i = masked position i (0, 2, 3).
- Validator: `plan/aggregate.rs:236-256` subset rule with the rollup sentence in its comment;
  `KeyDistribution::is_subset_of` `plan/mod.rs:286-294`; `regrouped_key_distribution`
  `:360-381`; `new_aggregate_batches` `:466-481`; `rebase_through_projection`
  `plan/common.rs:148-198` drops the whole claim when one hashed column is projected away;
  `new_sort` keeps `key_distribution` (`exec_ops.rs:129-135`); `new_merge_sorted_partitions`
  resets it (`partition_ops.rs:134-141`).
- `architecture.md:269-275` states the keys-alone rule; the sentence entered the page at
  7c1d427d (2026-09-08) from the exec-model design (15147012); the translator never
  implemented it. `test_end_to_end.py:283-286` emits on `keys`; `plan_helpers.py:97,121` emits
  on `grouped`.
- Goldens: `tp4-single.plans.txt:2738` and the five tpcds emits at `:2687, 3338, 8671, 14962,
  15968` carry the gid in `hash=`; the init `GpuAggregate` under each has no `hashed_on`; the
  project above each final merge has none today. `recipe-payloads.txt:3340-3343` is q5's
  `#92 CudfRepartition{Hash, 1→4}` with `hash: channel@0, id@1, __grouping_id@2` under
  `sha256=efc8a721…`. The `--- recipes ---` lines print no keys.
- Corpus: `.inc:97,218,251` are the three lines; the four comment blocks are where stated
  (modulo finding 2). `cost-registry.csv:6,81,134` as stated. `build-test.md:42` names only
  q80 on #189; N=447 is 444 enabled cpu cells + 3 property tests, so 456 after. `mini.result.txt`
  sections for all three carry `mode=tp1-rowgroup`; the result section is written only by
  `authoritative_mode` (`common/corpus.rs:380,394-407`).
- Minimum query: `SMALL_TABLE_BYTES = 5 MiB` (`planner/mod.rs:37`); `aggregate-groupby`'s golden
  records `source_bytes=5310789` for exactly `l_quantity, l_returnflag` and plans four lanes with
  a shuffle (`tp4-single.plans.txt:1-9,28`), so `ROLLUP(l_returnflag)` over the same two columns
  reaches `CpuEmitter::emit`. The "764 KB for `l_returnflag` alone" figure was not verified
  and is not load-bearing.
- The existing planner test `:652-690` asserts only the init's masks and the merge's
  `group_by` (keys + gid) and stays green. `GpuEmitPartitions.hash_keys` and
  `GpuAggregateBatches.body` are `pub`, so the proposed test can read them as the
  neighbouring test at `:436-455` does.
- `walk-drives-every-plan.md:126`, `-impl.md:106`, `declared-schemas-derived.md:99`,
  `active-tickets.md:221` reference #189 as stated. No other wiki page, test or source names
  the number.

## 4. Corrected proposal

Not needed — verdict is sound. One wording change to section 3 for finding 1: guard the
rebinding on `!group.is_single()` and place it directly above `tree = match shuffle` (`:370`).

## 5. Complexity

**S**, agreeing. ~12 lines in one planner file, one unit test, nine corpus cells and their
registry/`.inc`/wiki bookkeeping; the golden churn is mechanical and confined to the sections
listed. The only cost above a typical S is running tpcds q5 and q80 on the CPU at three tp4
modes to author nine execution sections, which is minutes, not a device build.
