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
- **[#259](../archive/archived-tickets.md#t259)** records that pbench's device cells have never been
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

## Blocked at reviewing (2026-10-08)

The device-free half landed, was reviewed and is answered: plan Tasks 2, 5b (minus its gtest), 5c
(minus the deleted step 4) and 9 step 1, on PR #169 against `ENS-pbench`. Review round 1's one
blocking finding and both importants are closed.

**The task stops here rather than going to `completing`, because it is half built and not finished
with shortcuts.** A completeness signoff now would assert that the task was solved under its
constraints, and it was not attempted under them: shad-gpu has been off the network all day and
`verda` resolves nowhere, so the spec's device bar was never in scope for this run. The state is
`blocked(reviewing)`, and only the human or the helper clears it.

### What the device half still owes

- **Plan Task 1 — #201's murmur gate**, which is the whole of that ticket. Its module is declared
  `#[cfg(all(test, feature = "gpu"))]`, so the gate runs on the device rung alone and its proof is
  the seed-mutation red-green cycle on it. Nothing here substitutes.
- **Plan Tasks 3, 4, 6, 6b and 7** — the float, boolean, timestamp and decimal keys on the device,
  the NaN canonicalisation, the decimal cast, and the unsigned widening. Each is one half of a
  two-engine rule whose only proof is a device gate, which is why none of their cpu halves is in
  this branch.
- **Plan Task 5b's gtest** — `PlanExecutor.CastTimestampMicrosToSeconds` is unwritten and still
  owed; and the four rows added to `cpp/tests/gpu/test_plan_executor.cpp`'s literal matrix were
  compiled here and never run. The first run on a card must confirm the count is 24, that each of
  the four `NULL::TimestampX` projects throws, and that the throw is `build_scalar`'s default
  rather than an earlier refusal that happens to carry the type name — the assertion is a substring
  search.
- **Plan Task 8 and Task 9 step 2** — every gpu cell, none of which this round enabled.
- **Plan Task 10** — the archival of #95, #189, #201, #206 and #240, which must not happen until
  the cells those tickets hold off are actually run. #189 in particular still holds 6 cpu cells,
  the `uint-key-group` and `uint-key-join` ones that want comet's unsigned arm.

### One thing the device half must not forget

`pbench/rollup-small-keys`' device cells now cite #65 and #206 rather than #189, and rows 197 and
198 keep #189 for the unsigned half. A device cycle that turns a cell on must drop the tag that
held it, and a cell that fails must carry the ticket it actually failed on — the registry rule asks
only for *some* ticket, so it will not catch a tag that has stopped explaining anything.

## Rebased onto the finished ENS-pbench (2026-10-08)

pbench reached `done` first, per "work one branch to completion before you rebase the next", and
this branch then moved onto it. Seven commits replayed with
`git rebase --onto ENS-pbench badda3d5 ENS-repartition-keys`. **`--onto` and the old fork point are
not optional**: `badda3d5` is an ancestor of this branch and not of the rebased ENS-pbench, so a
plain `git rebase ENS-pbench` would replay pbench's own rewritten commits and invent conflicts.
Verified both ways before running it. The pre-rebase tip was `9b52b6a0`, kept here in case anyone
needs to diff against it.

**This rebase is not documentation-only and the state does not come back on its own.** It carries
pbench's device cycle: 30 newly enabled gpu cells, a new `gpu-result.txt`, a regenerated
`dim.parquet` with two more columns, all 17 other pbench goldens, and `int8-key-group`. The board
therefore still says `rebase needed(building)`, and a developer re-runs the proving commands before
the parenthesised state is restored.

### Every conflict, and how each was resolved

Nothing conflicted in Rust or C++. Six files, and two of them needed deciding rather than picking.

- **`llm-wiki/tasks/tasks.md`, four times** — the state line. Resolved by the override rather than
  the ownership rule, which is the standing note for this chain: the rule would give the child's
  side everything from `building` to `done` and so keep the stale state while silently dropping the
  `rebase needed` mark. HEAD's side every time, which carries the mark and the real PR number.
- **`testdata/cost-registry.csv`, twice, both the `rollup_small_keys` row** — resolved by intent,
  not by side, after reading each replayed commit's own diff. The first time: this branch's cpu
  columns (all five on, its #189 fix) with **pbench's measured `65` kept in the ticket column**.
  The second time: `29dd7a10`'s own side, which already carried `65` and retired the now-stale
  `189` for `206`. Taking either side wholesale would have dropped one run's measurement.
- **`peacockdb-core/tests/common/corpus_cases.inc`** — the `sparse_probe_*` and `timestamp_*` block.
  **HEAD's gpu columns are the cycle's measurements and had to survive**: `sparse-probe-left` and
  `-semi` at every mode, the three `timestamp-*-key-group` at the two tp1 modes. The replayed side
  had them all at `none`, which is what they were before any device ran. From this branch's side
  only its deliberate edit was taken: the stale `// device: not runnable, #240` comment goes, since
  this branch is what gave the wire its timestamp types.
- **`llm-wiki/tickets/corpus-coverage.md`, #65's corpus paragraph** — both sides were right about
  different things. HEAD's measurement stands (`rollup-small-keys` does reach #65 on a device, its
  two tp1 cells off on the width), and this branch's correction stands too (#189 no longer explains
  any device cell, the shuffle having stopped hashing the id). Merged, in the trimmed form the cap
  needs, with the three tp4 cells now named on #206.
- **`llm-wiki/tasks/repartition-keys-impl.md`, Task 5c Step 4** — worth recording, because the two
  sides had independently found the same defect. pbench's completeness analyst found that the step
  cannot execute (its three `.sql` files do not exist until #255 closes, and the `NOT_RUNNABLE`
  guard is bidirectional, so it goes red on contact) and the coordinator deferred it on ENS-pbench.
  This branch's own run had already found it and gone further, marking it `[~] DELETED, not
  deferred`. This branch's decision and its checkbox states win, with the other side's file and line
  citations folded in.
- **`llm-wiki/build-test.md`, five hunks across three commits** — all counts, and both sides stale
  against the merged tree. Taken to HEAD through the replay and **re-derived once at the end**, from
  the declarations rather than by adding deltas. Re-deriving seven times would have been waste and
  six of the seven answers would have been thrown away.

### The counts, re-derived from the merged tree

Mechanically, off `corpus_cases.inc` and `cost-registry.csv`: **177 active lines, 697 cpu cells, 56
gpu cells**, 136 lines at all five cpu modes, 8 partial, 33 fully out, oracles 110/16/14/33/4. The
cpu cells are pbench's 679 plus the 18 that #189's drop turns on. **Both directions agree** — the
registry's enabled counts equal the declarations' at 697 and 56 — and no off gpu cell with a live
cpu twin lacks a ticket, which is the rule a bad resolution here would have broken.

Applied: `Corpus, cpu` 921 → **939** (697 + 6 + 236), `test_cpu_corpus` 922 → **940**, the DuckDB
tier unchanged at 236, the cpu block 1629 → **1647**, Rust 2353 → **2371**, grand total 2851 →
**2869**. Every block header equals its rows and every total its parts.

**One figure is unmeasured and is the developer's first job.** `--lib` is carried at 678, pbench's
measurement, because this branch's own tests may have moved it and a coordinator cannot build. If it
is not 678, the cpu block, Rust and the grand total move with it.

### What the re-prove owes

Everything in the task's verification bar, on the new base, plus the two things this rebase makes
specifically doubtful: that the registry ↔ corpus pair still agrees in both directions after six
hand-resolved hunks, and that no pbench golden moved. Red drops the task to `building` with the
failure recorded here; green restores `building`, which the override set and which the device
half below is the work for.

## The re-prove on the new base (2026-10-09) — green

Every command with a `timeout`, plain `cargo test --features rust-only -p peacockdb-core` into
`./target`, `--test-threads=2`. `cargo test --no-run` built every rust-only target from cold:
exit 0, no warning.

| command | measured | expected | |
|---|---|---|---|
| `test_cpu_corpus` | **940 passed, 0 failed** | 940 | ✓ |
| `--lib` | **686** (684 passed, 0 failed, 2 ignored) | 678 carried | **corrected** |
| `test_corpus_goldens` | 26 passed | 26 | ✓ |
| `test_cost_model` | 3 passed | 3 | ✓ |
| `test_golden_format` | 43 passed | 43 | ✓ |
| `test_module_layout` | 18 passed | 18 | ✓ |
| `test_ci_coverage` | 11 passed | 11 | ✓ |
| `cargo test -p cost-report` | 39 passed | 39 | ✓ |
| `test_duckdb_result.py` | 20 passed | 20 | ✓ |
| `test_duckdb_cost.py` | 41 passed | 41 | ✓ |
| `generate_pbench.sh --check` | `pbench.sf1 matches gen.sql` | — | ✓ |

**The 27 carried reds are gone, and that is the rebase's doing, not a weakening.** They were the
26 `duckdb_gpu_*` cases plus
`every_enabled_device_cell_has_its_gpu_result_section_and_no_other`, red on a `gpu-result.txt` no
device had written. pbench's device cycle wrote the three files, so all 56 `duckdb_gpu_*` cases
now compare against a recorded answer and pass. Counted in the log: exactly 56 `duckdb_gpu_*`
cases, which is the 56 enabled gpu cells.

### `--lib` is 686, and the page's own rows already said so

`build-test.md`'s header carried pbench's 678; its **rows sum to 1655**, and
1655 − 939 − 1 − 26 − 3 = **686**. The rebase took the row numbers to HEAD (which carry this
branch's 8 new tests: `Recipes per join type` 24→27, `Translator` 29→30, the new
`Rollup's shuffle keys` 3, `Expression writer` 17→18) and re-derived only the four-figure header
line, so the header was the one thing left at pbench's value. `--list` on the binary says
`686 tests, 0 benchmarks`, independently.

Corrected in `build-test.md`, re-summed by script rather than by eye: `--lib` 678 → **686**, the
cpu block 1647 → **1655**, Rust 2371 → **2379**, grand total 2869 → **2877**. Every block header
now equals its rows (cpu 1655, ffi 7, gpu 606) and the grand total its parts
(2379 + 97 + 401 = 2877).

### The registry ↔ corpus pair, answered

Both deciding tests are green by name in the `test_cpu_corpus` log:
`the_registry_matches_the_cpu_corpus_in_both_directions` ... ok and
`every_device_cell_has_a_cpu_cell_at_the_same_mode` ... ok. Independently re-derived here, by
parsing the declarations and the CSV:

| | declarations (`corpus_cases.inc`) | registry (`cost-registry.csv`) |
|---|--:|--:|
| active lines / rows | 177 | 198 |
| cpu cells | 697 | 697 enabled |
| gpu cells | 56 | 56 enabled |

and 136 lines at all five cpu modes, 8 partial, 33 fully out, oracles 110 / 16 / 14 / 33 / 4,
144 running lines — every figure the coordinator re-derived, reproduced. No line declares a gpu
cell without a cpu cell. So the six hand-resolved hunks agree in both directions.

### No pbench golden moved that this branch did not move on purpose

`git diff --stat ENS-pbench..HEAD -- testdata/goldens/pbench.sf1/` is 12 files, and **the device
cycle's three answer files are not among them**: `gpu-result.txt`, `duckdb-result.txt` and the
two tp1 `-mini.cpu.txt`/`.cost.txt` sets are byte-identical to the base. The 12 are exactly this
branch's own two edits:

- `timestamp-s-key-group`'s `--- recipes ---` block at all five modes, `not runnable: …​ (#240)` →
  a real recipe tree (Task 5b);
- `rollup-small-keys`'s tp4 cpu cells turning on — the three `tp4-*-mini.cpu.txt` and
  `.cost.txt` sections going `skipped: not enabled at this mode` → a real block, the three
  `tp4-*.plans.txt` sections losing `__grouping_id` from `hash=`/`hashed_on=`, and
  `mini.result.txt`'s `mode=` stamp going `tp1-rowgroup` → `tp4-sized` with not one answer row
  changed.

### Verdict

Green. The parenthesised state is `building` and the device half below is the work.

## The device half (2026-10-09), on nebius-gpu's L40S

Host `dmitry@89.169.109.150`, card idle, cuDF 25.02 at `~/data/miniforge3/envs/rapids-cuda-12.2`.
Every cycle is: rsync the **uncommitted** tree with `--delete-after`, `build-test-shadgpu.sh
--build` on the host, then run the staged binary directly. Never `--run` or `--pull-results`,
which ssh to shad-gpu.

### The device baseline on the new base, before any of this task's device work

Measured first, so a later red is attributable. Every target green:

