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

### Round 2, review round 1's findings

The blocking one, the important one and the three nits in code. Nothing else moved: no device ran,
no device cell was enabled, no ticket was archived, no golden changed.

**Blocking — the fbs append broke `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`.**
The finding is right, and the red is arithmetic rather than a guess: the generated header in this
box's cmake tree declares `inline const DataType (&EnumValuesDataType())[24]` (line 159), `cases`
held 20 rows, so `ASSERT_EQ(cases.size(), std::size(fb::EnumValuesDataType()))` is `20 != 24`.
The count assertion was not weakened. Four rows were added, one per unit, each `{}` — meaning a
refusal that names the type — and `cases` is 24 again.

- **What the rows assert, and why that is today's truth.** A bare `NULL::T` project reaches
  `build_column`'s literal branch (`expr.cpp:825`), which calls `build_scalar` directly rather
  than going through the AST. `build_scalar` (`expr.cpp:445-483`) has arms for Boolean, Int8-64,
  Float32-64, Utf8, LargeUtf8, Date32 and Decimal128, and a default that throws
  `"unsupported scalar type: " + fb::EnumNameDataType(sv->type())` — the same function the test's
  refusal branch searches `e.what()` for, so the name matches by construction. **Date64 is the
  precedent two rows above**: `fb_to_type_id` maps it to `TIMESTAMP_MILLISECONDS` and
  `build_scalar` has no arm, so its row is `{}` as well. Mapping a type for a column schema and
  being able to build a scalar of it are separate, and only the first landed here. That makes the
  four rows a live gate: the day `build_scalar` gains a timestamp arm they go red and get an
  answer, which is the right place to notice it.
- **Compiled, not run.** `make tests/gpu/test_plan_executor.o` in the native cmake tree at
  `target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-b2c7b0edb10b4476/out/build` (g++-12,
  `cudf_ROOT=~/data/miniforge3/envs/rapids-cuda-12.2`, Unix Makefiles, `CMAKE_HOME_DIRECTORY` this
  worktree's `cpp`). The object was deleted and rebuilt: exit 0, no warnings, against that tree's
  24-member `gpu_plan_generated.h`. There is no card, so the case was not run.
- **What the first run on a card has to confirm**: that the case passes — the count at 24, and
  each of the four `NULL::TimestampX` projects throwing — and that the throw is `build_scalar`'s
  default rather than an earlier refusal that happens to carry the name, since the assertion is a
  substring search and not an equality. Plan Task 5b's `PlanExecutor.CastTimestampMicrosToSeconds`
  is still unwritten and still owed.

**Nothing else enumerates that enum.** `EnumValuesDataType`, `EnumNamesDataType`, `MIN_DataType`
and `MAX_DataType` appear in the C++ at that one assertion and nowhere else. In Rust the only
`ENUM_VALUES` reader is `wire/tests.rs:956`, over `PlanNodeKind`, and it iterates the generated
array instead of copying it, so a new kind is covered rather than miscounted. The one other
hand-maintained count is `plan/tests/layout_injection.rs:53`'s 18, over plan node kinds, which
this branch does not touch. `serialize.rs`'s `convert_data_type` and `test_support/device_schema.rs`'s
`device_type_of` are keyed on the *arrow* enum and refuse or panic on an unmapped type, so neither
can go stale by a count.

**Important — six registry rows cited `189` for cells it no longer explains.** `registry.rs:242`'s
rule is that a cell turned off names the ticket that explains it, and after the drop neither engine
hashes the gid, so `189` explains nothing on a row whose cpu cells are all on. The reviewer is
right that deferring this to the device half is what makes it rot: `cost-report/src/main.rs:475`
resolves a registry ticket against the archive too, so an archived `189` would keep resolving.

| row | query | before | after |
|--:|---|---|---|
| 6 | `tpcds/q5` | `65 152 189` | `65 152` |
| 19 | `tpcds/q18` | `65 189 220` | `65 220` |
| 23 | `tpcds/q22` | `65 189 220` | `65 220` |
| 81 | `tpcds/q80` | `65 152 189 220` | `65 152 220` |
| 134 | `tpch/rollup_over_join` | `152 189 220` | `152 220` |
| 185 | `pbench/rollup_small_keys` | `189` | `65 206` |

Row 185 needed the substitution because dropping `189` would have left it with no ticket at all,
and both replacements were checked against the golden: `tp4-single.plans.txt`'s
`rollup-small-keys` section declares `__grouping_id:UInt8` and the case line is
`schema_validation_enabled`, which is #65's refusal; its `GpuEmitPartitions: hash=[f_k8@0, f_kb@1]`
takes `f_kb:Boolean`, which the kernel's type switch has no arm for, which is #206. On the other
five, every remaining ticket still explains an off cell: #65 the four tpcds rows (the device's
`Int32` gid against the declared `UInt8`, schema-validated), #152 q5, q80 and `rollup-over-join`
(the build handle a streamed probe erases), #220 q18, q22, q80 and `rollup-over-join` (the cpu's
several batches per join call). So none of the five needed a ticket added.

