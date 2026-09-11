# #189 — the shuffle cannot hash a rollup's grouping-set id

Read-only proposal at master c18e063a. Paths relative to `/media/data/peacockdb`.

## 1. Issue

At the three tp4 modes every rollup whose final aggregate sits above a shuffle refuses at run
time on the CPU:

    GpuEmitPartitions lane 0: assigning the scatter's lanes: External error: comet murmur3:
    Internal error: Unsupported data type in hasher: UInt8

The planner puts `__grouping_id` into the shuffle's key list. The plan golden shows it —
`testdata/goldens/tpch.sf1/tp4-single.plans.txt:2737`:

    GpuEmitPartitions: hash=[c_nationkey@0, c_mktsegment@1, __grouping_id@2], ... schema=[..., __grouping_id:UInt8, ...]

and the same line, with the gid last in `hash=`, for tpcds q5, q18, q22, q77 and q80 in each tp4
plan golden (6 queries × 3 tp4 goldens; the tp1 goldens have no shuffle and no such line).

The CPU scatter (`peacockdb-core/src/executor/cpu_backend/emit.rs:60` →
`cpu_backend/spark_partitioning.rs:41`) hands the key columns to comet's
`create_murmur3_hashes`, whose type dispatch (`datafusion-comet-spark-expr-0.6.0/src/hash_funcs/
utils.rs`, macro `create_hashes_internal!`) has arms for Boolean, Int8/16/32/64, Float32/64,
Timestamp, Date32/64, Utf8, LargeUtf8, Binary, LargeBinary, FixedSizeBinary, Decimal128 and
Dictionary — and no unsigned arm at all, because Spark has no unsigned types. `__grouping_id` is
`UInt8` for ≤ 8 group expressions (`datafusion-expr-45.0.0/src/logical_plan/plan.rs:3223`,
`Aggregate::grouping_id_type`; UInt16/32/64 above that). So the shuffle refuses by name.

Cells disabled (cpu, and therefore gpu): `tpch/rollup_over_join`, `tpcds/q5`, `tpcds/q80` at
`tp4-single`, `tp4-rowgroup`, `tp4-sized` — nine cpu cells. `corpus_cases.inc:97,218,251`;
`cost-registry.csv` rows 6, 81, 134 (`cpu_tp4_*` = disabled, tickets carry `189`);
`build-test.md:42` names `tpcds/q80 three by #189`. The 00-tickets row is correct as written.
q18/q22 also carry the shape but are out entirely on #163; q77 carries it but #175 refuses first
in its subtree; q14's rollup is above a one-lane input and has no shuffle.

Two things the reading adds to the ticket.