| binary | measured | `build-test.md` |
|---|---|---|
| `peacock_plan_tests` | 56 passed | 56 |
| `peacock_gpu_tests` | 4 passed | 4 |
| `test_gpu_corpus` | 58 passed | 58 |
| `test_node_timing` | 1 passed | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 536 passed | 536 |
| `peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered | 11 (3 are `bench_`) |

`the_registry_matches_the_gpu_corpus_in_both_directions` is green, so the registry ↔ corpus pair
agrees on the **device** side too, not only the cpu side.

**Two things the record left owing on the gtests are answered.**
`Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` **passes on a card** — the count
assertion holds at 24 and each of the four `NULL::TimestampX` projects throws. And the throw is
`build_scalar`'s default, not an earlier refusal carrying the name: `grep -rn EnumNameDataType
cpp/src/` returns exactly two sites, `expr.cpp:483` (`build_scalar`'s default,
`"unsupported scalar type: " + name`) and `expr.cpp:654`, which names a *scalar function's*
`return_type()` and is unreachable from a bare `NULL::T` project. So the substring search cannot
be satisfied by anything else.

### Plan Task 1 — #201, the gate proves `rows_per_lane` and not a copy of it

**The defect, measured on a card first.** With the gate unchanged and production's
`SEED` set to 43 — the kernel still seeded at 42 — **all 10 gates passed**. That is #201 exactly:
the guard could not go red, because `cpu_partition_ids` was a second copy of the rule that moved
with the gate rather than with production.

Then the rewrite: `pmod` and `cpu_partition_ids` are gone from the gate, `pmod` is `pub(crate)`
in `spark_partitioning.rs`, and `production_partition_ids` builds a `RecordBatch` and a `Column`
expr per key and inverts production `rows_per_lane`'s per-lane row lists into one id per row.
`assert_gpu_matches_comet_live` → `assert_gpu_matches_rule_live`.
`cpu_reference_2col_partition_ids_for_probe` is deleted — it was a print-only probe over the
deleted copy. `pmod_handles_negative_hashes` now asserts the imported production `pmod`.

**The red-green cycle, the whole of #201's proof:**

- `SEED = 43`: **7 failed, 2 passed** — every `*_match_rule_live` gate red, each printing its two
  lane vectors, e.g. the string gate `left: [3, 7, 1, 0, 4, 2]` against
  `right: [6, 7, 3, 4, 2, 4]`. The two that stay green are the two that do not drive the kernel
  (`pmod_handles_negative_hashes`, `step_i_comet_murmur3_public_api_compiles_and_runs`).
- `SEED = 42` restored: **9 passed, 0 failed.**

The gate count is 10 → 9, the deleted probe.

**The third copy of the rule is gone too.** `CudfGpu.SparkPartitionIdsMatchCometSingleCol` and
`CudfGpu.SparkPartitionIdsMatchComet2ColWithNulls` and their `gpu_partition_ids` helper are
deleted from `cpp/tests/gpu/test_cudf.cpp` — 47 lines of hardcoded comet ids. With them went the
includes nothing else in the file used (`cudf_test/column_wrapper.hpp`, `peacock/partitioning.hpp`,
`cuda_runtime.h`, `cudf/hashing.hpp`, `cudf/table/table_view.hpp`, `cudf/utilities/default_stream.hpp`,
`cstdio`, `vector`) and the `cudf::test::get_default_stream` shim, which existed only for
`strings_column_wrapper`. `peacock_gpu_tests` now runs **2 tests, both PASSED**, so
`build-test.md`'s "cuDF GPU smoke" row drops 4 → 2.

### Plan Tasks 3, 4, 5, 6, 6b — every kernel arm, each red first as a live gate

One device cycle per arm group, `PCK_TEST_FILTER` being the binary's own filter argument
(`peacockdb_core_gpu_lib murmur_conformance --test-threads=1`). The gate count runs 9 → 19.

| plan task | arm | the red, measured | green |
|---|---|---|--:|
| 3 | Boolean, as comet's i32 | `unsupported key column cuDF type_id=11` at `.cu:179` | 12 |
| 4 | Float32/64 | `type_id=9` and `type_id=10` at `.cu:180` | 15 |
| 5 | Timestamp, all four units + a zoned one | `type_id=13` (TIMESTAMP_SECONDS) at `.cu:212` | 16 |
| 6 | Decimal128, 16 bytes | `type_id=27` at `.cu:219`, both gates | 18 |
| 6b | UInt8/16/32/64 | **the cpu first**: `comet murmur3: Internal error: Unsupported data type in hasher: UInt8` — comet has no unsigned arm at all, so the rule refuses before the kernel is reached | 19 |

**Int8, the zero-row input and the all-NULL key were green on arrival**, which is the survey's
point: the kernel already widened INT8 and already skipped the kernel launch at `n == 0`, and
nothing asserted either. They are gates now.

Each arm also widened the `CUDF_FAIL` text's supported list, so the refusal a *future* unmapped
type gets names what is actually supported. The fixed-width kernel's comment
"Int64/Timestamp-as-i64 → 8B" was true of nothing before Task 5 and is true now.

**The cpu halves, in `hash_keys`.** Three normalizations before comet, each the other half of a
two-engine rule:

- `canonical_nans` maps every NaN to Rust's `NAN` (`0x7ff8000000000000` / `0x7fc00000`). comet
  hashes a float by its bits, so without it `NaN` and `-NaN` split at tp4 while the device
  equates them at tp1.
- a `Decimal128(p, s)` with `p < 38` is cast to `Decimal128(38, s)`, so comet hashes 16 bytes at
  every precision.
- `UInt8`/`UInt16` cast to `Int32`, `UInt32` to `Int64`, and `UInt64` goes through
  `unary::<_, Int64Type>(|v| v as i64)` — **by its bits, not its value**, since a `u64` past
  `i64::MAX` has no `i64` value. The gates carry `1 << 63` and `u64::MAX` for exactly that.

#### One deviation: the NaN lane test is rust-only, not a device gate

The plan puts `every_nan_shares_a_lane_and_so_do_the_two_zeros` in `murmur_conformance.rs` with a
`lanes_of` helper, then says to run it locally under `--features rust-only`. Those two cannot both
hold: `cpu_backend/mod.rs:716` declares `gpu_tests` `#[cfg(all(test, feature = "gpu"))]`, so
anything in that file runs on the device rung alone.

The test needs no device — it asserts which rows the **cpu rule** puts together — so under the
rung rule ("a test module declares the lowest build rung it needs") it belongs in the rust-only
tier, and it is now `executor/cpu_backend/spark_partitioning/tests.rs`, the sanctioned
`foo.rs` + `foo/tests.rs` shape that `accumulate.rs` and `join.rs` already use in this
subcomponent. It therefore runs in every CI cpu leg rather than only on a card, which is strictly
more coverage than the plan asked for. Its red-green cycle was watched in that tier:

    assertion `left == right` failed: NaN and -NaN share a lane
      left: 7
     right: 2

before `canonical_nans`, green after. `lanes_of` lives in that file instead, so
`murmur_conformance.rs` gained no unused helper.

#### The cpu goldens D2 moves: 18 `.cpu.txt` sections and 12 `.cost.txt`, 6 queries, not one answer

The decimal cast changes which lane a decimal key lands in at p ≤ 18, so the cpu's own tp4
goldens move. **That is decision D2's designed effect, not a regression** — the spec says "a
decimal's lane no longer matches Spark's at p ≤ 18, deliberately" — but it is a golden movement
and it was diagnosed before it was regenerated, not cleared with `UPDATE_CANONICAL`.

**The 18 reds, before any regeneration**, from `test_cpu_corpus` (922 passed, 18 failed):
`pbench/decimal15-key-group` and `-key-join`, `tpch/q2`, `q10`, `q18`, `tpcds/q82` — six
queries × the three tp4 modes, and **nothing else in the 940**. All 18 panic at the same site,
`test_support/corpus_golden.rs:202`, which is the golden-text differ. Zero DuckDB cases and zero
meta cases failed, which is what says the *answers* did not move: the DuckDB tier compares
against `duckdb-result.txt` and each corpus case compares the live answer against plain
DataFusion at one partition, and both stayed green through the red.

The signature is lane redistribution and nothing else — `in_rows=[[12,14,17,14]]` →
`[[14,14,11,18]]` on q18 (same total, 57), `[[0,0,0,2]]` → `[[0,0,2,0]]` on q82 (a permutation).

**`decimal38` does not move, and that is the check that the cause is the cast and not something
else**: at p = 38 comet already hashed 16 bytes, so the widening is a no-op there, and
`decimal38-key-group`/`-key-join` stayed green throughout.

Regenerated scoped to those six queries, then counted by parsing `== <section>` headers rather
than by eye:

| | |
|---|--:|
| `*-mini.cpu.txt` sections moved | **18** (6 queries × 3 tp4 modes) |
| `*-mini.cost.txt` sections moved | **12** |
| sections moved, both kinds | **30** |
| sections added | **0** |
| sections removed | **0** |

No `.plans.txt`, no `.result.txt` and not `recipe-payloads.txt` moved — correct, because the lane
rule is not in the wire and does not change the answer. The 12 cost sections are the 18 minus
`tpch/q18` and `tpcds/q82`, whose costs are unchanged because their lane change is a permutation
of the same per-lane totals and the model sums across lanes; `test_cost_model` is green, which is
the independent check that each `.cost.txt` still derives from its `.cpu.txt`.

**The strongest single check: not one `output_rows=` value changed anywhere in the 18 `.cpu.txt`
sections**, which are the only ones that carry the field.
Extracting every `output_rows=N` from the `-` side and the `+` side of the diff and sorting both
gives byte-identical lists. Only the per-lane `in_rows` split and the `output_bytes` that follows
from a different row mix moved.

Afterwards: `test_cpu_corpus` **940 passed, 0 failed**; `test_cost_model` 3,
`test_corpus_goldens` 26, `test_golden_format` 43, `--lib` 685 passed + 2 ignored = **687**.

### Plan Task 7 — every key type through the operator harness

`synthetic::key_types(rows, seed)` and `key_types_schema()`: 20 columns, ordinals 0–19, the
unsigned four last so the rest keep theirs. Five tests of the generator itself, in the rust-only
tier beside `a_synthetic_batch_is_the_same_batch_twice`, and they are gates rather than
bookkeeping — each asserts a property a case downstream depends on and nothing else would catch:
deterministic from the seed, a null in all 19 non-id columns, **both zeros and both NaN signs
present in `f64`**, a `u32` past `i32::MAX` and a `u64` past `i64::MAX`, and a `dec38` value no
`i64` can hold.

**The three `bug_` pins flipped rather than being deleted quietly.** Before Task 7 they were
*red*, which is the signal the style guide describes — `bug_a_float_key_is_refused_on_the_device`,
`bug_a_boolean_key_is_refused_on_the_device` and `bug_a_decimal_key_is_refused_on_the_device`
asserted a refusal the kernel no longer makes (measured: 542 passed, **3 failed**, exactly those
three). They are gone, and in their place 21 green cases: one per key ordinal 1–19, a mixed
composite over `[f64, dec38, s, d]`, and a zero-row batch scattered on each of the 19 in turn.
`refused_key_type` went with them, having no caller.

`emit_schema_cases.rs` gained 15, one per key type whose kernel arm *normalizes* the column —
the half a lane comparison cannot see, since both engines could agree on the lane and the device
still hand up the widened column rather than the declared one.

| binary | before | after |
|---|--:|--:|
| `emit_cases` | 18 (3 of them pins) | **33** |
| `emit_schema_cases` | 6 | **21** |

#### Plan Task 7 step 3b — #245's struct-key pin cannot be written, and #255 is why

The pin was written, run, and removed. It does not reach the hasher at all:

    type_structural_size: unhandled DataType Struct([...]) — add a deterministic arm
    panicked at peacockdb-core/src/common.rs:66