**Rows 197 and 198 keep `189`** — `pbench`'s `uint-key-group` and `uint-key-join`, whose three
`cpu_tp4_*` cells are off and whose blocker is the unsigned arm itself. That is the ticket's own
"6 are still off", and the registry now says exactly what the ticket says.

**Two ticket-file sentences this falsifies**, both markdown and so not mine to edit:

- **#65's "Corpus queries" paragraph** omits `pbench/rollup-small-keys`, which now carries `65`,
  and says each cell is "off first on #152, #189 or #220". After the retag no device cell anywhere
  rests on #189; the eight queries' device cells are held by #152, #206, #212 and #220.
- **#206's "Corpus queries" paragraph** names `pbench/float64-key-group` and `bool-key-group`.
  `rollup-small-keys` is a third: it hashes `f_kb` beside `f_k8`.

**Nits.** Process history left four comment blocks, each rewritten to today's state:
`corpus_cases.inc:78` (q22 alone is cut on the cpu now, so "two of the five" and the "was the
other … now hashes" clause go), `:196` (q5's rollup sentence goes entirely — its cpu cells are
ordinary), `:222` (q77's three tp4 cells rest on #212, and its rollup "does not meet" #189 rather
than "no longer meets" it), `:234` (q80 and q77 are one shape with different fates, which is the
standing point; the #189-versus-#212 race is not). `expr_writer.rs:203` loses the clause about the
timestamps having had a number of their own. `serialize.rs:143` names the type once — the format
is now `column {}: {why} (#249)`, since `{why}`'s `{other:?}` already carries it, and
`bug_a_schema_holding_an_interval_is_refused_at_plan_time` still proves both halves are in the
message (`why.contains("iv") && why.contains("Interval")`).

**Measured counts.** Each with a `timeout`, plain `cargo test --features rust-only` into `./target`.

| command | result |
|---|---|
| `test_cpu_corpus` | 877 passed, **27 failed**, 904 total — 26 `duckdb_gpu_*` plus `every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, the carried set in count and membership |
| `--lib` | 682 passed, 0 failed, 2 ignored |
| `test_corpus_goldens` | 26 passed |
| `test_cost_model` | 3 passed |
| `test_golden_format` | 43 passed |
| `test_module_layout` | 18 passed |
| `test_ci_coverage` | 9 passed |
| `cargo test -p cost-report` | 39 passed |
| `python3 testdata/test_duckdb_result.py` | 20 passed |
| `testdata/generate_pbench.sh --check` | `pbench.sf1 matches gen.sql` |
| device type-check | `CUDF_ROOT=… scripts/cargo-cudf.sh check -p peacockdb-core --tests --features gpu` exit 0 |
| gtest TU | `make tests/gpu/test_plan_executor.o` exit 0 from a deleted object, no warnings |

No golden moved, which is the expected shape: the diff is four comment blocks, one message format,
one registry column and one C++ case list. `rustfmt --check --config skip_children=true` is clean
on both touched Rust files, and `corpus_cases.inc` is not a rustfmt input.

**Deviations.**

1. **`git clang-format`'s reflow of the case list was declined.** Inserting into the braced
   initializer makes clang-format want all 18 pre-existing rows one-per-line; it proposed no change
   to the four rows added, which are already in its shape. `coding-style.md` says to format the
   lines changed and never whole files, and the reflow is 18 lines of unrelated churn around an
   8-line addition.
2. **The impl plan's Task 5c step 3 snippet still carries `"column {} of type {}: {why} (#249)"`**,
   which is where the duplicated type came from. The step is closed and the shipped format is the
   one above; recorded here rather than by editing a checked-off plan step.
3. **One pre-existing warning each side.** `unused import: AsArray`
   (`gpu_tests/aggregate_dimension_cases.rs:9`) under the device check, and
   `function sha_links is never used` under `cargo test -p cost-report`. Both are on `ENS-pbench`
   and neither file is in this branch's diff.

## Review round 1 (2026-10-08)

**1 blocking, 2 important, 6 nits.** The reviewer ran no project code; everything is text and
arithmetic over committed artifacts, which it says per finding.

### Blocking

1. **The fbs append breaks a device-rung gtest written to catch exactly this.**
   `cpp/tests/gpu/test_plan_executor.cpp:2124` asserts
   `cases.size() == std::size(fb::EnumValuesDataType())` — its own comment says the list is a copy
   of the enum so that it fails by count when the enum grows. `cases` holds 20; the fbs now
   declares 24. No generated header is committed and `cpp/CMakeLists.txt:135` runs `flatc` with
   `DEPENDS ${FBS_SCHEMA}`, so the very build that compiles the four new `fb_to_type_id` arms is
   the build where that count is 24: `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`
   is red. Nothing in the rust-only loop can see it — the test is `cpp/tests/gpu/`, shad-gpu only,
   and there is no Rust mirror. **This is a tripwire that fired and was not answered**, which is
   the regression rule. Fix: four rows expecting a refusal naming the type. `build_scalar`
   (`cpp/src/expr.cpp:445-483`) has no Timestamp arm and its default throws
   `"unsupported scalar type: …"`, so a `NULL::TimestampX` literal must refuse today — which makes
   those four rows a live gate on the new members rather than bookkeeping.

### Important

2. **`build-test.md:35`'s "149 of them running" is wrong and is mine** — I transcribed it from the
   round's report. The same sentence says 176 lines and 33 out entirely, and 149 + 33 = 182. It is
   **143**, unchanged by this round: all six queries already ran at the two tp1 modes, so the 18
   cells are new *cells* on lines that already ran, not new running lines. Nothing asserts the
   figure, so review is the only thing that catches it. Corrected, and the paragraph re-wrapped —
   my edit had left two orphan lines.
3. **`testdata/cost-registry.csv:185`: `pbench/rollup_small_keys`'s five off device cells now cite
   a ticket that no longer explains them.** After the drop neither engine hashes the gid, so #189
   is not what holds those cells — #65 is (the device's gid is `Int32` against the declared
   `UInt8`, and that line is schema-validated) and #206 is (`hash=[f_k8@0, f_kb@1]`, and `f_kb` is
   Boolean, for which there is no kernel arm). `registry.rs:242` asks only for *some* ticket, and
   `cost-report/src/main.rs:475` resolves a registry ticket against the archive too, so the day
   #189 is archived those five cells stay explained by a closed ticket and nothing goes red. The
   same staleness, lower stakes, on the other five rows. Retag `185` to `65 206`.

### Nits

Process history in code comments at `corpus_cases.inc:81, 197, 225, 234-238` and
`wire/expr_writer.rs:203` ("was the other, held off…", "#189 is fixed and q80 runs at every cpu
mode"), against the rule that a comment states today's state and git holds the sequence. #189 says
"24 cpu cells over seven queries" and it is eight. #189 is 35 lines, over the cap, and its "Fix
proposed" paragraph now describes code that exists and can go. The new `Rollup's shuffle keys` row
sits in `build-test.md`'s *subcomponent* group where `aggregate.rs` beside `aggregate/tests.rs` is
the page's *module unit* pattern. And `serialize.rs:143` names the type twice in one message.

### The positive half, which is most of the round

The golden movement **holds, by three independent routes**: 69 sections moved, 0 added, 0 removed,
split exactly as reported, and the six `.result.txt` sections changed one line each,
`mode=tp1-rowgroup` → `mode=tp4-sized`. The author stamp is not decoration — `corpus.rs:466`
compares body *and* `mode=` line, and only the authoritative mode writes — so the committed body
is now tp4-sized's answer and is byte-identical to tp1-rowgroup's, over a comparison
(`batches_to_sorted_str`) that makes it set equality rather than row-order luck. Independently,
each cell checks the live answer against plain DataFusion at one partition, so tp1 and tp4 both
green against the same oracle means tp4 equals tp1. And the top node's `output_rows` is identical
across all five modes for all six queries and matches each committed table's row count.

**No `__grouping_id` survives in any `hash=` or `hashed_on=` list** in any of the 15 plan goldens,
while the `group_by=` and `schema=` mentions remain — the merge still groups on it, which is the
correctness argument, and the validator permits it by subset rather than equality. **tpcds q14 is
right not to have moved**: at all three tp4 modes its rollup is `lanes=1` with no
`GpuEmitPartitions` at all. The five tpcds queries whose tp4 plans moved are exactly q5, q18, q22,
q77 and q80.

**Both held-back decisions were right.** Deleting the `NOT_RUNNABLE` step: the guard asserts
`carried == MODES.len()` per declaration, so an absent query gives 0 and goes red; and both halves
hold today, exactly one `not runnable` query across all 15 goldens and exactly that one entry in
the list. Holding #201: `gpu_tests/` holds only `mod.rs` and `murmur_conformance.rs`, so there is
no cpu-only substitute — a cpu-side murmur test would be a fourth copy of the rule, which is what
#201 is about. **The other two fences hold, verified cell by cell**: exactly 18 registry changes,
all `cpu_tp4_*` on the six named rows, no gpu cell or ticket column touched, and no lane-rule file
in the diff at all.

**`architecture.md` was already waiting for this** — its "the merge groups on keys + gid and the
shuffle still hashes the keys alone" was false on the base and is true now. And every other count
on `build-test.md` reconciles, "692 cells" recomputing exactly as 135×5 + 7×2 + 1×3.

**For the signoff:** the spec's Registry says #189's "15 cpu cells" over five queries; 18 over six
landed, `pbench/rollup-small-keys` being the sixth, which the frozen spec predates. Enabling it is
right — comet hashes Boolean as i32, so the cpu shuffle takes `f_kb` — but it is more than the
spec's paragraph says and the signoff should name it rather than let the arithmetic look wrong
later.