- **The wiki already states the intended rule and the code does not implement it.**
  `architecture.md:269-275` ("Grouping sets"): "the merge groups on keys + gid and the shuffle
  still hashes the keys alone. The rule that permits that is `hashKeys ⊆ group columns`". The
  validator carries the same sentence (`plan/aggregate.rs:236-239`: "a grouping-set rollup hashes
  on the keys while grouping on keys plus __grouping_id"), and the Python model's end-to-end
  rollup test does it (`scripts/exec_model/tests/test_end_to_end.py:283-286`, "Hashing the user
  keys only — the subset rule"). The translator is the one place that does the other thing. This
  is drift between page and code; the fix below is what makes the page true.
- **Hashing the gid could never have agreed across the two engines anyway.** DataFusion's gid
  is `UInt8` encoded MSB-first (`datafusion-physical-plan-45.0.0/src/aggregates/mod.rs:1273`,
  `(acc << 1) | is_null` → a two-key rollup gives 0, 1, 3); the C++ expansion materialises an
  `INT32` with bit i = position i (`cpp/src/operators/aggregate.cpp:388-392,414` → 0, 2, 3).
  That is #65. With the gid in the key list the device and the CPU hash different bytes for the
  middle sets and place the same rows in different lanes — a per-lane golden divergence that
  only the device tier could see, and it cannot see it yet (#152/#183 refuse first). Taking the
  gid out of the hash removes that latent divergence with the refusal.

## 2. Root cause

`planner/translator/aggregate.rs`:

- `shuffle_below` (`:39-67`) walks from the Final aggregate down through coalesce/repartition
  and, at a `RepartitionExec` with `Partitioning::Hash(exprs, n)`, records
  `Shuffle::ByHash { keys: hash_key_ordinals(exprs, &repartition.input().schema()), n }` (`:58`).
- The `exprs` are DataFusion's. For `FinalPartitioned` the required distribution is
  `HashPartitioned(self.group_by.input_exprs())` (`datafusion-physical-plan-45.0.0/src/aggregates/
  mod.rs:817-818`), and the Final's `group_by` is the partial's `as_final()` (`:311-329`), whose
  expressions are the partial's `output_exprs()` (`:225-243`) — the user keys at ordinals
  `0..n` **plus `Column("__grouping_id", n)`** when the group is not single. So the repartition
  hashes `[k0, …, k(n-1), __grouping_id]`, and `hash_key_ordinals` transcribes all of them.
- `aggregate_sequence` (`:242-389`) takes that `Shuffle` verbatim: `:370-371`
  `Shuffle::ByHash { keys, n } if lanes(tree) > 1 => shuffled(tree, keys, n)`. Nothing between
  the DataFusion node and `GpuEmitPartitions::new` (`plan/partition_ops.rs:108-131`) reconsiders
  the key list, although the same function already knows the gid exists and where it sits:
  `key_columns = group.expr().len() + usize::from(!group.is_single())` (`:264`) and
  `grouping_sets` (`:272-276`).
- Both engines then read the node's `hash_keys` as written: the CPU in `CpuEmitter::new`
  (`cpu_backend/emit.rs:36-44`) → `rows_per_lane` → comet's `_ =>` arm; the device in
  `wire/node_writer.rs:277-288` (`CudfRepartition.hash_exprs`) → `cpp/src/node_session.cpp:391`
  → `spark_hash_partition.cu:163-185`, whose switch has STRING, INT8/16 (cast to INT32), INT32,
  INT64 and TIMESTAMP_DAYS and would take the C++ INT32 gid — with #65's bytes.

Why the shape is otherwise right: the merge above the shuffle groups on `[keys…, gid]`
(`keys_through`, `:281-285`, `merge_body` `:334-340`), and the co-location rule a finalising
merge over N lanes needs is `hash_keys ⊆ group columns` (`plan/mod.rs:285-295`
`KeyDistribution::is_subset_of`, enforced at `plan/aggregate.rs:240-256`). Hashing the user keys
alone satisfies it: equal `(keys, gid)` implies equal `keys` implies one lane. A masked key is a
typed NULL, which every Spark hasher skips (comet `hash_array*` macros; the kernel's `is_null`
skip), so the rolled-up rows of one set land where their surviving keys say and the grand-total
row lands in `pmod(seed, N)` — the #137 shape `architecture.md:294-295` already describes.

## 3. Localized fix

**One decision in the translator: a grouping-set aggregate's shuffle hashes the user keys and not
the id.** No executor, wire, ABI or C++ change.

### `peacockdb-core/src/planner/translator/aggregate.rs`, `aggregate_sequence`

After `grouping_sets` is built (`:272-276`) and before the shuffle is consumed (`:370`), rebind
`shuffle` with the gid ordinal removed. The ordinal is `group.expr().len()` by construction of
`output_exprs()` above; it is present iff `!grouping_sets.is_empty()`.

```rust
    // DataFusion regroups the final on keys plus the id and hashes on all of them. This
    // mode hashes the user keys alone: hash_keys ⊆ group columns is all co-location
    // needs, a masked key is a typed NULL the hasher skips, and the id is an unsigned
    // type the Spark hasher has no arm for (#189).
    let shuffle = match shuffle {
        Shuffle::ByHash { mut keys, n } if !grouping_sets.is_empty() => {
            keys.retain(|key| *key as usize != group.expr().len());
            Shuffle::ByHash { keys, n }
        }
        other => other,
    };
```

Eight lines plus a four-line comment (the in-body cap). `shuffle` is a by-value parameter, so
the rebinding needs no signature change. `retain` rather than `truncate`, so a key list DataFusion
ever hands over in another order is still handled by identity of the ordinal and not by position.
The comment must not carry the ticket number once the ticket is archived — drop `(#189)` at
close, or write it without it from the start.

Optional, same site, closes hacks-audit finding 12's last bullet ("nothing checks that the keys
are below `key_columns`"): after the `retain`, refuse with `PlanError::Invalid` if any remaining
key is `>= group.expr().len()`. Four lines. Not required for the cells; listed so the developer
decides rather than rediscovers.

### What it deliberately does not touch

- `cpu_backend/spark_partitioning.rs` and `cpp/src/spark_hash_partition.cu` — no new type arm on
  either hasher, so the murmur3 conformance gate (`test_inc2_conformance.rs`) is untouched. That
  is the point: the ticket's "widen the gate" cost is what this proposal declines to pay.
- `cpp/src/operators/aggregate.cpp` — the INT32 gid and its encoding stay (#65).
- `hash_key_ordinals` (`translator/common.rs:46`) and the `nodes.rs:180-198` repartition arm —
  they transcribe DataFusion and stay total. The only DataFusion repartition that ever carries a
  gid is the one directly under a Final grouping-set aggregate, which `shuffle_below` alone
  reaches (`nodes.rs` translates repartitions under `Single`/`SinglePartitioned`, whose keys are
  the raw group expressions and never the gid).
- `GpuEmitPartitions`, its validation (`partition_ops.rs:33-63`), `CpuEmitter`, the `CudfRepartition`
  writer and reader, `gpu_plan.fbs`.
- The planner test `grouping_sets_expand_at_the_init_and_group_on_the_id_above_it`
  (`translator/tests.rs:652-690`): it asserts the merge groups on keys + gid, which stays true.

### How CPU and GPU stay one engine

Both read `GpuEmitPartitions.hash_keys` from the same node — `CpuEmitter::new` and
`wire/node_writer.rs::repartition` — and neither derives a key of its own. Changing the plan value
changes both by construction; there is no CPU-only or GPU-only branch. The device path additionally
stops depending on #65's encoding for lane placement, so the two engines can agree on rollup lanes
while that ticket stays open. The per-lane `in_rows`/`batch_rows` a device run will later be read
against are authored by the cpu tier from the same hash.

### Layout claims that move with it

`new_emit_partitions` declares `hashed_on = hash_keys`, and the final `GpuAggregateBatches`
re-derives its claim through `regrouped_key_distribution` (`plan/aggregate.rs:360-379`); with the
gid gone the claim survives every projection that keeps the user keys. So the tp4 plan goldens move
on more than the `hash=` line: for each of the six rollup sections,

- `GpuEmitPartitions`: `hash=[…]` and `hashed_on=[…]` lose the gid;
- the final `GpuAggregateBatches`: `hashed_on=[…]` loses the gid;
- the `GpuProject` that drops the gid gains `hashed_on=[keys…]` (today the dropped gid kills the
  whole claim, `plan/common.rs:163-174`);
- for q5/q80/q18/q22/q77 the `GpuSort` above that project gains the same claim (`exec_ops.rs:124`
  keeps `key_distribution`); `GpuMergeSortedPartitions` resets it and prints nothing new.

Three to four lines per section, six sections, three tp4 goldens per bench. The `--- recipes ---`
lines do not print keys and the `--- memory ---` section reads no hash keys
(`memory_estimation.rs` has no emit arm), so neither moves. The tp1 goldens are untouched.

### Pinning tests, goldens, registry rows and comments that change

- **New planner test**, `planner/translator/tests.rs`, red before the fix:
  `a_rollup_shuffles_on_its_keys_and_not_on_the_grouping_id` — plan
  `SELECT c_nationkey, c_mktsegment, count(*) FROM customer GROUP BY ROLLUP(c_nationkey,
  c_mktsegment)` via `translated_at_tp4(…, 0)` (small-table threshold 0, so customer's two row
  groups give four lanes and a shuffle, as the neighbouring test already relies on), find the
  `EmitPartitions`, assert `emit.hash_keys == vec![0, 1]` (today `[0, 1, 2]`), assert the final
  `AggregateBatches` layout is `ByHash { hash_keys: vec![0, 1] }` and its `group_by.len() == 3`,
  then `validate_all`. Execution-level proof is the nine corpus cells below, which are the
  regression test the hacks-audit asked for ("the only assertion anywhere is about the plan
  text" is the failure mode to avoid).
- **Plan goldens**: `testdata/goldens/{tpch,tpcds}.sf1/tp4-{single,rowgroup,sized}.plans.txt`
  regenerated (`UPDATE_CANONICAL=1 … --test test_plan_goldens`), diff confined to the six rollup
  sections as listed above.
- **`testdata/goldens/recipe-payloads.txt`**: the tpcds q5 section's `#92 CudfRepartition{Hash,
  1→4}` payload (`:3341-3343`, `hash: channel@0, id@1, __grouping_id@2`) loses one ordinal and its
  sha256 changes. This is a deliberate content change on an existing field, not a format change;
  regenerate with `PEACOCK_REWRITE_RECIPE_BYTES=1` and the fixed `/tmp` symlink build-test.md
  requires, and say so in the commit — the golden's rule is that a regen must be argued, and this
  is the argument.
- **`peacockdb-core/tests/common/corpus_cases.inc`**: lines 97, 218, 251 gain
  `| tp4_single | tp4_rowgroup | tp4_sized` in the cpu list (gpu stays `none`). Comments at
  `:89-96` (rollup-over-join), `:210-216` (q5), `:234-238` (q77's "whether its rollup would also
  meet #189 … is unknown"), `:244-248` (q80) are rewritten so no line says a cell is off on #189;
  the q80 paragraph's point — that #175 and #189 were two candidates and the query decided —
  becomes history and goes.
- **`testdata/cost-registry.csv`** rows 6 (q5), 81 (q80), 134 (rollup_over_join): the three
  `cpu_tp4_*` cells `disabled → enabled`; `189` leaves the tickets column. The registry ↔ corpus
  test (`test_cpu_corpus.rs`) checks both directions, so the CSV and the `.inc` move together.
- **Execution goldens**: the nine `skipped: not enabled at this mode` sections (three per tp4
  `<mode>-mini.cpu.txt` in `tpcds.sf1/` for q5 and q80, one per tp4 file in `tpch.sf1/` for
  rollup-over-join) become real sections when the corpus cpu tier runs with `PCK_UPDATE_SECTIONS=1`,
  and their `.cost.txt` siblings follow. `mini.result.txt` in both benches: the three queries'
  `mode=` line moves from `tp1-rowgroup` to `tp4-sized` (last enabled mode authors the result);
  rows are compared sorted and q5/q80's `ORDER BY channel, id LIMIT 100` has a unique key across
  sets, so the rows themselves should not move. The cost-regression gate omits sections with no
  base total (`cost-report/src/main.rs:1211-1222`), so the nine new sections cannot fail it.
- **`llm-wiki/build-test.md:42`**: drop "`tpcds/q80` three by #189"; the row's N rises by 9
  (447 → 456) and the grand total at `:7` with it. The row's "37 queries" and "thirteen queries
  out on #163" are already stale against the `.inc` (00-tickets notes it) and can be corrected in
  the same edit.
- **`llm-wiki/tasks/active-tickets.md:223-241`**: #189 moves to `archive/archived-tickets.md`
  with the resolution; #190's cross-reference at `:221` ("as with #189") retargets to the archive
  anchor.
- **`llm-wiki/architecture.md`**: no sentence changes — `:273` becomes true. Report the drift in
  the signoff rather than editing.
- **Recommended companion, not required**: `scripts/exec_model/tests/plan_helpers.py:121`
  `aggregate_by` emits on `grouped` (keys + gid) where `test_end_to_end.py:286` emits on `keys`;
  change the argument to `list(keys)` so the model carries the rule the planner now does. One
  token; the affected plans (q5, q14, q80 in `plans_tpcds_channels.py`) run only under the manual
  `exec-model-corpus.yml`, and their pandas oracle compares are lane-independent.

### Hacks-audit scaffolding

#189 was outside the audit's search (`hacks-audit.md:8-9`), and the reading found no production
branch, field, flag or mock knob that exists because of it: the scaffolding is the three
mode lists, the four `.inc` comments, the three registry rows and the build-test sentence above,
all of which the fix removes. One audit item sits on the same lines and is respected rather than
fought: finding 12's bullet on `translator/aggregate.rs:59` ("nothing checks that the keys are
below `key_columns`"), optionally closed above. The validator's subset rule
(`plan/aggregate.rs:240-256`) is what the fix satisfies, not a check to loosen.
Two frozen-or-active specs list #189 as a refusal the device walk should learn to *observe*
(`tasks/walk-drives-every-plan.md:126`, `-impl.md:106`; `tasks/declared-schemas-derived.md:99`).
After the fix there is no such refusal; those lists shrink to #45/#95/#55, and the spec is the
human's to amend — flag it, do not edit it from a developer dispatch.

## 4. Alternatives rejected

- **Add a `UInt8 → Int32` cast arm to `spark_partitioning.rs::hash_keys`** (beside the
  `Utf8View → Utf8` one). One line on the CPU, but it invents a Spark-murmur3 definition for a type
  Spark does not have, the device kernel has no UINT8 arm and would need one plus the conformance
  gate widened (the ticket's own cost), and it would still place rollup rows in different lanes on
  the two engines while #65 stands. Fixes the symptom, keeps the divergence.
- **Filter the gid by name (`Aggregate::INTERNAL_GROUPING_ID`) in `shuffle_below` or
  `hash_key_ordinals`.** Works today; keys off a string a DataFusion upgrade may rename, and puts
  a grouping-set rule in a function that knows nothing about grouping sets. The ordinal at the
  sequence is derived from the same fact that places the column.
- **Make the C++ gid `UInt8` with DataFusion's encoding and hash it on both sides.** That is #65
  plus a kernel arm plus the gate; right for #65, not needed for these cells, XL against S.
- **Refuse unsigned hash keys at plan time in `GpuEmitPartitions::validate_schemas_and_partitions`.**
  Turns a run-time refusal into a plan-time one and re-enables nothing; it is #95's family (a key
  type the shuffle cannot take) and belongs there if anywhere.
- **Group on the user keys alone and drop the gid everywhere.** Wrong: a placeholder NULL and a
  natural NULL collide, which is the reason the id exists.

## 5. Minimum corpus query

```sql
SELECT l_returnflag, sum(l_quantity) FROM lineitem GROUP BY ROLLUP(l_returnflag);
```

Against `testdata/tpch.sf1`, at `tp4-single` (also `tp4-rowgroup`, `tp4-sized`), CPU backend. The
two projected columns read 5,310,789 uncompressed bytes (`aggregate-groupby`'s golden records the
same scan), just over `SMALL_TABLE_BYTES` = 5 MiB (`planner/mod.rs:37`), so lineitem plans four
lanes and DataFusion puts a `RepartitionExec(Hash([l_returnflag, __grouping_id], 4))` under the
Final. A join is not needed; `rollup-over-join.sql` uses one to get a *hashed input*, which is a
different shape. `count(*)` alone would not do — `l_returnflag` alone is 764 KB and plans one lane.

Today: plans and validates (the shape is
`Unload(Project(AggregateBatches(EmitPartitions(CoalesceAllBatches(MergePartitions(AggregateBatches(
Aggregate(LoadParquet))))))))` with `hash=[l_returnflag@0, __grouping_id@1]`), and refuses at run
time in `CpuEmitter::emit` (`cpu_backend/emit.rs:60`) with the comet message above. Expected answer:
four rows — A, N, R and the grand total. At the two tp1 modes it runs, since there is no shuffle.
On a device the same plan would reach `spark_partition_ids` with an INT32 gid and hash it; not
reachable today because the cpu tier authors first.

## 6. Cells re-enabled

Back on (cpu): `tpch/rollup_over_join`, `tpcds/q5`, `tpcds/q80` × `tp4-single`, `tp4-rowgroup`,
`tp4-sized` — nine cells; registry rows 6, 81, 134 `cpu_tp4_*` → enabled, `189` removed.

Stay off, behind another ticket: the gpu cells of all three (#152 build-side copy, #183 Utf8View
export — the rollup's `id`/`c_mktsegment` are strings); `tpcds/q77` × tp4 (#175 refuses in its
subtree before the shuffle; its `.inc` comment should stop naming #189 as a second candidate);
`tpcds/q18`, `q22` (#163, whole query); `tpcds/q14`, `q67`, `q70`, `q86` (#163 / window / #23; q14
has no gid shuffle anyway). #65 stays open and unchanged, and stops being on the hash path.

## 7. Risks and unknowns

- **Another blocker behind #189 at tp4 for q5/q80.** Not runnable here. The emit's input is a
  `GpuCoalesceAllBatches` that emits at done, so the scatter received its one batch only after the
  whole subtree — q80's outer joins included — had completed; #175 therefore did not fire below
  it, and what remains after the fix is the final merge (`sum` over decimal state, grouped, never
  empty), the sort and the limit. Expected to pass; unverified.
- **Exact golden line counts** are read from the goldens rather than produced; `hashed_on` gained
  by the project/sort above the final aggregate is derived from `rebase_through_projection` and
  `new_sort`, not observed.
- **The q5 payload digest** must be regenerated under the byte-rewrite variable with the fixed
  `/tmp` root; a regen without it goes red by design and is the signal to do it deliberately.
- **`mini.result.txt` rows**: assumed identical under the author change to `tp4-sized`; if any
  rollup output had a tied `LIMIT` this would not hold — q5/q80 sort on `(channel, id)`, unique
  across sets, and rollup-over-join has no limit.
- **Unsigned parquet columns as hash keys** remain a run-time refusal on the CPU and a `CUDF_FAIL`
  on the device. No corpus schema has one; not this ticket.
- **DataFusion's `SinglePartitioned` grouping-set aggregate** was reasoned about, not planned: its
  repartition hashes `input_exprs()` (raw keys), so the `nodes.rs` arm should never see a gid. If
  a future DataFusion puts a gid-carrying repartition anywhere `shuffle_below` does not reach, the
  refusal returns with the same message and the same one-line answer.

## 8. Complexity

**S.** One function in one planner file (~12 lines), one new planner unit test, three `.inc`
lines and four comments, three CSV rows, one build-test sentence, ticket archival; plus golden
regeneration that is wide in files but narrow in content — six tp4 plan goldens (six sections
each, 3–4 lines per section), one payload section and digest, nine execution sections authored
where `skipped:` sat, their cost siblings, and two result `mode=` lines. No C ABI, FlatBuffers
schema, wire-format or declared-schema change; the wire *content* of one existing field shrinks,
which is why the payload golden regenerates. No C++ touched, so no device build is needed to prove
it, but the cpu corpus tier must run at the three tp4 modes for the three queries to author the
sections.