**That is [#255](../tickets/complete-coverage.md#t255) by its own citation** — the ticket names
`common.rs:66` and the same `"add a deterministic arm"` message, and says in as many words "Not
#245 either — that is the shuffle's hasher". The harness prices every batch's schema, so a nested
key aborts before any scatter runs, on either engine. Adding a Struct arm to
`type_structural_size` to make a pin work would be production code changed for a test, outside
the spec's Restriction, and the comment at that site says the arm it wants is a *deterministic*
one, which a Struct's cannot be derived from the parent row count. So the step is **blocked by
#255, not deferred by choice** — the same shape as Task 5c step 4, in a different place.

### Plan Task 8 — #243's two pins, green as pins

Both assert the divergence and both matched the plan's predicted numbers on the first run, which
is itself evidence the divergence is understood rather than discovered:

| pin | cpu | device |
|---|--:|--:|
| `bug_a_float_group_key_splits_negative_zero_and_the_nans_on_the_cpu` | 6 groups | 4 |
| `bug_a_float_join_key_misses_negative_zero_and_nan_pairs_on_the_cpu` | 4 rows | 7 |

`float_key_batch()` and `rows()` are `pub(crate)` in `aggregate_cases.rs` and shared with
`join_cases.rs`. `hash_join_with` now delegates to a new `hash_join_on`, which takes the key pair
— the old helper hardcoded `vec![(1, 1)]` and the pin needs `(4, 4)`, and a parameter is better
than a second copy of the builder.

**The pins are about equality, not the lane rule**, and they stay green precisely because this
task did not touch the cpu's float equality: #243 is reworded, not fixed.

### Plan Task 5b's owed gtest, written and proved on a card

`PlanExecutor.CastTimestampMicrosToSeconds` plus `PlanExecutor.CastToEveryWireTimestampUnit`.
`peacock_plan_tests` is **58 passed** (was 56).

**The plan's construction does not work and the measurement says why.** It asks for a
`TIMESTAMP_MICROSECONDS` column holding `1'500'000` and `-1`, built by casting an int64. cuDF
refuses that:

    CUDF failure at cast_ops.cu:428: Timestamps cannot be converted to numeric
    without converting it to a duration

which confirms the note already in `test_plan_executor.cpp` ("cudf::cast makes no timestamp from
a number"). The source is therefore a Date32 `CASE` — 1995-03-15 for the first ten nations,
1969-12-31 for the rest — cast up to the unit under test, which gives a **negative** instant as
well as a positive one. The sub-second flooring the plan wanted cannot be reached without a
timestamp column in `tpch.minimal`, and is recorded here rather than faked.

**Red-green on a card, not assumed**: with the four `fb_to_type_id` arms deleted both gtests fail
with `type_dispatcher.hpp:546: Invalid type_id` — the target maps to `EMPTY` — and both pass with
them restored. That is the cycle the record said was owed for the C++ half of Task 5b.

### Plan Task 9 step 2 — the cells, each settled on what it measured

**The registry is the write-side authority, and that cost a round.** `declared_sections`
(`corpus_golden.rs:445`) reads `cost-registry.csv`, not `corpus_cases.inc`, so a newly-enabled
cell's golden section **cannot be written until the CSV says `enabled`** — the merge rebuilds
against a skeleton the CSV produces and puts the `skipped: not enabled at this mode` marker back.
Worse, a regeneration run reports that case as `ok` while doing it, because under regeneration the
case writes instead of asserting. The first attempt looked green and had written nothing; only
reading the golden back caught it. **Order matters: registry first, then regenerate, then verify.**
The same effect hid three `gpu-result.txt` sections on the first recording run.

Also: `UPDATE_CANONICAL=1` is the *whole-file* form and `PCK_UPDATE_SECTIONS=1` the filtered one
(`test_support/mod.rs:646`). A filtered run wants the latter.

#### The cpu side — #189's last six cells

`uint-key-group` and `uint-key-join` go `tp1_single | tp1_rowgroup` → `all_modes` on the cpu: six
cells, all green. These are the six #189 was still holding, and comet could not hash them before
Task 6b's widening. #189 now reaches all 24 of its cells.

#### The device side — 21 cells on, 24 left off with the ticket each actually failed on

Every candidate was enabled, run on the card, and then settled on the measurement. **21 on:**

| query | gpu before | gpu after | ticket struck |
|---|---|---|---|
| `bool-key-group` | two tp1 | **all five** | `206` |
| `decimal15-key-group` | two tp1 | **all five** | `95` |
| `decimal38-key-group` | two tp1 | **all five** | `95` |
| `timestamp-ms-key-group` | two tp1 | **all five** | `240` |
| `timestamp-ns-key-group` | two tp1 | **all five** | `240` |
| `timestamp-us-key-group` | two tp1 | **all five** | `240` |
| `uint-key-group` | two tp1 | **all five** | `189` |

Each of those seven rows now has **zero off cells, so its ticket column is empty** —
`registry.rs:242` would accept a stale ticket there, which is exactly the rot to avoid.

**Left off, each with the ticket it was measured to fail on:**

| query | cells | measured failure | ticket after | before |
|---|--:|---|---|---|
| `timestamp-s-key-group` | 5 | `CudfAggregate: only ColumnRef group exprs supported` — the query groups on `arrow_cast(f_ts_s, …)`, so the group expr is a CAST | **wants a new ticket**; left at `240` | `240` |
| `uint-key-join` | 4 | `#152` verbatim: "this join's recipe copies its build side per probe batch and the ABI has no copy" | `152` | `152 189` |
| `decimal15-key-join` | 5 | the cpu's `batch_rows=[[8192,8192,…]]` against the device's `[[132216]]` — #220's several-batches-per-join-call | `152 220` | `95 152 220` |
| `ts-key-join` | 5 | the same, `[[8192,…]]` vs `[[100000]]` | `152 220` | `152 220 240` |
| `rollup-small-keys` | 5 | `the output hook refused a batch: 2 __grouping_id: UInt8 vs INT32` — #65 verbatim | `65` | `65 206` |

**Four tags dropped because they stopped explaining a cell**: `95` from `decimal15-key-join`
(decimals hash now), `240` from `ts-key-join` (timestamps hash now), `206` from
`rollup-small-keys` (the boolean arm exists; #65 is what is left), `189` from `uint-key-join`
(the cpu cells are on). Each was checked against a *measured* failure, not against a plan.

#### `timestamp-s-key-group` is the one finding that needs a ticket I may not file

Its five device cells fail on a cause that is **not #240 and not in this task's scope**: the
device's `CudfAggregate` (`cpp/src/operators/aggregate.cpp:163`) refuses any group expression
that is not a `ColumnRef`. #240 is fixed — the ms/ns/us rows prove the timestamp *key* hashes —
and the wire now names the type, which is what this branch added. There is no existing ticket for
it: `grep` over every ticket file finds nothing about a computed group expression, and the nearest
neighbours are #57 (a value-form CASE) and #62 (a DISTINCT beside an aggregate), neither of which
is this.

So the row keeps `240` for now, because `cost-report/src/main.rs:475` **exits 1 on a registry
ticket that resolves to no anchor**, and the ticket files are not mine to write. **Two consequences
for whoever picks this up: #240 must not be archived until row 195 is retagged, and the ticket
needs filing.** Proposed, under the 15-line cap:

> ### #264 — the device refuses a group key that is not a bare column
> `CudfAggregate` throws `"CudfAggregate: only ColumnRef group exprs supported"`
> (`cpp/src/operators/aggregate.cpp:163`) for any group expression that is not a `ColumnRef`, so
> `GROUP BY` over a cast, an arithmetic expression or a function call is refused at run time on
> the device at every mode. The cpu answers. The planner does not lower a group expression into a
> project below the aggregate, and the recipe hands the expression through as it stands.
>
> **Corpus queries:** `pbench/timestamp-s-key-group`
> (`GROUP BY arrow_cast(f_ts_s, 'Timestamp(Second, None)')`). Its five device cells are off and
> registry row 195 tags `240` only because this ticket has no number yet; retag it on filing.
> Not #240 — the timestamp key itself hashes on both engines since repartition-keys, which the
> three `timestamp-{ms,ns,us}-key-group` rows demonstrate at all five modes.

### The final measurement, both engines, on the formatted tree

Every number below is from a run after the last edit, not carried forward.

**cpu, local, `--features rust-only` into `./target`, `--test-threads=2`.** `cargo test --no-run`
from cold: exit 0, **no warning**.

| target | measured | before this round |
|---|--:|--:|
| `test_cpu_corpus` | **967** passed, 0 failed | 940 |
| `--lib` | **692** (690 passed, 0 failed, 2 ignored) | 686 |
| `test_corpus_goldens` | 26 | 26 |
| `test_cost_model` | 3 | 3 |
| `test_golden_format` | 43 | 43 |
| `test_module_layout` | 18 | 18 |
| `test_ci_coverage` | 11 | 11 |
| `cargo test -p cost-report` | 39 | 39 |
| `test_duckdb_result.py` | 20 | 20 |
| `test_duckdb_cost.py` | 41 | 41 |
| `generate_pbench.sh --check` | `pbench.sf1 matches gen.sql` | — |
| `cost-report` binary | wrote `cost_report.html` | — |

The `cost-report` run matters on its own: `main.rs:475` exits 1 if any registry ticket resolves to
no anchor, so a completed run is the proof that every tag left on a row — and every tag cleared —
still resolves.

**device, nebius-gpu L40S, cuDF 25.02, each staged binary run directly.**

| binary | measured | before |
|---|--:|--:|
| `peacock_plan_tests` | **58** | 56 |
| `peacock_gpu_tests` | **2** | 4 |
| `peacock_cpu_tests` | 15 | 15 |
| `test_gpu_corpus` | **79** | 58 |
| `test_node_timing` | 1 | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | **580** | 536 |
| `peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered | 8 / 3 |
| of which `murmur_conformance` | **19** | 10 |

**The device lib's +44 reconciles exactly**, per file, against `HEAD`: `emit_cases` 15→33 (+18),
`emit_schema_cases` 6→21 (+15), `murmur_conformance` 10→19 (+9), `aggregate_cases` 20→21 (+1),
`join_cases` 89→90 (+1). 18+15+9+1+1 = 44, and 536+44 = 580.

### `build-test.md`, re-summed by script and not by eye

`--lib` 686 → **692**, `test_cpu_corpus` 940 → **967**, `Corpus, cpu` 939 → **966**,
`Harness helpers` 16 → **21**, a new `The lane rule's own properties` row at **1**, the cpu block
1655 → **1688**; `Corpus, device` 57 → **78**, `Operator harness` 335 → **355**,
`Operator harness, what the device holds` 134 → **149**, `GPU↔comet murmur3` 10 → **19**,
`test_gpu_corpus` 58 → **79**, the gpu block 606 → **671**; `cuDF GPU smoke` 4 → **2** and
`Plan-executor` 56 → **58**, which cancel, so C++ stays 97; Rust 2379 → **2477**, grand total
2877 → **2975**. Prose counts that are arithmetic: the DuckDB tier 236 → **257** (177 + 77 + 3),
`697 cells` → **703**, and the partial-line list drops `uint-key-group` and `uint-key-join`, which
now run at all five cpu modes, leaving `scalar-subquery-cross`.

A script checks three things rather than a reader: every block header equals its rows, every
header's named parts equal the header, and the grand total equals the two tables. All three hold.

### Formatting

`rustfmt --edition 2024 --config skip_children=true` is **clean** on
`spark_partitioning.rs`, `spark_partitioning/tests.rs`, `synthetic.rs`, `emit_cases.rs`,
`emit_schema_cases.rs`, `join_cases.rs` and `aggregate_cases.rs` — every one of which was clean at
`HEAD` and had to stay so. `murmur_conformance.rs` was **already 189 lines drifted at `HEAD`**
(its data vectors are hand-formatted several-values-per-line), and the precedent here is round 2's
deviation 4: rather than bury the rewrite in 189 lines of unrelated reflow, every construct *this
round added* was put in rustfmt's shape one at a time — eight of them — and the remaining 129
differing lines were then checked to be nothing but the pre-existing vector idiom.

`git clang-format --diff HEAD -- cpp/` is **clean**; its 112-line patch was applied to the working
tree with `git apply` (the index is untouched, and `git clang-format` itself refuses an unstaged
tree). The machine format was kept even where it breaks the one-parameter-per-line idiom of the
kernels around it, as `coding-style.md` requires and as round 2's deviation 5 already settled.

### Deferred by the chain-wide host override, each recorded as such

- **the sf40 pair**, `peacock_tpch_tests` and `peacock_tpchv_tests` — not run. Doubly
  unrunnable: 69 GiB against the L40S's 46 GB, and no `tpch.sf40` on the host.
- **`--run-benchmarks` and the three `bench_` cases** — not run; the 3 filtered cases in every
  `peacock_gpu_benchmarks` line above are exactly those.
- **Nsight captures** — not taken.
- **H200 timing** — not taken; no H200.
- **cuDF 26.02** — not built; red there for #260 and #94, outside this chain.

Nothing was deleted from the host: it ended with 37 GB free of 96, so the override's cleanup rules
were never needed.

### One trap worth the next developer's attention: the registry writer and CRLF

Editing `cost-registry.csv` with Python's `csv.writer` rewrites **every** line, because the
writer's default terminator is `\r\n`. The eleven intended row changes came out as a 199-line
diff, and **every test still passed** — the parser is tolerant of it — so nothing would have
caught it before review. `git diff --numstat` is what caught it: 199/199 where 11/11 was expected.
Converted back to LF and re-verified; the diff is now 11 rows and nothing else. Check the numstat
after any scripted edit of that file.

### What this round did not take, and why

- **Plan Task 10's wiki edits and the five archivals.** `llm-wiki/` markdown other than this
  plan and this file belongs to the human in this dispatch, with `build-test.md`'s counts carved
  out and done. The prose owed, and what each sentence should say, is above.
- **#95, #189, #201 and #206 are ready to archive; #240 is not.** Every cell the first four held
  is now run and enabled, #189's at all 24. #240's last five cells
  (`pbench/timestamp-s-key-group`) are off on a different cause, so archiving it would leave them
  explained by a closed ticket — the rot this round was told to avoid. File the proposed #264,
  retag registry row 195, and #240 goes with the rest.
- **The #245 struct-key pin** — blocked by #255, demonstrated rather than assumed.
- **The float rows** (`float32-key-group`, `float64-key-group`, `float64-key-join`) stay
  commented out, on #243, which this task reworded and did not fix. The spec says so.
- **The sub-second half of the timestamp cast gtest** — `tpch.minimal` has no timestamp column
  and cuDF makes no timestamp from a number, measured.

### The last run, on the exact tree left in the working directory

After the CRLF fix and the formatting, both engines re-run end to end:

    cpu   test_cpu_corpus 967/0 · --lib 690+2 · test_corpus_goldens 26 · test_cost_model 3
          test_golden_format 43 · test_module_layout 18 · test_ci_coverage 11
          cost-report 39 · cost-report binary exit 0
    gpu   peacock_plan_tests 58 · peacock_gpu_tests 2 · peacock_cpu_tests 15
          test_gpu_corpus 79/0 · test_node_timing 1 · gpu_lib 580/0
          peacock_gpu_benchmarks 8 passed, 3 filtered

Zero failures anywhere. The device build emitted **no warning**; the one warning the device
type-check still reports, `unused import: AsArray` in `gpu_tests/aggregate_dimension_cases.rs`,
is on `ENS-pbench` and that file is not in this diff. nebius-gpu ended with 37 GB free of 96, so
nothing was cleaned.

**To stage:** the 32 modified files plus the one new directory,
`peacockdb-core/src/executor/cpu_backend/spark_partitioning/` (which holds `tests.rs`).

## The rebase discharged, and the state restored past it (2026-10-09)

The re-prove came back green, so the rebase is discharged: `test_cpu_corpus` 940/0 with the 27
carried reds gone, the registry ↔ corpus pair agreeing in both directions by name, and no pbench
golden moved that this branch did not move deliberately. The six hunks I resolved by hand hold.

The parenthesised state was `building`, which is what a green re-prove restores. The same dispatch
then built the device half and reported it green with evidence, which is the trigger for
`reviewing` — so the board goes there rather than through `building` and straight back out. The
distinction matters only if the re-prove had been red, and it was not.

`--lib` was the one count I carried unmeasured at pbench's 678. It is **686** on this branch, and
the page's own rows already summed to it: 1655 − 939 − 1 − 26 − 3 = 686, with `--list` agreeing
independently. The rebase had re-derived the header alone. Corrected, with its three rollups.

## The coordinator's half of Task 10, and the archival left undone (2026-10-09)

**#264 filed**, in `corpus-coverage.md`'s Aggregates section, from the developer's proposed text:
the device refuses a group key that is not a bare column, so `GROUP BY` over a cast is refused at
run time at every mode while the cpu answers. `pbench/timestamp-s-key-group` is retagged from `240`
to `264` — the timestamp key itself hashes on both engines now, which the three
`timestamp-{ms,ns,us}-key-group` rows demonstrate at all five modes, so `240` had stopped
explaining that cell and was only there for want of a number. Index re-summed: 122 open,
corpus-coverage 35, next-free 265, and every row's declared count equal to both its listed IDs and
its file's anchors, in the same order. Every registry tag resolves to a live or archived anchor.

**`architecture.md`'s hash section gained the invariant this task created.** It said placement is
identical by construction and named the gate; it now also says the gate calls production
`rows_per_lane` rather than a copy of the rule (#201's whole point), and that three key types agree
with each other rather than with Spark — decimals hashed as 16 bytes where Spark hashes 8 at
precision ≤ 18, NaN canonicalized where comet hashes raw bits, and unsigned keys, which Spark has
no type for, cast or reinterpreted to the next wider signed type. That is the fact a reader needs
before touching either side, and it is the one thing neither engine's code says on its own.

**#243 reworded to the cpu's equality alone**, as the Restriction directs, and trimmed from 27
non-blank lines to 19 while doing it. The lane split it also described is fixed and is now named as
out of scope rather than left reading as open.

### The archival is not done, deliberately

The spec's Scope ends "#95/#189/#201/#206/#240 archived" and I have not done it. Measured, by the
only test that settles it — how many registry rows still tag each:

- **#189, #201, #206: zero rows.** **#240: zero**, after the retag above.
- **#95: eight rows** — `tpcds/q24`, `q37`, `q75`, `q82` and `tpch/q2`, `q10`, `q15`, `q18`.

So #95 is not archivable on the evidence, and whether those eight tags have stopped explaining
anything is precisely the judgement this round's own warning is about: the registry rule asks for
*some* ticket, so it cannot catch a tag that has gone stale. Most of those rows wait on #152 too,
which the spec predicted; a reviewer should decide whether `95` is still the live blocker on any
cell or is now rot.

Three reasons the other four are also left for a verified pass rather than done here. Each is
linked from five to seven `llm-wiki` files including the board's own task heading, so archiving is
a link migration and not a move. A coordinator cannot build, and
`cost-report/src/main.rs:475` exits 1 on a ticket resolving nowhere, so the change needs a run to
prove. And `build-test.md`'s own note on [#263](../tickets/testinfra.md#t263) says a commit that
only moves a ticket can turn the rust tier red while both CI layers skip the pipeline — which is
the shape of an archival commit exactly, and the reason archival is ordinarily the helper's
post-merge step rather than a task's.

## Review round on the device half (2026-10-09)

**6 blocking, 5 important, 5 nits.** The code half was found sound — #201 genuinely closed, the
three divergences one rule each implemented on both engines with the cpu half really in this
branch, D2's golden movement exactly the decimal cast's fingerprint, every `build-test.md` count
re-summing, and the Restriction held. **Every blocking finding was in the prose**: four ticket
bodies still described their own subject as open, one named a closed ticket as a live blocker, and
one was a registry tag column that had gone stale.

The reviewer also settled two things from source that this round had got wrong, and both matter
more than the findings they correct:

- **Archiving cannot make a ticket resolve nowhere.** `TicketIndex::load` searches
  `archive/archived-tickets.md` as well as `tickets/`, so a move puts the anchor into a file that
  is already read. The reason recorded in this file's previous section was wrong, and I had used it
  to defer the archival. Measured over the committed tree: 34 distinct registry tags, zero
  unresolved.
- **The #263 hazard does not bite these five.** `ticket_is_open` reads only `tickets/`, so
  archiving reddens a `duckdb_divergent(<n>)` — but the only tickets in a `duckdb_divergent` line
  are #251 and #205. None of #95/#189/#201/#206/#240 is one.
- What is left of my caution is the one real risk: **nothing checks the wiki's ticket links**, so a
  retarget missed during an archival is silent.

### The five blocking prose fixes, mine, all applied

Each ticket body is rewritten to the state the registry actually records, verified cell by cell
first rather than from the account.

- **#206** read as fully open and named two `bug_` pins this branch deleted. Now: fixed, both arms
  in the kernel under live gates, `bool-key-group` on at all five device modes, and the two float
  rows commented out on #243 — which is the cpu's float equality, not this ticket.
- **#95** was untouched and still prescribed the design D2 rejected: dispatch by logical precision
  and thread precision through the partition FFI. That is the costliest of the four, because #95 is
  the only one of the five still cited by registry rows, so it is the body a reader arrives at, and
  it would have sent the next developer to build an argument deliberately not built. Now: the 16-byte
  rule on both engines, with D2's reason, and the eight stale tags named.
- **#240** said the kernel arm was outstanding and nobody had run the C++ side on a card. Both
  halves are in; the three `timestamp-{ms,us,ns}-key-group` rows are on at all five device modes,
  which is what proves the key itself hashes, and `timestamp-s-key-group` is off on #264.
- **#189** warned that enabling all 24 cells would turn 6 of them red. All 24 are on: the unsigned
  arm this branch added is the second fix the ticket said could not come from the grouping-id drop.
  No registry row carries it.
- **#65** named #206 as the live blocker of three tp4 cells. The registry says all five of
  `rollup-small-keys`' device cells are #65's; `206` was struck from that row by the device half and
  the sentence was not updated.

Also mine and applied: `build-test.md`'s murmur row described the ten-test file and named a deleted
test, where the file holds nineteen and two of them need no device; and `architecture.md`'s #201
clause was written as "used to compare", which `coding-style.md` forbids on that page — restated as
the invariant and what breaks it.

### Left for the developer, with the reviewer's own reasoning

**B6, and it is the one that needs care.** The spec asked that "#95's eight rows lose `95` where it
is their last ticket on a cell" and they still carry it. The reviewer derived it cell by cell: on
`tpcds/q75` and `tpch/q2`, `q10`, `q15`, `q18` every device cell including `gpu_tp1_single` is off,
and tp1-single runs one lane and hashes nothing, so `95` could never have been the blocker there;
on `tpcds/q24` all five cpu cells are off on #190 so no device cell is declarable; on `tpcds/q37`
and `q82` the off cells are the shuffling modes, where `95` *was* live and is not now. Every one of
the eight keeps another ticket, so `registry.rs`'s `off == 0 || !tickets.is_empty()` still passes
after the drop and no cell has to move. Dropping the tag is **not** a claim that those cells pass —
#152 still holds q37's and q82's.

Then I4 (`wire/tests.rs` crossed 1000 lines; `wire/serialize/tests.rs` is both the fix and the
shape this branch already used for `spark_partitioning/tests.rs`), I5 (a pin's comment states the
opposite of what the test asserts), N1 (`drop_grouping_id` runs before the `lanes > 1` test, so its
`Err` can refuse a one-lane plan for a shuffle never performed — unreachable today), N4 and N5, the
two count slips in this file, and then the archival of all five tickets with a verification run.

## The review round's developer half, and the archival done (2026-10-09)

B6, I4, I5, N1, N4, N5, the two count slips, and the archival of all five tickets. No device work:
nothing in this round needed a card, and nothing in it changed a count.

### B6 — the eight `95` tags, re-derived before anything was dropped

The derivation is one fact plus a per-row check. The fact: **#95 is fixed on both engines**, so no
cell anywhere is held by a decimal key the kernel cannot hash. The per-row check is whether `95`
was ever the *live* blocker, and whether something else still explains the off cells:

| row | cpu cells | device cells off | `95` could have been live? | tickets after |
|---|---|---|---|---|
| `tpcds/q24` | **all five off** on #190 | all five | no — no device cell is declarable at all | `190` |
| `tpcds/q37` | all on | 4 (tp1-rowgroup + three tp4) | at tp4 only; the tp1-rowgroup cell is #152's | `152` |
| `tpcds/q82` | all on | 4 (tp1-rowgroup + three tp4) | the same | `152` |
| `tpcds/q75` | all on | **all five, `gpu_tp1_single` included** | no — tp1-single is one lane and hashes nothing | `152` |
| `tpch/q2` | all on | all five | no, the same | `152 220` |
| `tpch/q10` | all on | all five | no, the same | `152 220` |
| `tpch/q15` | all on | all five | no, the same | `183 220` |
| `tpch/q18` | all on | all five | no, the same | `152 220` |

Where the reviewer's derivation and mine differ: on q37 and q82 the four off cells are **not** only
the shuffling modes — `gpu_tp1_rowgroup` is off too, and that mode runs one lane. So on those two
rows `95` was live at tp4 and could never have been live at tp1-rowgroup, where #152 (a second
probe batch) is the whole of it. The conclusion is the same: `95` explains nothing now.

Dropped from all eight rows; every one keeps another ticket, so `registry.rs:242`'s
`off == 0 || !tickets.is_empty()` still holds — proved by running the tier, not by reading it
(`test_cpu_corpus` 967/0, which includes the registry test). **Not a claim that those cells pass.**
33 distinct registry tags remain, down from 34.

One thing for `stale-cells`, not this task: `tpch/q15` now reads `183 220`, and **#183 is archived**
— a closed ticket explaining five off cells. That is exactly the 16 cells task 2 exists to clear,
and it was true before this round too.

### I4 — `wire/serialize/tests.rs`

980 lines left in `wire/tests.rs`. The three tests that reached `super::serialize::` moved to
`wire/serialize/tests.rs`, the `foo.rs` + `foo/tests.rs` shape, and now reach `super::` directly.
`--lib` stays **692** and `test_module_layout` **18**. `build-test.md` had to move with it, since
its rows count cases per module: `Recipes per join type` 27 → **24** (back to the base's figure)
and a new module-unit row `Arrow to the wire enum` at **3**. The cpu block still sums to 1688.

### I5 — what the pin actually pins

The `bug_` prefix is right and the comment was not. The test asserts a **refusal**, which is
wrong-for-a-user and is what #249 still owes an arm for; the `Null` the comment described is the
behaviour this branch deleted. The comment now says that, and the name needed no change — it
already reads "is refused at plan time", which is the wrong behaviour being pinned.

### N1 — the drop moved inside the `lanes > 1` arm

**No red test was possible and that is worth stating.** `drop_grouping_id` errs only on keys that
are empty after the retain or that sit past the id, and no plan the translator builds produces
either; with well-formed keys the old order computed the dropped keys, discarded them in the
one-lane arm, and planned the identical tree. So the reordering has no observable difference on any
input the planner can reach, and a test that distinguished the two would have to inject a key list
DataFusion never emits. Verified instead by the three `drop_grouping_id` unit tests (unchanged, they
call it directly), the 138 `planner::` cases, and `test_cpu_corpus` 967.

### N4, N5

`corpus_cases.inc:359` regains a trailing `// device: #264, a cast group key` — the line is the one
place that says why that column is `none` while its four siblings are `all_modes`. Do **not** run
`git clang-format` over that file: it reads the `.inc` as C++ and rewraps `corpus_query!` lines.
`spark_hash_partition.cu`'s dispatch comment now says the `default: CUDF_FAIL` sits inside
`if (n > 0)`, so a zero-row table with an unsupported key type succeeds and says nothing.
Comment only; pre-existing and harmless.

### N2, N3 — both counts re-measured, not re-read

N2: the device table sums to **24** off cells (5 + 4 + 5 + 5 + 5), not 20. "21 on" was right.
N3: "30 sections" was two counts added together. Measured from the base
(`d39fddede`) by parsing `== <section>` headers: **18** `*-mini.cpu.txt` sections moved (6 queries ×
3 tp4 modes, all 18 of the 18 that exist) and **12** `*-mini.cost.txt` (12 of 18 — tpch/q18's and
tpcds/q82's cost sections did not move, which is the permutation argument the section already made).
`output_rows=` lives only in the `.cpu.txt` kind, so that invariant is over 18 sections, not 30;
re-measured, 0 of the 18 changed its `output_rows` multiset.

### The archival, and the link migration that is the actual work

All five moved to `archived-tickets.md`'s Done, each carrying
`**Done 2026-10-09 by repartition-keys, PR #169; awaiting merge.**`. Links repointed, by grep over
both the `#tNN` and bare `#NN` spellings:

| ticket | links repointed | files |
|---|--:|---|
| #95 | 4 | `build-test.md`, `tasks.md`, `repartition-keys.md`, `tickets/complete-coverage.md` |
| #189 | 3 | `build-test.md`, `tasks.md`, `repartition-keys.md` |
| #201 | 4 | `build-test.md`, `architecture.md:1058`, `tasks.md`, `repartition-keys.md` |
| #206 | 2 | `tasks.md`, `repartition-keys.md` |
| #240 | 3 | `tasks.md`, `repartition-keys.md`, `tickets/corpus-coverage.md` (#240's own cross-link from #264) |

Plus two inside the archive that the move itself invalidated (#184's body cited #95 at its old
path; now same-file `#t95`). 18 in all.

`tickets.md` re-summed by script: corpus-coverage **35 → 30**, header **122 → 117**. Checked in
three directions for every row, not just the one that moved — declared count equals its listed
IDs, equals its file's `<a id>` anchors, in the same order, and each file's own Contents list
matches its anchors; the total equals the header. 181 anchors across `tickets.md`, `tickets/*.md`
and the archive, all distinct, each sitting above its own `### #NN` header, which is what
`anchored_numbers` requires.

**`cost-report` the binary, not only its tests** — the check the round was told to prize.
`scripts/cost-report-preview.sh`, exit **0**: every one of the 33 registry tags resolves to an
anchor. `#183` renders as `archive/archived-tickets.md#t183` in the output, which is the live proof
that `TicketIndex::load` reads the archive — the fact the previous round got wrong and used to
defer this work.

### Two things the tree contradicted that were not on the list

- **#201's body was never rewritten.** It was not among the five blocking prose fixes, and it still
  said "The fix is the gate calling `rows_per_lane` … not done". Archiving it in that state is the
  contradiction the round warned about for #95. Rewritten to the closed state from source:
  `murmur_conformance.rs` has no `cpu_partition_ids`, imports production `pmod` and `rows_per_lane`,
  and holds 19 tests of which two need no card.
- **#65 still named #206 as a live blocker.** The fixed sentence settled `rollup-small-keys`; the
  next sentence said the other rows "are off first on #152, **#206**, #212 or #220", and no registry
  row carries `206` at all. Now `#152, #212 or #220`, with #189 and #206 named as no longer
  blocking.

Also fixed where found: the archive header listed `tasks/active-tickets.md` among the files
`TicketIndex::load` reads, and the code reads `tickets.md`, `tickets/*.md` and the archive;
#184's body said "q15's registry row carries #95", which B6 made false.

### Left undone, deliberately

- **`reports/corpus-fixes.md:1128`'s D2 is still written as an open decision**, and the shape it
  recommends — `hash_key_precisions: [uint8]` appended to `CudfRepartition` — is the one D2
  rejected. Its "Cells at stake: q15 × tp4 (3 gpu)" is also stale: q15's device cells are #183's
  and #220's. `reports/` is the human's; the impl's Task 10 lists it as owed.
- **145 dead ticket links in `archive/archived-tasks.md`** and 7 more in `archived-tickets.md`,
  pointing at `active-tickets.md` (no such file) or `../tickets.md#tNN` (no anchors there). Five of
  them name #95 or #189. All predate this round, and repointing five of 152 would leave the file
  less consistent than it is. 176 broken ticket links wiki-wide, of 478.
- The device half's log above still reads `timestamp-s-key-group … left at 240` in its table. That
  was true when written; the retag to `264` is recorded in the section below it. Left as the dated
  record it is.

## Completeness pass — what is missing (analyst, 2026-10-09)

Read as one change: `git diff ENS-pbench...HEAD`, head `caec7e1b`. The question is what the
branch does not contain. **2 blocking, 5 important**, plus the three `architecture.md` sentences,
which are not scored. Everything below was derived from the committed tree; no project code was
built or run.

### Blocking

**B1. `build-test.md:531-541` still describes the pre-task device corpus.** The table counts
beside it were re-summed (`Corpus, device` 57 → 78, the gpu block 671) and the prose was not.
Five false statements, each measured against `cost-registry.csv`:

| the page says | measured |
|---|---|
| "Fifty-six cells today" | **77** (26 tpch/tpcds + 51 pbench; 78 = those plus the regeneration case) |
| "From pbench, thirty" | **51** |
| "the seven key-type groups … at the two tp1 modes, where no shuffle hashes the key" | all five modes on each of the seven; the shuffle hashes the key, which is this task |
| "The rest are off against #152, **#95**, #220 and the device's own tickets (#57, #63, #65, #205, **#206**, **#240**)" | no registry row carries 95, 206 or 240; all three are archived |
| "The fifty-seventh case is that a device run under a regeneration writes no golden" | the seventy-eighth |

One commit ago the review round graded four ticket bodies blocking for describing their own
subject as open. This paragraph describes this task's subject as not done, on the page an agent
reads under pressure to find a single fact.

**B2. Both public C++ headers still state the contract this branch changed.**

- `cpp/include/peacock/partitioning.hpp:1` — "Spark-compatible (**comet-identical**) hash
  partitioning on the GPU"; `:5-7` — "The CPU twin uses comet's `create_murmur3_hashes` (Spark
  spec), so to make the GPU partition assignment agree by construction we own a small
  Spark-murmur3 hash kernel"; `:30-33` — "the single source of truth the conformance test
  asserts **against comet's CPU twin**".
- `cpp/include/peacock_gpu.h:278-283` — "so the live conformance test can assert the REAL GPU
  path == **the REAL comet CPU helper** over the SAME bytes".

Neither is true now. Placement is deliberately not comet's for decimals at p ≤ 18, for NaN and
for every unsigned width, and since #201 the gate's reference is production `rows_per_lane`, not
comet. `architecture.md`'s hash section calls the divergence "the one thing to know before
changing either side", and these two headers are what a C++ reader opens first.
`prompts.md`: every commit keeps code, code comments and llm-wiki content in agreement. The
`.cu` comments were all updated; the headers were not touched at all.

### Important

**I1. The two hand-off facts this round bought die with the detail file.** `#259` exists because
a recipe in `<task>-impl.md` and `<task>-detail.md` is deleted at merge. Two such facts are here
and nowhere else:

- **Registry first, then regenerate, then read the file back** (§"Plan Task 9 step 2"):
  `declared_sections` (`corpus_golden.rs:445`) reads `cost-registry.csv`, not
  `corpus_cases.inc`, so a newly-enabled cpu cell's golden section cannot be written until the
  CSV says `enabled` — and under regeneration the case reports `ok` while writing nothing. It
  cost this round a round. `join-backend-impl.md`'s Task 14 step 1 sets `corpus_cases.inc` and
  runs, enabling cpu cells for #190's four rows and q77; it does not say the CSV must move first.
  `build-test.md`'s "Consequences worth knowing before you regenerate" list does not carry it
  either.
- **`git diff --numstat` after any scripted edit of `cost-registry.csv`** (§"One trap worth the
  next developer's attention"): Python's `csv.writer` terminates `\r\n` and rewrites all 198
  lines while every test still passes. join-backend edits that file in a dozen batches.

Secondary, same shape and lower stakes: the #245 struct-key pin is blocked by #255 and
demonstrated, but neither #245 nor #255 records that the harness pin is owed with the fix —
#255's "guard to delete when this closes" lists the registry rows, corpus lines and
`duckdb-result.txt` sections only.

**I2. Two spec claims the branch does not deliver.** Both already measured here; the signoff has
to restate them rather than let the arithmetic look wrong later.

- Scope, `plan_goldens.rs` row: "`NOT_RUNNABLE`: `timestamp-s-key-group` leaves; **the three
  #249 pbench queries enter**". They did not enter: the three `.sql` files do not exist (they are
  #255's) and the declaration guard is bidirectional, so the step was deleted rather than
  deferred. Recorded twice above.
- Registry: "`uint-key-group` and `uint-key-join` turn on at tp4 **on both engines**".
  `uint-key-group` did, both engines. `uint-key-join`'s device column is unchanged —
  `gpu_tp1_single` alone, its four other cells measured as #152 verbatim. Zero device cells were
  turned on for that row.

**I3. `reports/corpus-fixes.md` fix 12 still prescribes the design D2 rejected.** D2
(`:1128-1141`) now reads "decided, and neither append was taken" and ends "Details in fix 12".
Fix 12 (`#fix12`, `:625-668`) is the rejected design in full implementation detail:
"Recommended: append `hash_key_precisions: [uint8]` to `CudfRepartition`", "`partitioning.hpp`
gains `struct HashKey { column; decimal_precision }`, both entry points take `vector<HashKey>`",
"hashing `width` LE bytes … (8 if p ≤ 18, else 16)", plus a tests/goldens/registry recipe for it
and "#184 and #95 archived" as outstanding. That is the trap the review round called the costliest
when #95's own body carried it, one pointer away. One superseded line at the head of fix 12
closes it. (`reports/` is the human's; the report is dated at `188c23ce`, but D2 was already
updated, so the snapshot convention no longer protects the section it points into.)

**I4. The task's central invariant is not self-policing.** "The shuffle hashes every key type the
planner emits" is asserted by 19 hand-written gates in `murmur_conformance.rs` and 21 harness
cases in `emit_cases.rs`. Nothing fails by count when `convert_data_type`
(`wire/serialize.rs:102`) gains a member: the next key type lands unproven on both engines and
says nothing until a query reaches `CUDF_FAIL` at run time — and with zero rows the kernel's
`default` sits inside `if (n > 0)` and does not even say that. The tree already owns the pattern
one file away: `Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot`
(`test_plan_executor.cpp:2186-2190`) asserts `cases.size() == std::size(fb::EnumValuesDataType())`
precisely so a new member cannot arrive unexamined, and that assertion is what caught this
branch's own fbs append. The failure mode is a refusal rather than a wrong answer, so this is
important and not blocking; naming it in the signoff discharges it as well as code would.

**I5. Two ticket sentences this branch's own work falsified.**

- `tickets/complete-coverage.md:67` — #249's title still ends "**and writes such a field as
  `Null`**". The body now says "used to map … to `Null`" and "half done by repartition-keys", so
  the title contradicts its own first paragraph. The index is what a reader scans.
- `tickets/complete-coverage.md:31-32` — #195's bullet "a shuffle keyed on a decimal: not a query
  but #95's kernel work, and the murmur3 conformance gate extended to cover it", in a list headed
  "six shapes have no query at all". Both halves are now delivered: `pbench/decimal15-key-group`
  and `decimal38-key-group` are on at all five device modes, and the gate carries p ≤ 18 and
  p > 18 plus a decimal-then-string composite. The branch repointed the bullet's `#95` link to the
  archive and left the bullet, so #195 still counts six.

### `architecture.md`, the sentences this branch falsified

Correction only; three, and all three are outside the hash section the branch already extended.

1. **"Interfaces"**, `:1009-1011` — "**[`peacock::partitioning`]** — the second public header:
   `spark_partition_ids` and `spark_hash_partition`, **our own bit-exact Spark-murmur3 at seed
   42**, because cuDF ships only standard murmur3." Bit-exact with Spark is what three key types
   deliberately are not since this branch. Correction: bit-exact with the cpu's lane rule at seed
   42, with the three divergences in [Rehash and the comet hash](#rehash-and-the-comet-hash).
2. **"Interfaces"**, `:963-964` — "two test hooks: `peacock_spark_partition_ids`, which runs the
   murmur3 kernel over one Arrow C-data batch **so the Rust side can compare it against
   comet's**". #201's whole point is that the comparison is now against production
   `rows_per_lane`. Correction: "so the Rust side can compare it against the production lane
   rule".
3. **"Every cast is explicit"**, `:379-387` — "**Four stay in C++ with a reason.** … Hash key
   normalization feeds the hash alone and never reaches a returned value." Hash key normalization
   is now on both engines — the cpu half is the decimal widening, the NaN canonicalization and the
   unsigned widening in `spark_partitioning::hash_keys` — so it is not one of the four that stay
   in C++. The reason given (a cast that cannot change an answer needs no plan node) is still the
   right one and should stay; what moves is "in C++" → "outside the plan, once per engine".

Nothing else on the page is untrue. Two sentences the branch made true rather than false, checked
rather than assumed: "the merge groups on keys + gid and the shuffle still hashes the keys alone"
(`:288-290`) and "A rollup's last set masks every key, so those rows hash on nothing and land in
the single lane `pmod(seed, N)`" (`:313`) — before the drop the gid was a non-null constant in the
hash and neither held.

### Verified sound, so nobody re-derives it

- **77 enabled gpu cells, 77 `gpu-result.txt` sections** (pbench 51, tpch 22, tpcds 4) and 703
  enabled cpu cells; 177 active lines, 138 at all five cpu modes, 6 partial, 33 out. Every
  `build-test.md` figure I recomputed matches: `--lib` rows, `test_cpu_corpus` 967/`Corpus, cpu`
  966, 703 cells, 144 running lines, murmur 19, `emit_cases` 33, `emit_schema_cases` 21,
  `Harness helpers` 21, `Rollup's shuffle keys` 3, `Arrow to the wire enum` 3, `The lane rule's
  own properties` 1, `Translator` 30, `Expression writer` 18.
- **No cell is left under an archived ticket by this branch.** All 25 rows that carried
  95/189/201/206/240 were re-examined and the tag struck; the registry's remaining archived tags
  are #32, #143, #183 and #187, every one pre-existing (#183 and #187 are `stale-cells`'s 16
  cells). No row has an off cell and no ticket.
- **`tickets.md` is consistent in three directions** for all eleven files — declared count =
  listed IDs = file anchors, in the same order, summing to 117. Five anchors added to the archive,
  none removed, none duplicated. No link anywhere still points at the five archived tickets'
  old paths.
- **One production caller of the lane rule** (`cpu_backend/emit.rs:57`), one gate, one rust-only
  test. **`spark_hash_partition` delegates to `spark_partition_ids`**, so the normalizing switch
  has no second copy on the production scatter path — the shape #201 was about, one level up.
- **The exec model is not a third copy**: `operators/partition_ops.py` uses crc32 by design and
  says so, and no emitter in `tpch.plans.txt`/`tpcds.plans.txt` hashes `__grouping_id`, so #189's
  drop leaves it in step.
- **Date64 agrees by accident and it is worth knowing**: the new `TIMESTAMP_MILLISECONDS` arm
  makes a `Date64` key hash as its i64 ms, and comet has a `Date64` arm doing exactly that
  (`hash_funcs/utils.rs`), so a type nothing gates now works on both engines where the device used
  to refuse. `Float16`, `Binary` and `LargeBinary` are the only members `convert_data_type` names
  that the kernel has no arm for, and `fb_to_type_id` maps all three to `EMPTY`, so the device
  cannot hold them at all — out of this task's reach, not a gap in it.
- **No device item was filed as deferred that the host override does not defer.** The detail's
  deferred list is exactly the override's four (the sf40 pair, `--run-benchmarks`, Nsight, H200)
  plus cuDF 26.02, which the override itself assigns to 25.02 and which chain J's `verify-26.02`
  owns. The two non-override holds are each demonstrated rather than asserted — the #245 pin by a
  measured `common.rs:66` panic that #255 cites by line, the float rows by the spec's own
  Restriction — and each sits on a live ticket.

### One stale line in this file, for whoever writes the signoff

§"Left undone, deliberately" (above) says `reports/corpus-fixes.md`'s D2 "is still written as an
open decision", and the impl plan's Task 10 lists it as owed. Commit `a75ecf08` rewrote D2. D2 is
done; what is left there is fix 12 (I3). Do not carry D2 into the signoff as a shortcut.

## Completeness pass, and what it found twice (2026-10-09)

Two readings at `caec7e1b`, dispatched together. **The reviewer: 1 blocking, 5 important. The
analyst: 2 blocking, 5 important, plus three falsified `architecture.md` sentences.** The analyst's
own section is above, and it disclosed that the reviewer's list would overlap on its B1 — it did,
which is the second time in this chain that the finding both readings arrived at independently was
the real one.

**Neither found anything wrong with the code.** Between them they re-derived: all 703 `.cost.txt`
sections byte-exactly from their `.cpu.txt` siblings; conservation over 67,443 per-lane checks;
loader batches against `partition_groups` over 4,328 nodes; the registry against the corpus on both
engines; every `build-test.md` count, the grand total 2975 equalling the sum of all 85 N-column
rows; the three divergences read side by side against comet 0.6.0's own dispatch arm for arm; and
#201's closure, including that no fourth implementation of the rule exists anywhere in the tree.
`gpu-result.txt` holds exactly 77 sections against 77 enabled cells, set-equal with no extras.

### The blocking pair, both prose, both mine

**`build-test.md`'s device-corpus paragraph was the pre-task text.** The table count beside it had
been re-summed to 78 and the paragraph had not moved at all. Five false statements, and the worst of
them named three tickets this branch had just archived as live blockers, and said the seven key-type
groups run "at the two tp1 modes, where no shuffle hashes the key" — which is the negation of what
the task did. Rewritten from the registry: 77 cells, pbench's 51 at every mode, #152 and #220
holding over 900 between them, and #264 named.

**Both public C++ headers still stated the comet-identical contract.** `partitioning.hpp:1`
("comet-identical") and `:30` ("asserts against comet's CPU twin"), and `peacock_gpu.h:278` ("the
REAL comet CPU helper"). Every `.cu` comment had been updated and the headers never touched —
and `architecture.md` points a reader at `partitioning.hpp` as the place to look. Both now say what
the rule actually is, with the three departures named in the header itself.

### The correction that matters most, because it would have been acted on

The analyst checked Spark's own `Murmur3Hash` and found **the paragraph I wrote named the wrong
counterparty for two of the three rules.** Spark hashes a float through `doubleToLongBits`, which
already collapses every NaN to the same bit pattern this branch canonicalizes to — so the NaN rule
**restores** Spark's placement, and the departure is from comet, which hashes raw `to_le_bytes()`.
And `-0.0` folding to `+0.0` is Spark's rule and comet's both, not a departure at all. The unsigned
rule has no Spark counterpart to depart from. Only the decimal rule departs from Spark.

That matters more than the four prose findings around it: a reader told "we deliberately left Spark
here" could reverse the canonicalization believing raw bits are Spark's rule, which is exactly the
tp4 lane split the task fixed. Corrected in `architecture.md`, in `emit_cases.rs`'s case comment,
and in `partitioning.hpp`'s new paragraph.

### The rest, applied

`architecture.md`'s three falsified sentences, all outside the section the branch had extended:
"our own bit-exact Spark-murmur3" (bit-exact with the cpu's lane rule, not Spark), the conformance
hook comparing "against comet's" (against the production rule, which is #201), and "four stay in
C++" listing hash key normalization, which is now on both engines. `corpus_cases.inc`'s device-column
comment named #95 as a live cause in the present tense. #95's archived body justified the eight
dropped tags with a reason false for two of them — `tpcds/q37` and `q82` have `gpu_tp1_single`
enabled, so #95 *was* a real tp4 blocker there; corrected in the one artifact nobody revises later.
#249's header still ended "and writes such a field as `Null`", contradicting its own first
paragraph. #195's "no query at all" bullet for a decimal shuffle key, now delivered. And
`corpus-fixes.md`'s fix 12 was still the rejected design in full with D2 pointing at it — marked
superseded at its head, since D2 says "neither append was taken" and fix 12 said which to append.

**The two hand-off facts this round bought are now where they survive the merge**, which was the
analyst's point and #259's exact shape: `build-test.md`'s regeneration list gains the
registry-moves-first rule — `declared_sections` reads the CSV, not the `.inc`, so a golden for a
cell the CSV still calls `disabled` is not written and the case reports `ok` anyway — and the
`git diff --numstat` habit after a scripted CSV edit. `join-backend-impl.md`'s Task 14 step 1, which
edits that file in a dozen batches, now carries the ordering in the step itself.

### One important finding discharged by the signoff rather than by code

The task's central invariant — the shuffle hashes every key type the planner emits — rests on 19
hand-written gates and 21 harness cases, and **nothing fails by count** when `convert_data_type`
gains a member, so the next key type can land unproven on both engines. The tree owns the pattern
one file away (`Literals.EveryWireTypeEitherMakesAnAstLiteralOrSaysWhyNot` asserts its case count
against `EnumValuesDataType()`, and that assertion is what caught this branch's own fbs append).
The analyst's own judgement was that a signoff line discharges it as well as code would, since the
failure mode is a refusal and not a wrong answer, and adding a guard here would reopen a finished
task. Named in the signoff, with the pattern to copy.

## Blocked at completeness approved: the cost gate fires, and only a human can accept it (2026-10-09)

Everything else is done and signed off. CI's **Cost report** job fails its last step, the PR-only
cost-regression gate, and that job is not one the host override exempts — so `done`, which asserts
the PR is green, cannot be written.

```
cost-diff: 679 compared, 12 changed, 6 regression(s)   exit 1
```

**Bisected across this branch's own runs**, so nobody need redo it: green on `1a1d03b6` (the
rebase), red from `74da1059` (the device half) onward, and nothing since has touched a cost input.

**Why four readings missed it.** The round ran `scripts/cost-report-preview.sh`, which renders and
exits 0 and does not take a base SHA. Two review rounds and two completeness readings re-derived
all 703 `.cost.txt` sections byte-exactly from their `.cpu.txt` siblings — which proves the
derivation is right and says nothing about whether the total went up. The gate is the only thing in
the tree that asks the second question, and it runs on a PR alone.

### The six, and the one sentence that settles them

| section | base Σout | PR Σout | Δ |
|---|--:|--:|--:|
| `tpch/q2` tp4-rowgroup | 392,944,650 | 392,944,816 | **+166** |
| `tpch/q2` tp4-single | 394,695,770 | 394,695,804 | **+34** |
| `tpch/q2` tp4-sized | 392,898,679 | 392,898,713 | **+34** |
| `tpch/q10` tp4-rowgroup | 653,524,699 | 653,524,762 | **+63** |
| `tpch/q10` tp4-single | 653,445,902 | 653,445,965 | **+63** |
| `tpch/q10` tp4-sized | 652,518,141 | 652,518,204 | **+63** |

Against six improvements — `pbench/decimal15-key-group` −4 and `decimal15-key-join` −14 at each tp4
mode. **+423 bytes of regression and −54 of improvement on 3.17 GB of compared cost, and no row,
node, lane count or answer changed anywhere.** Worst single ratio +0.000042%.

**Benign and inherent to D2, tested rather than assumed.** `output_rows` unchanged in all twelve;
node skeletons identical element for element; `lanes=4` on both sides; the only answer-golden
changes on the whole branch are `mode=` author lines. For every moved figure but two,
`Δbytes = columns × Δ(Σ ceil(rows_lane/8))` — validity-bitmap rounding to the byte. The two
exceptions are `GpuSort` nodes whose per-lane top-N now holds a different *set* of rows at the same
row count, so the variable-width string content differs; the merged answer is unchanged. And
exactly one shuffle moved per query, always the only one whose key list holds a decimal of
precision ≤ 18 — `decimal38-key-group` did not move at all, because at p = 38 the cast is a no-op.
That is the cast's fingerprint and nothing else produces it.

**Unavoidable, not merely accepted.** Keeping comet's 8-byte width at p ≤ 18 needs the device to
know the precision and it cannot — cuDF's `data_type` carries none and the loader widens every
decimal to `Decimal128` — which is why D2 refused the wire field that would carry it. Dropping
those four columns from the shuffle key list would change which keys DataFusion picks, which the
Restriction forbids. And `hash_keys` has no incidental cast to strip: it maps only over
`hash_exprs`, matches only `Decimal128(p, _) if p < 38`, and the widened array reaches
`create_murmur3_hashes` alone, never the data flow.

### What the gate actually measures, which changes how the number reads

`DiffRow::is_regression()` (`cost-report/src/main.rs:1262`) is `self.new > self.old` — **any
increase of one byte**, no tolerance and no magnitude. The 1.4 threshold `build-test.md` names
beside it is `RATIO_GREEN_MAX`, used only by the coverage report's peacock-over-DuckDB bucket;
`DiffRow` has no `bucket()`. The two are one row of a CI table and two mechanisms. So "6 of 12
changed" is the weakest possible trigger and here it fired on 34 to 166 bytes.

**The artifact the human is asked to judge shows them no magnitude.** `fmt_delta` renders
`{:+.2}%` and both Σout columns render human-readable, so all twelve rows of the PR comment read
`623.25 MB | 623.25 MB | 🔴 +0.00%`. Every figure is identical on both sides and the delta displays
as zero. That is a defect in a rendered artifact rather than in the engine, so it is not a ticket
by the standing rule — but it is why the table above is in this file: it is the only place the
numbers exist.

`architecture.md` gained the one sentence that would have made this legible in a minute: a cost
total is lane-split dependent, because `output_bytes` carries per-batch padding, so any change to
the lane rule moves it even where no row, node or lane count does — and the gate fires on a byte.

## Rebased onto the rebased pbench, and the block cleared by the human (2026-10-09)

`git rebase --onto ENS-pbench d39fdded ENS-repartition-keys`, 18 commits, no stop. Method and why
it is cheap: `duckdb-oracle-detail.md`, "The rebase, executed". The whole chain moved this run —
duckdb-oracle onto master `f0a6ecbf` and back to `done`, pbench after it and back to `done`.

**The block is cleared, and not by me.** `f0a6ecbf` adds this to the chain's host override:
"Cost regressions are accepted for repartition-keys, join-session-cpp and join-backend… for
repartition-keys, the +423-byte lane-split regression its developer diagnosed." The human named the
exact obstacle the block was written for, so the block is theirs to have cleared and is cleared.
Two things follow. The same commit takes the gate itself to a 10% tolerance
(`REGRESSION_FAIL_PCT`), so at +0.000042% the gate should now pass on its own rather than being
waived — but the rebase moves the base it diffs against, so it has to be **re-measured, not
assumed**. And the override asks for the regression to be named in the PR as well as here, which is
a comment on #169 the coordinator owes.

### The intersection, and why the goldens were resolved differently

This branch touches 74 files, the new parent moved 54, and they overlap in **twenty**: the six
`llm-wiki/*.md`, and **fourteen goldens** — eight of pbench's and six tpcds `tp4-*`. The 54
non-intersecting files came across byte for byte.

Goldens are not textually mergeable, and the three-way merge for them is the wrong question
anyway: master's `64ced62e` moved the plan-text renderer, so **every plan golden on this branch was
written by the old renderer** whether it conflicted or not. So the fourteen were resolved
mechanically — a one-line `merge=takebranch` driver (`cp %B %A`) attached to those paths alone,
taking the replayed commit's side every time — and all fourteen were then verified byte-identical
to the pre-rebase branch. That is deliberately the wrong content, and **the regeneration round
below is what makes it right**; a stale golden is a red test, which is the net under it. Nothing
else in the tree got that treatment: the driver was attached by path, and a conflict anywhere else
would have stopped the replay.

The six markdown files cost three hunks in `tasks.md`, three in `tickets.md`, two in
`build-test.md`, and nothing at all in the other three — `archive/archived-tickets.md`,
`tickets/complete-coverage.md` and `tickets/corpus-coverage.md` all merged clean, which is the
rebase's own evidence that this branch's five archivals do not collide with master's.

- **`tasks.md`**: parent's numbering and the two states below this one (duckdb-oracle and pbench are
  both `done` now), and **this branch's link targets** for its own five tickets — #201, #206, #240,
  #95 and #189 point at `archive/archived-tickets.md` here because this branch archived them, and
  master's copy still points at `corpus-coverage.md` because master has not seen that yet.
- **`tickets.md`**: next free **265** (this branch filed #264), keeping master's sentence that it
  reserved 264–279 for this chain and went on to 284 itself. Rows re-derived from the ticket files'
  anchors: **123 open over 11 rows**, every row agreeing with its file in count and order, header
  equal to the sum. The drop from pbench's 127 is this branch's five archivals plus #264.
- **`build-test.md`**: derived from the page's own two tables, not by adding deltas. **2981 — Rust
  2483, C++ 97, Python 401**; cpu 1692 (`--lib` 696, `test_cpu_corpus` 967, 26, 3), ffi 7, gpu
  **671** (580, 79, 11, 1). Every tier heading equals its rows and its own sub-breakdown. These are
  declarations to check against `--list` in the round below — the pbench round found one of them
  off by one.

## The re-prove on the rebased pbench, and the fourteen goldens regenerated (2026-10-09) — green

Everything green, both engines, on the exact tree left in the working directory. **The cost gate
now passes on its own**, measured rather than assumed: `cost-diff: 679 compared, 12 changed, 6
regression(s), 0 over 10%`, **exit 0**. The working tree holds **fourteen changed files and nothing
else** — all goldens, every one a regeneration, no code.

### The fourteen were predicted and then found, not assumed

The regeneration set was taken from the tests, not from the rebase note. Verify mode first:

- `cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens
  --test-threads=1` → **18 passed, 8 failed**: the five pbench `.plans.txt` and the three tpcds
  `tp4-*.plans.txt`. tpch's did not move at all, as predicted — its `projection=` join lines are
  `LeftSemi`/`LeftAnti`, which `projection_field` is a no-op for.
- `--test test_cpu_corpus -- --test-threads=2` → **937 passed, 30 failed**, and the 30 are
  10 queries × the three tp4 modes: pbench `finish-without-probe`, `not-not-in`, `not-or-not-in`,
  `sparse-build-anti`, `sparse-probe-semi`, and tpcds `q10`, `q35`, `q45`, `q58`, `q83` — exactly
  the five families master's `64ced62e` named plus pbench's own five.

That is **8 files under `pbench.sf1/` and 6 `tp4-*` under `tpcds.sf1/`**, which is the fourteen the
rebase note predicted. Nothing else was stale, and in particular **pbench's `tp1-*-mini.cpu.txt`
were already right**: this branch never edited them, so they came across from the parent carrying
pbench's own regeneration. Only files this branch had edited were takebranch'd, and only those lost
the renderer update.

### Regenerated, and what moved

`.plans.txt` in one call, `UPDATE_CANONICAL=1 … --lib -- planner::tests::plan_goldens
--test-threads=1` → 26 passed, 8 files written. `.cpu.txt` one query at a time,
`PCK_UPDATE_SECTIONS=1 … --test test_cpu_corpus <query> -- --test-threads=1`, ten runs, 5 cases
each, rc=0 every time — no section lost, which #213 is the reason to check.

| file set | changed lines each | what moved |
|---|--:|---|
| 5 × `pbench.sf1/*.plans.txt` | 5 | 5 `projection=[…]` name lists |
| 3 × `pbench.sf1/tp4-*-mini.cpu.txt` | 5 | the same 5, in the execution tree |
| 3 × `tpcds.sf1/tp4-*.plans.txt` | 12 | 11 `projection=[…]`, **+1 literal**: q90's `` `None,23,8` `` → `NULL` |
| 3 × `tpcds.sf1/tp4-*-mini.cpu.txt` | 11 | 11 `projection=[…]` |

Every file is line-for-line balanced in `git diff --numstat` (N added = N removed). **No `.cost.txt`
moved, no `.result.txt` moved, `recipe-payloads.txt` did not move** — `UPDATE_CANONICAL=1` alone
verifies it, and `the_payload_golden_carries_what_each_call_hands_the_executor` is green.

The one non-projection line is the **second arm of `64ced62e`**, `literal_text`'s null
`Decimal128`/`Decimal256` case, and it is renderer text on its face: the node is the same
`GpuProject`, the expression the same `CASE`, the schema still `[am_pm_ratio:Decimal128(23,8)]`.

### The mechanical checks, five of them, and the one that is master's own

1. **Mask `projection=[…]` and diff the rest.** Clean on all five pbench `.plans.txt` and all six
   `.cpu.txt` — **zero lines**. On the three tpcds `.plans.txt`, exactly **one** line each, the q90
   literal above. So no node name, no indentation, no other field moved anywhere.
2. **The `@N` ordinal sequence inside the projection fields is byte-identical** in all fourteen
   files — 47 ordinals per pbench plan file, 3703 per tpcds one. Only the names left of the `@`
   changed, which is what the new `projection_field` does: it still emits `ordinal`, and takes the
   name from `name_at(output, position)` instead of from build++probe.
3. **Every measured quantity in the six `.cpu.txt` is byte-identical**, key by key:
   `output_rows=`, `output_bytes=`, `in_rows=`, `batch_rows=`, `batch_bytes=`, `lanes=`,
   `batches=`, `early_exit=`, `join_type=`, `hashed_on=`, `hash=`. 243 occurrences of each of the
   first five per pbench file, 5389–5925 per tpcds one; 216 / 5314–5850 `lanes=`. Section counts
   unchanged (57 pbench, 81 tpcds, 60/99 in the plan files). **That is why `.cost.txt` could not
   move**, and the reason is checkable rather than asserted.
4. **The new names are the node's own output schema, in order.** Over the 58 changed
   `projection=` lines in the `.plans.txt` files — the only goldens that carry a `schema=[…]`
   field — the name list equals the schema field list **58 of 58**. Under the *old* text it
   agreed **0 of 58**. The old goldens were wrong and are now right; this is not a cosmetic
   re-spelling. For the `.cpu.txt` files, which carry no `schema=`, the equivalent holds: every
   one of the 3 / 9 distinct new projection fields per file appears verbatim in its sibling
   `.plans.txt`.
5. **Master's own pin agrees.** `planner::tests::join_projection_names::`
   `every_join_projection_in_a_golden_names_the_column_its_ordinal_selects` — written by
   `64ced62e` to hold this rule over the whole corpus — is **green** on the regenerated tree. It
   is the authoritative form of check 4 and it did not need writing.

And the join types: **every** changed projection line is `LeftMark`, `RightSemi` or `RightAnti` —
pbench 1/3/1 per file, tpcds 3/8 per file — and no other join type moved in any golden. That is
`projection_field`'s predicted blast radius exactly.

**Two independent cross-checks against the two parents**, because both regenerated some of these
files themselves:

- vs **master `f0a6ecbf`**: the three tpcds `tp4-*.plans.txt` now have a `projection=` field set
  **byte-identical to master's**. Their `.cpu.txt` siblings differ by **45 additions and zero
  removals**, and the reason is this branch's own work — master has 10 `skipped:` sections where
  this branch has 6.
- vs the parent **`ENS-pbench` 09ab965b**: the five pbench `.plans.txt` projection sets are
  **identical** to the parent's. The three `.cpu.txt` differ by **one addition and zero removals**
  (`projection=[d_id@0, f_id@2]`), again a cell this branch turned on — 33 `skipped:` → 30.

Nothing was removed or renumbered anywhere. **No moved section, no moved row count, no moved node,
no changed `lanes=`, no changed answer.**

### The proving set, in full, with the command that produced each figure

A rebase onto a moved renderer plus a golden regeneration, so the subset shortcut does not apply.
`cargo test --no-run --features rust-only -p peacockdb-core` from cold: **exit 0, zero warnings**.
Every command under `timeout`.

**cpu, local, `--features rust-only` into `./target`, `--test-threads=2`:**

| target | measured | `build-test.md` | |
|---|--:|--:|:-:|
| `test_cpu_corpus` | **967** passed, 0 failed | 967 | ✓ |
| `--lib` | **696** (694 passed, 0 failed, 2 ignored) | 696 | ✓ |
| `test_corpus_goldens` | 26 | 26 | ✓ |
| `test_cost_model` | 3 | 3 | ✓ |
| `test_golden_format` | 43 | 43 | ✓ |
| `test_module_layout` | 18 | 18 | ✓ |
| `test_ci_coverage` | 11 | 11 | ✓ |
| `cargo test -p cost-report` | 41 | 41 | ✓ |
| `generate_pbench.sh --check` | `pbench.sf1 matches gen.sql` | — | ✓ |

The two ignored are #182's pair, as before. `test_corpus_goldens` and `test_cost_model` together
are the proof the regeneration kept the derivation: the first reads every committed section against
its own arithmetic, the second re-derives all 703 `.cost.txt` sections from their `.cpu.txt`
siblings.

**ffi, local, cuDF 25.02 via `scripts/cargo-cudf.sh`:** `peacockdb-ffi --test test_ffi` **3 passed**.
The binary exits **127** under `cargo test -- --list` until `LD_LIBRARY_PATH` carries
`target-cudf-*/debug/build/peacockdb-ffi-*/out/lib` and the cuDF root's `lib`, exactly as
`build-test.md` warns; run directly with those set it lists 3 and passes 3.

**C++ cpu, local.** `cpp/build` in this worktree is still an empty root-owned directory, so the
build is configured in `/tmp/dkb-cppbuild` (left over from the device round and still valid —
`CMAKE_HOME_DIRECTORY` points at this worktree's `cpp/`). `ninja peacock_cpu_tests` → 18/18,
**zero warnings**; `ctest -L cpu` → **1/1 passed**; the binary direct → **15 tests from 7 suites,
PASSED 15**.

**python, local.** `matplotlib` is absent on this workstation and `test_plot.py` imports it at
module scope, so it was installed with `pip3 install --target /tmp/dkb-pydeps` and reached by
`PYTHONPATH`; the system environment is untouched.

| set | measured | `build-test.md` | |
|---|--:|--:|:-:|
| `testdata/test_duckdb_cost.py` | 41 | 41 | ✓ |
| `testdata/test_duckdb_result.py` | 20 | 20 | ✓ |
| `scripts/exec_model/tests/` prototype, CI's exclusion list applied | 17+4+18+21+23+63+24+22+15+9 = **216** | 216 | ✓ |
| `scripts/calibration/tests/` | 5+4+3 = **12** | 12 | ✓ |

**device, nebius-gpu L40S, cuDF 25.02, each staged binary run directly**, `--test-threads=1`,
`PEACOCK_TESTDATA_DIR=$PWD/testdata`. Card idle at 0 MiB of 46068 before the run; 37 GB free on `/`
after, nothing cleaned. The device build emitted **zero warnings**. The tree was rsynced
(`--delete-after`) **after** the regeneration, and the 14 regenerated goldens plus
`recipe-payloads.txt` were then **sha256-compared local against remote and are identical** — so
the device verified these exact bytes.

| binary | measured | `build-test.md` | |
|---|--:|--:|:-:|
| `peacock_gpu_tests` | 2 passed | 2 | ✓ |
| `peacock_plan_tests` | 58 passed | 58 | ✓ |
| `peacock_cpu_tests` (on the card too) | 15 passed | 15 | ✓ |
| `test_gpu_corpus` | **79** passed, 0 failed | 79 | ✓ |
| `test_node_timing` | 1 passed | 1 | ✓ |
| `peacockdb_core_gpu_lib gpu_tests::` | **580** passed, 0 failed | 580 | ✓ |
| `peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered | 11 (3 are `bench_`) | ✓ |
| of which `murmur_conformance` | **19** | 19 | ✓ |

`the_registry_matches_the_gpu_corpus_in_both_directions` is green, and so is
`the_registry_matches_the_cpu_corpus_in_both_directions` in the cpu log — the pair agrees on both
sides after the rebase. This branch's device half is intact: 21 device cells, the murmur gate at
19 cases, every kernel arm.

### The twelve cost sections, re-derived, and the gate's answer

The gate is reproducible locally and **the preview script is not the way**: the binary takes
`--cost-diff --base REF|DIR` and `base_total` resolves a ref with `git show <base>:<path>`, so
from this worktree:

    cargo run -q -p cost-report -- --cost-diff --base 09ab965b --html … --md-diff …
    cost-diff: 679 compared, 12 changed, 6 regression(s), 0 over 10%      exit 0

**The regression did not move.** Every one of the twelve is byte-for-byte the figure the
"Blocked at completeness approved" table records, against the **new** base:

| section | base Σout (`ENS-pbench`) | PR Σout | Δ |
|---|--:|--:|--:|
| `tpch/q2` tp4-rowgroup | 392,944,650 | 392,944,816 | **+166** |
| `tpch/q2` tp4-single | 394,695,770 | 394,695,804 | **+34** |
| `tpch/q2` tp4-sized | 392,898,679 | 392,898,713 | **+34** |
| `tpch/q10` tp4-rowgroup | 653,524,699 | 653,524,762 | **+63** |
| `tpch/q10` tp4-single | 653,445,902 | 653,445,965 | **+63** |
| `tpch/q10` tp4-sized | 652,518,141 | 652,518,204 | **+63** |
| `pbench/decimal15-key-group` tp4-rowgroup | 534,208 | 534,204 | −4 |
| `pbench/decimal15-key-group` tp4-single | 490,408 | 490,404 | −4 |
| `pbench/decimal15-key-group` tp4-sized | 490,408 | 490,404 | −4 |
| `pbench/decimal15-key-join` tp4-rowgroup | 8,094,652 | 8,094,638 | −14 |
| `pbench/decimal15-key-join` tp4-single | 8,094,592 | 8,094,578 | −14 |
| `pbench/decimal15-key-join` tp4-sized | 8,094,592 | 8,094,578 | −14 |

**+423 of regression, −54 of improvement**, worst single ratio **+0.0000422%** (`tpch/q2`
tp4-rowgroup). Re-derived twice: once by the real binary, once by an independent reimplementation
of `cost_diff`/`fails_gate` over the same `peacockdb_cost=` totals — both give 679 / 12 / 6 / 0.
The six tpch base figures are the same at master `f0a6ecbf` as at `ENS-pbench`, so the rebase moved
neither side. pbench's six have no master baseline at all — those `.cost.txt` files are pbench's
own new files — and they are improvements, which never fail a gate.

**One number in this file to correct.** "on 3.17 GB of compared cost" is the base sum of the
**twelve changed** sections (3,165,826,701 B). The sum over all **679 compared** is
**440,198,043,713 B — 440.20 GB**. The regression is +423 bytes in 440 GB, not in 3.17 GB; the
sentence makes it read about 140× larger than it is.

**The rendering defect the last round named reproduces exactly.** All twelve rows of the markdown
read `… | 623.25 MB | 623.25 MB | 🔴 +0.00%` — both Σout columns human-readable and identical,
`fmt_delta`'s `{:+.2}%` rounding every delta to zero. Still not a ticket by the standing rule, and
still the reason this table exists here.

### `cost-report-preview.sh --base <sha>` — small, and worth doing in a later round

`cost-report-preview.sh` renders the **coverage** report: `cargo run -q -p cost-report -- --sha
$(git rev-parse HEAD) --html "$out"`. It never passes `--cost-diff`, so it cannot answer the gate's
question whatever it is given — that is the hole four readings fell into, and it is in the script,
not in the binary.

The change is about four lines and no new code in `cost-report`: accept `--base <sha>`, and when it
is present run the binary a second time with `--cost-diff --base "$base" --html
"${out%.html}-diff.html" --md-diff "${out%.html}-diff.md"`, propagating its exit status. The
default `<sha>` a developer wants is the PR's base, which locally is the parent branch's head
(`git merge-base HEAD <parent>`), so a bare `--base` could default to that. **Not built this
round** — this round restores a finished task and takes no new work.

### `build-test.md`'s counts, every one measured

**All of them are right.** No drift, unlike the pbench round. Measured by `--list` /
`--gtest_list_tests` wherever a binary exists here:

    cpu   1692 = --lib 696 + test_cpu_corpus 967 + test_corpus_goldens 26 + test_cost_model 3
    ffi      7 = --lib -- ffi_tests:: 4 + peacockdb-ffi --test test_ffi 3
    gpu    671 = --lib -- gpu_tests:: 580 + test_gpu_corpus 79 + peacock_gpu_benchmarks 11
                 + test_node_timing 1
    else   113 = test_golden_format 43 + test_ci_coverage 11 + test_module_layout 18
                 + cost-report 41
    Rust  2483 = 1692 + 7 + 671 + 113
    C++     97 = (15 + 2 + 58 + 4 + 4) + (4 + 1 + 4 + 4 + 1)
    Python 401 = (41 + 20 + 216 + 12) + (19 + 93)
    TOTAL 2981 = 2483 + 97 + 401

The whole device lib reconciles independently: `peacockdb_core_gpu_lib --list` is **1280**, and
696 + 4 + 580 = 1280 — the cpu, ffi and gpu rungs of one binary, each listed under its own path
filter. `--lib` is **696** and not the 692 the last round measured: master's `64ced62e` brought
`join_projection_names` and the `plan_text` cases with it, and the rebase's declared 696 is right.

Three groups could not be run here and were counted from the source instead, each named so it is
not mistaken for a measurement: the **C++ manual tier** — `TEST`/`TEST_F` counts of
`test_tpch_streamed.cpp` 4, `test_cudf_nodes.cpp` 1, `test_multi_gpu_tpch.cpp` 4,
`test_multi_gpu_tpchv.cpp` 4, `test_basic_multi_gpu.cpp` 1 = 14, binaries the gate script does not
build; `test_tpch.py` **19** `def test_`, which needs the generated sf1 and rides dataset-matrix;
and the **exec-model corpus 93** = `test_tpch_corpus.py`'s 22 `def test_` plus `test_tpcds.py`'s
**71** generated from `plans_tpcds.QUERIES` (its own comment says "seventy-one copies"), both
manual-dispatch only. `peacock_tpch_tests` and `peacock_tpchv_tests` **are** staged on the card and
`--gtest_list_tests` gives **4** and **4**, so those two rows are measured even though the sf40
dataset is absent.

### Deferred by the chain-wide host override, unchanged from the device round

- **the sf40 pair**, `peacock_tpch_tests` / `peacock_tpchv_tests` — listed (4 + 4) but **not run**:
  69 GiB against the L40S's 46 GB, and no `tpch.sf40` on nebius-gpu.
- **`--run-benchmarks` and the three `bench_` cases** — not run; the 3 filtered in
  `peacock_gpu_benchmarks` are exactly those.
- **Nsight captures** — not taken. **H200 timing** — not taken; no H200.
- **cuDF 26.02** — not built; red there for #260 and #94, outside this chain.
- **shad-gpu** — never touched: no `--run`, no `--pull-results`. **verda** — unreachable,
  `VERDA_CLIENT_ID` unset, so every CPU run is local.
- **The C++ manual and 2gpu tiers** (14 cases) — not built here; counted from source above.
- **The two dataset-dependent Python sets** (19 + 93) — not run; counted from source above.

### Two workstation traps for the next developer

- **`cpp/build` is still an empty root-owned directory** in this worktree, and cmake dies at
  configure with `Unable to (re)create the private pkgRedirects directory`. Removing it needs sudo.
  `/tmp/dkb-cppbuild` is a working configure against this worktree's `cpp/` and survives between
  rounds.
- **A previous round left `/tmp/struct.py`** — a scratch cost-skeleton script. Any python script
  run *from `/tmp`* gets `/tmp` as `sys.path[0]`, so `import struct` picks that file up and
  **executes it**, printing its output into the middle of yours and then dying inside
  `gzip`/`matplotlib`. It cost one confusing `pip3 install` failure and one run of output that
  looked like it came from nowhere. Keep scratch python in a subdirectory (`/tmp/dkb/`), not in
  `/tmp` itself.

### Verdict

Green on every tier that the host override leaves runnable, and the cost gate — the one thing the
block was written for — **passes locally against the new base with 0 of 6 regressions over the 10%
tolerance**. No finding that is a defect: every red was a stale golden, and all fourteen are
regenerated with the renderer's own rule and master's own pin agreeing. The working tree holds
those fourteen files and nothing else; no git state was touched.

### The block cleared, and on what

Two independent grounds, either of which would do.

**The human cleared it.** `f0a6ecbf` accepts "the +423-byte lane-split regression its developer
diagnosed" by name, in the chain's own host override, and says a red cost-report job does not block
`done` for this task. A coordinator never clears a block whatever its origin, and this one it did
not: the human named the exact obstacle the block was written for.

**And the obstacle is gone anyway.** The same commit takes the gate to a 10% tolerance, and the
re-prove round measured the real binary on the new base — `cargo run -p cost-report -- --cost-diff
--base ENS-pbench` reports `679 compared, 12 changed, 6 regression(s), 0 over 10%`, exit 0. So the
acceptance is not even being spent. The twelve figures are unchanged from the blocked round, and
the six tpch base figures are identical at master `f0a6ecbf` and at `ENS-pbench`, so the rebase
moved neither side.

The override asks for the regression to be named in the PR as well as here; that comment is on
#169.

So `blocked(completeness approved)` → `completeness approved`. What remains for `done` is CI.
