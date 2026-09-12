# walk-drives-every-plan — run record

Spec: [`walk-drives-every-plan.md`](walk-drives-every-plan.md). Plan:
[`walk-drives-every-plan-impl.md`](walk-drives-every-plan-impl.md). Branch
`ENS-walk-drives-every-plan` off `ENS-empty-build`.

### 2026-09-12 — building: plan task 0 dispatched, the decision paragraph

The task is conditional on the survey's report, which lives only on branch
`ENS-sink-divergence-survey` (`git show ENS-sink-divergence-survey:llm-wiki/reports/sink-divergence.md`).
An analyst writes the one paragraph the spec asks for and the task stops for the human. Two
things the spec predates, for that reading: task 10 made the walk's driver shared and drive the
sort kinds (`wire/gpu_tests/walk.rs`, `driven()` in `wire/gpu_tests/mod.rs`), so the refusal
list of "What it refuses today" has moved; and task 12 answered the empty-build pad, so the
spec's "Engine limits" row citing #175 now describes #212 (a build side that emits no batch at
all), #175 being archived.

### 2026-09-12 — plan task 0: the decision paragraph

Read at HEAD `ad152933` against `ENS-sink-divergence-survey:llm-wiki/reports/sink-divergence.md`
§4 and §"What this changes" › `walk-drives-every-plan.md`; the walk as it is now
(`wire/gpu_tests/walk.rs`, `driven()` in `wire/gpu_tests/mod.rs`); task 10's catalog
(`wire/gpu_tests/declared.rs`, `declared-schemas.md` signoff); task 9's operator harness
(`tests/gpu_tests/join_cases.rs`, `nested_cases.rs`, `aggregate_cases.rs`); the recipe arms
(`wire/attach.rs`, `wire/join.rs`, `plan/join.rs`'s capability matrix).

None of the spec's eight harness-gap shapes is worth teaching on the survey's evidence, and the
task should shrink to task 2 alone — a device refusal recorded rather than fatal — with task 3
dropped and tasks 1, 4 and 5 not started. The survey's verdict for this task was narrow: everything
that crosses the boundary is three classes the sink already reports in full, and the walk earns its
cost only for what does not cross — the `avg` count state (#163), the nullability flag, and the
cells that fail above the sink. Against the tree as it is now none of those three needs a shape
taught. The walk already drives every aggregate shape — init, merge, the self-finalizing init and
ROLLUP, at one and two lanes (`SUM_BY_FLAG`, `AVG_BY_FLAG`, `MAX_OF_SUMS`, `ROLLUP` in
`wire/gpu_tests/mod.rs`); what it cannot do is *measure* an aggregate's calls, because they carry no
declaration, and that is `declared-schemas-derived.md`'s work, not a harness gap — and task 9's
operator harness has meanwhile measured the count state per call on hand-built init and merge
nodes (`bug_a_welford_init_exports_its_count_as_int64`,
`bug_a_welford_merge_exports_its_count_as_int64`, #163), so the survey's one argument for buying
the harness has been bought. The nullability flag is the production exporter's `has_nulls()` and
the walk reads through that same export, so no teaching reaches it (task 10's query 8 recorded the
limitation). The above-sink refusals are task 2's: `walk.rs:127` still asserts `rc == 0` on
`execute_node` (`:98` on the scan, `:61` on `begin_plan` — `Session` moved into the walk file under
task 10, so the spec's "not in the walk file" is now false), so #45, #55, #189's device half (the
ticket says the CPU hasher refuses first, and the walk sends the repartition seq straight to the
device) and #203 abort the walk; #203 is why catalog query 11 is `#[ignore]` rather than `bug_`.
The export side is already recorded (`read_export` returns `Err`, proved by
`a_refused_export_is_returned_rather_than_panicking`), so task 2 is one function and three sites,
and it is what the `derived` spec's #45 and #189 rows need. The table below is why nothing else
survives: of the spec's eight gaps, three stand — `AccumulatedKeys` at `:225` (the unfiltered
semi-join finish), the bare call at `:296` (a mid-plan `LIMIT`), the export row range at `:489`
(a root-adjacent `LIMIT`) — four were never gaps (`:234`, `:335`, `:379`, `:495` guard invariants
no recipe arm can violate; task 10 declared *on* the call rather than adding one, so the "one call
per scan/sink" rows never stopped being true), and one is unreachable (`:408`: a limit's call is
`PerStraddlingBatch`, which `phases()` puts in `at_done`, so a limit trips `:296` first). No survey
class names a join or a limit, so the three that stand are worth teaching only when
`declared-schemas-derived` is rewritten and asks for them — and its blocker note that "most of its
queries are undrivable by the walk as it stands" is stale after task 10: six of its eleven query
rows drive today, three are device refusals task 2 makes observable, two need a join shape taught.
Of the spec's two engine limits, both are #152 and neither is #136 or #175/#212: the `:451`
finish-pass assertion refuses a shape the device runs for LeftSemi, LeftAnti and LeftMark
(`a_left_semi_join_over_two_probe_batches_agrees`, `left_anti_finishing_after_only_zero_row_probes_agrees`
drive the key project, the accumulated-keys concat and the finish on both backends — the recipe
*implements* #136's plan, it does not refuse under it), so that site is a harness gap except for
Left and Full, which #152 refuses at their first probe batch; `:469` is #152 for the six
build-copying kinds and a gap for the same semi family; and `driven()`'s cross/nested-loop arm
cites #152 for a shape the device runs at one probe batch (`a_cross_join_is_the_product_on_both`,
`an_inner_nested_loop_join_agrees`) while its Left/Full arm says "no shape here plans" for a shape
the device cannot run. #175 is closed by task 12 (`right_over_a_zero_row_build_pads_every_probe_row`
is green); #212 is the driver's `without_build` route for an upstream that emits no batch at all,
which a walk with no scheduler and one batch per lane never takes, so it is not a walk refusal and
the spec's row should not inherit it. Every #152 refusal the walk could pin is already pinned under
#152 in `join_cases.rs` and `nested_cases.rs` — thirty-odd rows of the Known-wrong table — so task 3
would be the second register the spec's own §3 forbids and is worth nothing; the only thing worth
doing at `driven()` is a comment fix so its three messages say what the operator harness proved,
and that can ride with task 2. If the human keeps nothing, the survey loses nothing it asked for.

**The refusal table, rebuilt by the spec's grep** (`panic!\|assert!\|assert_eq!\|unreachable!`
over `walk.rs`: 36 hits; `mod.rs`: 12; `declared.rs`: 26, all case assertions). Of the 48 in the
walk and its cases, ten are refusals — the spec's ten, moved — three are `rc == 0` aborts, and the
rest are ordinary assertions and test bodies. `driven()`'s three `Refused` arms are not grep-visible
and are listed last. Site numbers are `walk.rs` at HEAD unless said.

| site | message (its own words) | spec row → kind now, ticket | standing |
|---|---|---|---|
| `:225` `resolve` | `{input:?} is not a handle the walk holds` — `AccumulatedKeys`, `RowGroups`, `RowRange` | gap `:221` → **harness gap** for `AccumulatedKeys` (LeftSemi/LeftAnti/LeftMark without a residual: `ProbeKeys` per batch, concat at done); `RowGroups` and `RowRange` are dead here — `source()` and `unload()` call the session directly and never resolve them | stands, one input of three |
| `:234` `only` | `{what}: N handles rather than one` | gap `:230` → **invariant**: `make` caps every call but the repartition at one handle, and the repartition has its own arm; only a call answering zero handles could trip it, and none proven does | never a gap |
| `:296` `make` | `… takes runtime bounds rather than a seq, and no shape here plans one` — `Call::bare` | gap `:290` → **harness gap**: the mid-plan `GpuLimit` (`SliceHandle`, `PerStraddlingBatch`) — the sink's own bare call never reaches `make` | stands |
| `:335` `source` | `a scan's recipe is one call per batch` | gap `:309` → **invariant**: `attach.rs::scan` emits one call; task 10 put `output_schema` on that call, it added none | never a gap |
| `:379` `emit_partitions` | `{}: expected a repartition, got {other:?}` | gap `:350` → **invariant**: `EmitPartitions` is the only `PartitionEmitter` and plans one `Repartition` | never a gap |
| `:408` `per_lane` | `nothing runs at done … a streaming limit is the shape that reaches here, and none is planned` | gap `:379` → **unreachable**: `phases()` keeps only `PerBatch`/`PerProbeBatch` as streamed, so a limit's `PerStraddlingBatch` call sits in `at_done` and the limit fails at `:296` instead; every accumulator recipe has a non-streamed call | dead |
| `:451` `join` | `a finish pass accumulates probe keys across batches (#136), which no shape here plans` | limit `:422` #136 → **harness gap** for the unfiltered semi family (the device runs the finish: `a_left_semi_join_over_two_probe_batches_agrees`, `left_anti_finishing_*`, `left_mark_finishing_*`); **engine limit #152** for Left and Full (`bug_a_left_join_refuses_its_first_probe_batch_on_the_device`, the Full sibling). #136 refuses nothing | stands only as #152 for Left/Full, already pinned |
| `:469` `join` | `{} probe batches, and the call consumes the build handle with no ABI symbol to copy it (#152) — every shape here plans one probe batch` | limit `:441` #152 → **engine limit #152** for Inner, Right, RightSemi, RightAnti, Cross, nested-loop Inner (each pinned: `bug_*_refuses_its_second_probe_batch_on_the_device`); **harness gap** for LeftSemi/LeftAnti/LeftMark, whose probe streams on the device | stands, already pinned; the spec's "join at several probe batches" test is possible for the semi family only |
| `:489` `unload` | `a root-adjacent limit gives the sink a row range per handle, and no shape here plans one` | gap `:460` → **harness gap**: a root-adjacent `LIMIT`; `export` hard-codes `0..u64::MAX` | stands |
| `:495` `unload` | `a sink's recipe is one call per handle` | gap `:466` → **invariant**: `attach.rs::unload` emits one call | never a gap |
| `:61` `open` | `begin_plan failed: …` | spec §"A refusal treated as a crash" → **abort on a device refusal** | stands (task 2) |
| `:98` `scan` | `execute_scan_rowgroups(#seq, …) failed: …` | same → **abort** | stands (task 2) |
| `:127` `execute` | `execute_node(#seq, … handles) failed: …` | the spec's `Session::execute:154`, now inside the walk file → **abort**; #45, #55, #189, #203 are unobservable past it | stands (task 2) |
| `driven()` HashJoin `_` | `a join type no shape here plans` | limit row → **harness gap** for Right, RightSemi, RightAnti, filtered LeftAnti/LeftMark (device green at one probe batch: `a_right_join_over_one_probe_batch_agrees`, `a_right_semi_join_over_one_probe_batch_agrees`, `a_right_anti_join_under_null_equals_null_agrees`) and, with `:225`, unfiltered LeftAnti/LeftMark; **engine limit #152** for Left and Full, which the message under-describes | mixed; Left/Full already pinned |
| `driven()` `ProbeKeys \| NullPad \| Narrow` | `the finish pass accumulates probe keys across batches (#136)` | limit row → `ProbeKeys`, `Narrow`: **harness gap** (the semi family's finish path); `NullPad`: **engine limit #152** — only Left/Full pad, and they refuse before the pad is reached. #175 answered by task 12; #212 is the driver's `without_build` route, not a walk shape | `NullPad` stands as #152; the other two are gaps |
| `driven()` `CrossJoin \| NestedLoopJoin` | `both copy their build side, and the ABI has no copy (#152)` | limit row #152 → **harness gap** at one probe batch (`a_cross_join_is_the_product_on_both`, `an_inner_nested_loop_join_agrees`, `a_left_nested_loop_join_pads_the_unmatched`; `category_of` already routes both through `join()`); #152 bites only at a second batch, which `:469` guards | never a limit at the walk's batch layout |

Counts: of the spec's eight harness gaps, **3 stand** (`:225` for `AccumulatedKeys`, `:296`,
`:489`), **4 were never gaps** (`:234`, `:335`, `:379`, `:495`), **1 is dead** (`:408`). Of the two
engine limits, **0 stand as classified**: both sites still refuse, but `:451` is a harness gap
except for Left/Full under #152, and `:469` is #152 for six kinds and a gap for three; every
#152 refusal either would pin is already a `bug_` row in `build-test.md`'s Known-wrong table. The
`driven()` row: Left/Full and `NullPad` stand as #152 (pinned); cross, nested-loop, `ProbeKeys`
and `Narrow` are gaps; #175 closed, #212 not a walk refusal. Three `rc == 0` aborts stand and are
task 2. Four `driven()`-only gaps the spec's table never had a row for: Right, RightSemi,
RightAnti, and cross/nested-loop, each one query away from the `join()` arm that exists.

### 2026-09-12 — blocked(building): the human decides on the paragraph above

The spec makes this the human's decision and forbids beginning plan task 1 on the run's own
judgement, so the block is by design and the analyst's reading is the paragraph itself. What it
recommends: shrink to plan task 2 alone — a device refusal recorded rather than fatal at the three
`rc == 0` sites in `wire/gpu_tests/walk.rs` (`:61`, `:98`, `:127`), with `driven()`'s three
messages corrected to what the operator harness proved — and drop tasks 1, 3, 4 and 5. Two things
the spec predates are recorded above: the `rc == 0` site is now in the walk file, and
`declared-schemas-derived.md`'s "undrivable" note is stale after task 10. Nothing else in the
chain can progress: tasks 1–12 are done and await the human's merges.
