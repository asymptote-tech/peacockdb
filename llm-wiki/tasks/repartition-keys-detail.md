# repartition-keys — run detail

Working notes. The spec is [`repartition-keys.md`](repartition-keys.md) (frozen); the plan the
developer works in is [`repartition-keys-impl.md`](repartition-keys-impl.md).

## Branch and PR

- Branch `ENS-repartition-keys`, forked off `ENS-pbench` at `badda3d5`.
- Task 4 of chain J, so its PR targets **`ENS-pbench`**. **PR #169**, base verified with `gh pr view 169 --json baseRefName`.
- Task 2 (`stale-cells`) is blocked and has no branch, so the chain's branch order is
  `master → ENS-duckdb-oracle → ENS-pbench → ENS-repartition-keys`. A resequence for the human at
  merge, not a conflict: disjoint lines and rows.

## Hosts, probed 2026-10-08

- **shad-gpu: down** all day — :22 and :443 both time out while DNS resolves and the host key is
  intact. **verda: unlocatable** — resolves nowhere, no `VERDA_*` credentials. This box builds and
  links the device targets against cuDF 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2` and has
  no card. Under 7 GiB free on `/`.

## Why this task is dispatched partially, and what the partial is

An analyst read the task against the device absence. Two of the spec's three verification bars are
rust-only and reachable today, so the earlier blanket block over tasks 4-9 was wrong for this one.
**Four plan tasks are device-free, in this order:**

1. **Task 2 — #189's grouping-id drop.** Planner only: `drop_grouping_id`, a planner test, three
   unit tests, a plan-golden regeneration.
2. **Task 5b — the four wire timestamp types**, minus its gtest: the fbs append,
   `convert_data_type`, `fb_text`, the `fb_to_type_id` arms compiled but not run, and
   `timestamp-s-key-group` leaving `NOT_RUNNABLE`. Both later C++ tasks consume these values.
3. **Task 5c — #249's plan-time refusal, with step 4 deleted.** See the deviation below.
4. **Task 9 step 1 — the 18 cpu cells**: tpch `rollup-over-join` and tpcds q5, q18, q22, q80
   (15), plus pbench's `rollup-small-keys` (3).

**The order matters and is not the plan's.** Task 1 — the murmur gate, #201 — is **not** rust-only,
which is the opposite of what it looks like: `cpu_backend/mod.rs:708` declares
`#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`, so every case in `murmur_conformance.rs` runs
in the device rung only, and `build-test.md` says so. #201's proof *is* the seed-mutation red-green
cycle on that gate, so rewriting the one gate that proves cpu and device agree, and landing it
unrun, would be #201 over again. Task 1 waits for a card.

## Three fences. Nothing in the partial may assert a cell that was not proven

- **No Task 10.** Archiving #95, #189, #201, #206 or #240 is the one write here that would lie —
  #189 included, since 6 cells still want comet's unsigned arm.
- **No Task 9 step 2, and no gpu cell enabled at all.**
  `every_enabled_device_cell_has_its_gpu_result_section_and_no_other` makes this self-policing: an
  enabled device cell with no recorded answer goes red.
- **No cpu-side lane-rule change** — not Task 6's decimal cast, not Task 4's NaN canonicalisation,
  not Task 6b's unsigned widening. Each is one half of a two-engine rule whose only proof is a
  device gate; landing the cpu half alone moves cpu goldens and leaves "the engines agree"
  asserted by nothing. `registry.rs` is too weak to catch it — it wants only *some* ticket on a row
  with off cells — so the guard is the dispatch, not a test.

## The deviation the partial requires

**Task 5c step 4 is deleted, not deferred.** It adds `struct-key-join`, `struct-through-join` and
`interval-through-join` to `NOT_RUNNABLE`, and those three query files do not exist — they are held
behind [#255](../tickets/complete-coverage.md#t255), the planner panicking on a Struct or Interval
column instead of refusing. `every_query_that_cannot_cross_the_wire_is_declared_and_every_declaration_is_true`
(`plan_goldens.rs:495`) asserts each declaration carries its line in all five modes, so declaring
an absent query goes red. Deleting it costs nothing measurable: no existing query puts an
unnameable type in a serialized schema, and #249's own text already assigns those declarations to
the #255 task. **The spec's Scope line about the three #249 queries is therefore false**, and the
spec is frozen with one write left, so it is recorded here and in the signoff rather than edited.

## Two things already settled, so the round need not re-derive them

- **[#253](../tickets/corpus-coverage.md#t253) is decided** — a side on `duckdb_divergent`, an
  optional ticket and column list on `duckdb_fingerprint`, with the two rejected alternatives
  named. It gates only the device half, so nothing in this partial turns on it. `uint-key-group`'s
  cpu side is settled too: its line is already `duckdb_fingerprint` with tp1 enabled and green, so
  the tp4 cpu answer is the same rows.
- **#189 reaches 18 of its 24 cells**, and the plan is already right about it — Task 9 step 1
  enables exactly those 18 and the unsigned half is Task 6b, inside this task. The ticket's fix
  paragraph was corrected to say so.

## One healthy red to expect

tpcds q14 has `rollup` in its features and its cpu tp4 cells are enabled today. If #189's drop
changes its lanes, its committed sections fail. That is a real finding, not something to clear with
`UPDATE_CANONICAL` — the plan expects only six queries to move.

## Carried forward

- **27 rust-only cases are red on the base and belong to task 1**: the 26 `duckdb_gpu_*` tpch and
  tpcds cases plus the coverage guard, on a `gpu-result.txt` no device wrote. The count must stay
  27.
- **Run the rust-only loop as plain `cargo test --features rust-only` into `./target`**, never
  through `scripts/cargo-cudf.sh`, which redirects to a cuDF target dir and would recompile the
  DataFusion stack with under 7 GiB free.
- **[#259](../tickets/corpus-coverage.md#t259)** records that pbench's device cells have never been
  run and that `int8-key-group` never landed; this task's own key-type rows are part of what that
  cycle will cover.

## Round 1 result (2026-10-08)

The device-free partial, exactly the four plan tasks the scope above names. No device ran; no
device cell was enabled; no ticket was archived. `cpu_backend/mod.rs:709` was re-read and the
scope note is right: `#[cfg(all(test, feature = "gpu"))] mod gpu_tests;`, so plan Task 1 was not
started.

### Plan Task 2 — #189's grouping-id drop (planner only)

`drop_grouping_id(keys, id)` in `planner/translator/aggregate.rs`, applied by a `let shuffle =
match shuffle` directly above `tree = match shuffle`, guarded on `!group.is_single()`. The merge
above still *groups* on `__grouping_id`; only the *hash* loses it.

Red then green, in order:

- `a_rollup_shuffle_hashes_its_keys_and_never_the_grouping_id` (`translator/tests.rs`) — red with
  `left: [0, 1, 2], right: [0, 1]`, the message the plan predicted.
- Three unit tests in a new `translator/aggregate/tests.rs` (the sanctioned `foo.rs` +
  `foo/tests.rs` shape, as `translator/expr.rs` already uses, so `drop_grouping_id` stays private):
  the id leaves and the user keys stay; a shuffle on the id alone is refused; a key above the id is
  refused. All three were watched red against a `Ok(keys)` stub before the real body went in.

### Plan Task 5b — the four wire timestamp types (minus its gtest)

`gpu_plan.fbs` appends `TimestampSecond`=20, `TimestampMillisecond`=21, `TimestampMicrosecond`=22,
`TimestampNanosecond`=23 — an append, and the generated C++ header was read back to confirm
`Decimal128` is still 19. `convert_data_type` maps `Timestamp(unit, _)` for any zone, since cuDF's
`data_type` has no zone and the values are UTC `int64`s either way. `fb_to_type_id` gained the four
arms. `fb_text` needed no change: it renders a field's type through the generated enum's `Debug`
(`fb_text.rs:232`), which the new round-trip test pins.

- `every_timestamp_unit_crosses_the_wire_with_its_unit` — red as a compile error (no variant),
  then green.
- `a_serialized_schema_carries_each_timestamp_unit_under_its_own_name` — serializes and reads back,
  asserting the rendered name. Red-green cycle verified by deleting the four `convert_data_type`
  arms and watching it fail, then restoring them.
- `a_cast_to_any_timestamp_unit_is_written_with_its_unit` (`wire/expr_writer/tests.rs`) — the
  positive half of what the old `(#240)` refusal used to assert.
- `NOT_RUNNABLE` lost `("pbench", "timestamp-s-key-group", "240")`. **The red that proved the wire
  change end to end came first:** with the arms in and the declaration still there,
  `every_published_seq_addresses_the_kind_its_recipe_claims` went red with
  `left: ["tpch mixed-join"], right: ["pbench timestamp-s-key-group", "tpch mixed-join"]`, and all
  five pbench plan goldens went red with `not runnable: …` → a real recipe tree. Only then was the
  declaration removed.

### Plan Task 5c — #249's plan-time refusal, step 4 deleted

`serialize_schema` returns `Result<_, PlanError>` and puts every field through
`convert_data_type`; the `unwrap_or(fb::DataType::Null)` is gone. Its two callers
(`node_writer::scan`, `aggregate_writer::aggregate`) already returned `Result` and gained a `?`.
`expr_writer::data_type` lost its `Timestamp(..) => "#240"` arm, so every unnamed type now cites
`(#249)`. Grepped afterwards: no `convert_data_type` call site swallows an error any more.

- `bug_a_schema_holding_an_interval_is_refused_at_plan_time` — red because `serialize_schema`
  returned a schema and had no `expect_err` to call, then green.

**Step 4 stayed deleted**, as the scope above requires, and the reason was re-verified here:
`testdata/pbench-queries/` holds no `struct-key-join.sql`, `struct-through-join.sql` or
`interval-through-join.sql`, and `plan_goldens.rs:495`'s second loop asserts
`carried == MODES.len()` per declaration, so declaring an absent query is `0 != 5`. #249's own text
(`complete-coverage.md`) says those two queries "land with that fix [#255], and are declared not
runnable on this one then". **The spec's Scope line about "pbench's two #249 queries are declared
not runnable on it" is therefore still false and still owed to the signoff.**

### Plan Task 9 step 1 — the 18 cpu cells

Six `corpus_query!` lines went from `tp1_single | tp1_rowgroup` to `all_modes` in the **cpu** column
only; every one keeps `gpu=none`. `cost-registry.csv`'s `cpu_tp4_single`, `cpu_tp4_rowgroup` and
`cpu_tp4_sized` went `disabled` → `enabled` on the same six rows. Nothing else in the CSV moved: a
column-by-column diff shows **0 rows whose gpu cells changed** and 26 enabled gpu cells before and
after. No ticket was struck from any row — `registry.rs:229-240` only wants *some* ticket on a row
with off cells, and each of these six still has its five gpu cells off.

| query | cpu before | cpu after | cells |
|---|---|---|--:|
| `tpch/rollup-over-join` | tp1-single, tp1-rowgroup | all five | +3 |
| `tpcds/q5` | tp1-single, tp1-rowgroup | all five | +3 |
| `tpcds/q18` | tp1-single, tp1-rowgroup | all five | +3 |
| `tpcds/q22` | tp1-single, tp1-rowgroup | all five | +3 |
| `tpcds/q80` | tp1-single, tp1-rowgroup | all five | +3 |
| `pbench/rollup-small-keys` | tp1-single, tp1-rowgroup | all five | +3 |

`test_cpu_corpus` went 886 → 904 cases, which is +18 and nothing else.

### Golden movement: 69 sections, 8 query names, nothing else

Counted by parsing `== <section>` headers out of every changed golden and diffing section bodies
against `HEAD`. **No section was added or removed anywhere** — every one of the 69 is a body that
changed, so nothing was pruned.

| file class | sections | why |
|---|--:|---|
| `*.plans.txt` | 26 | 21 are #189: the emit's `hash=` and `hashed_on=` lose `__grouping_id`, and the merge, project and sort above *gain* `hashed_on=[user keys]` — the distribution now renumbers through a project that does not carry the id, where before the whole annotation was lost. 7 queries × 3 tp4 modes. The other 5 are `timestamp-s-key-group` at all five modes, `not runnable` → a recipe tree. |
| `recipe-payloads.txt` | 1 | `tpcds q5` only: one `sha256=` and the `hash: channel@0, id@1, __grouping_id@2` → `hash: channel@0, id@1`. Regenerated deliberately with `PEACOCK_REWRITE_RECIPE_BYTES=1`. |
| `*-mini.cpu.txt` | 18 | all 18 went `skipped: not enabled at this mode` → a real `early_exit=…` section. Checked mechanically, not by eye. |
| `*-mini.cost.txt` | 18 | all 18 the same, skip marker → a derived cost block. |
| `mini.result.txt` | 6 | **the `mode=` stamp line and nothing else** — `mode=tp1-rowgroup` → `mode=tp4-sized`, because tp4-sized is now each query's last declared mode. Not one answer row changed, which is the evidence that the tp4 answers equal the tp1 answers already committed. |

Afterwards, **no `hash=` or `hashed_on=` list in any plan golden holds `__grouping_id`** (15+3+3
occurrences in the three tp4-single files before), and the `sorted_on` count per file is unchanged,
so nothing was lost alongside the drop.

The seven queries whose plans moved: `tpch/rollup-over-join`, `tpcds/q5`, `q18`, `q22`, `q77`,
`q80`, `pbench/rollup-small-keys`.

### The q14 red did not happen, and here is why

**tpcds q14 did not move at all** — its plan goldens are byte-identical and its enabled cpu tp4
cells stayed green. It has `rollup` in its features, but its rollup's `GpuAggregate` and the
`GpuAggregateBatches` above it are both `lanes=1` at tp4 (`tpcds.sf1/tp4-single.plans.txt`, the q14
section), so there is no hash shuffle below that Final for #189 to touch. Its other
`GpuEmitPartitions` nodes are join shuffles on ordinary keys. So the feared seventh mover is not
q14.

### The seventh query that moved is pbench's, and it was expected

The plan's Task 2 step 5 names six queries whose goldens move, and seven moved. The seventh is
`pbench/rollup-small-keys`, which **the scope above asks for by name** as part of the 18 cells —
the plan's list of six predates pbench. It is not an unintended mover: it is one of the six queries
whose cells this round turns on, and tpcds q77 is the sixth on the plan's list but is not among the
18 (its tp4 cells wait on #212, not #189, as `build-test.md` and `corpus_cases.inc` both say).

### Measured counts

| command | result |
|---|---|
| `test_cpu_corpus` | 877 passed, **27 failed**, 904 total — the 27 are byte-identical to the base's 27 (`diff` of the sorted failure lists) |
| `--lib` | 682 passed, 0 failed, 2 ignored — 676 → 684 cases (+8 new tests) |
| `test_corpus_goldens` | 26 passed |
| `test_cost_model` | 3 passed |
| `test_golden_format` | 43 passed |
| `test_module_layout` | 18 passed |
| `test_ci_coverage` | 9 passed |
| `cargo test -p cost-report` | 39 passed |
| `python3 testdata/test_duckdb_result.py` | 20 passed |
| `testdata/generate_pbench.sh --check` | `pbench.sf1 matches gen.sql` |
| device type-check | `CUDF_ROOT=… scripts/cargo-cudf.sh check -p peacockdb-core --tests --features gpu` exit 0, one pre-existing `unused import: AsArray`, no new warning |

The 27 carried reds are the 26 `duckdb_gpu_*` tpch/tpcds cases plus
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`. Unchanged in count and in
membership.

### The C++ half, compiled and not run

`cpp/src/expr.cpp`'s four `fb_to_type_id` arms compiled as part of the device check:
`expr.cpp.o` in `target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-b2c7b0edb10b4476/` has an
mtime after the source edit, and the generated `gpu_plan_generated.h` beside it carries all four
`DataType_Timestamp*` members at 20–23. **No gtest was run** — there is no card. The
`PlanExecutor.CastTimestampMicrosToSeconds` gtest of plan Task 5b step 1 is not written and is
still owed.

### Deviations

1. **Plan Task 5c step 4 deleted**, per the scope above. The spec's Scope line about the three
   #249 queries stays false and belongs in the signoff.
2. **`a_type_the_wire_cannot_name_is_refused_with_its_ticket` was rewritten, not just extended.**
   It asserted that a `Timestamp(Second)` cast is refused with `(#240)` — which Task 5b makes
   false by design, and its own doc comment said "until repartition-keys adds the timestamps". Its
   timestamp half is gone and `a_cast_to_any_timestamp_unit_is_written_with_its_unit` asserts the
   opposite. This was the only unplanned red in the round.
3. **No `65` tag added to `pbench/rollup-small-keys`'s registry row**, which plan Task 9 step 1
   mentions. Its five gpu cells are still off and its only ticket is `189`; retagging them `65` is
   a claim about why the *device* refuses them, and nothing device-side ran. Left for the device
   half. No test asks for it.
4. **`wire/expr_writer/tests.rs` was deliberately left unformatted.** `HEAD` already had 40
   rustfmt-drifted lines in it; rustfmt's proposed hunks were checked line by line against my
   added range (271–285) and none of them touch it. Formatting the file would have buried a
   15-line addition in 40 lines of unrelated reflow. Every other touched Rust file was formatted
   with `--config skip_children=true`, so rustfmt could not follow a `mod` declaration into a file
   I had not touched.
5. **`clang-format` split the four new C++ arms onto two lines each**, which does not match the
   hand-aligned one-line idiom of the switch around them. `coding-style.md` names `.clang-format`
   as the authority for changed C++ lines, so the machine format was kept.

### What the device half still owes

- **Plan Task 1 (#201)** — untouched. The murmur gate still proves the kernel against its own
  `cpu_partition_ids`, and the seed-mutation red-green cycle is its proof.
- **Plan Tasks 3, 4, 5, 6, 6b** — every kernel arm (bool, float, timestamp, decimal, unsigned) and
  **all three cpu-side lane-rule halves**: the decimal cast, the NaN canonicalisation and the
  unsigned widening. None of them landed, by the third fence. `spark_partitioning.rs` and
  `spark_hash_partition.cu` are both untouched on this branch — verified by `git diff --stat`.
- **Plan Task 5b's gtest** — `PlanExecutor.CastTimestampMicrosToSeconds`, unwritten.
- **Plan Task 7** — the operator harness cases and `synthetic::key_types`.
- **Plan Task 8** — #243's two pins.
- **Plan Task 9 step 2** — every gpu cell, and the ticket striking for `95` and `189`.
- **Plan Task 10** — the wiki and the five archivals. Nothing was archived here.
- **#189's remaining 6 of 24 cells** — `uint-key-group` and `uint-key-join` at the three tp4 cpu
  modes, which need Task 6b's unsigned widening.

### Gotcha for the next developer

A regeneration run under `UPDATE_CANONICAL=1` can report one spurious failure:
`every_query_that_cannot_cross_the_wire_is_declared_and_every_declaration_is_true` reads the plan
goldens off disk while the `plan_goldens::<dataset>_<mode>` cases are still rewriting them, so it
can read a stale file and fail. It happened once here and a plain re-run was green. It is a
regen-time read/write race between libtest threads, not a CI flake — CI never sets the variable —
so no ticket was filed, but a regen that fails only on that test should be re-run before being
believed.
