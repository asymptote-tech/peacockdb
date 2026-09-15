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

### 2026-09-15 — building again: §2 dispatched

The human took the gate (spec, "Decision, 2026-09-15"): only §2 is built. Sites at HEAD
`591b45d8`: `walk.rs:61` (`begin_plan`), `:98` (`execute_scan_rowgroups`), `:127`
(`execute_node`) assert `rc == 0`; `read_export` at `:172` already returns `Err`. `driven()`'s
three `Refused` messages are `mod.rs:267`, `:272`, `:275`. The proving refusal for the new test:
#203 is the cheapest — `declared.rs` query 11 already plans it and is `#[ignore]` only because
the walk aborts. The developer picks among #45, #55, #189 (device half) and #203. Device cycle on
shad-gpu through `build-test-shadgpu.sh`; nothing here compiles under `rust-only`, so the CPU
side is a `cargo check` of the gpu-feature lib at most.

### 2026-09-15 — §2 built: the refusal is a value, the walk stops at it

Shape: `walk.rs` gains `Refusal { rc, call: Option<(Seq, FbKind)>, message }` — `call` is
`None` only for `begin_plan`, the session's own call that no recipe names and no `AbiSymbol`
spells; its `Display` is `#1 CudfProject answered rc 1: <last_error>`. `Session::open`, `scan`
and `execute` return `Result<_, Refusal>` through one `answered(rc, call)` that reads
`last_error` at the site. Every walk arm returns `Result<Lanes, Refusal>` and `?` stops at the
first refusal — no partial result is kept, because `execute_node` resets the session on any
exception and nothing held afterwards is a handle. `try_walk` is the fallible entry;
`walk` is `try_walk` panicking with `{sql}: {refusal}`, so every caller in `mod.rs` and
`declared.rs` is unchanged and still fails loudly, now naming rc and the call.

Subtleties for the next reader:

- **The ABI's code is always 1.** Every `catch` in `gpu_executor.cpp` returns 1 with the cause
  in `last_error`, so "the code" discriminates nothing on its own; the new test asserts `rc == 1`
  and the call, and pins #203 by its message (`cast to STRING`), since any refusal at the
  project would otherwise pass. Post-order for `SELECT CAST(n_nationkey AS VARCHAR) FROM
  nation` at `ONE_LANE`: scan `#0`, project `#1`, the sink has no seq.
- **Left and Full never reach the device in the walk.** Their recipe has `AtDone` calls, so
  `join()`'s all-`PerProbeBatch` assertion (`walk.rs`, the `#136` message) fires before any
  call; on the raw ABI they would fail at the join with `unknown input handle`, since
  `execute_node` consumes its inputs and `resolve` hands `BatchCopy` the same handle. The
  `driven()` message says "refused at the first probe batch … (#152)", which is what the
  operator harness proved of the executor (`copy_of` in `gpu_backend/join.rs`). That `#136`
  assertion message in `join()` still reads as if #136 refused something; the analyst's table
  says it does not, and this task left the string alone (§2 scope).
- **The submodule is empty in a fresh worktree.** `third_party/cudf` had no checkout here, and
  `peacockdb-ffi`'s cmake then fails with `include could not find requested file: rapids-cmake`
  (its `include(rapids_config.cmake)` error scrolls off the top). Populated with
  `git submodule update --init --reference /media/data/peacockdb/.git/modules/third_party/cudf
  third_party/cudf`; superproject status unchanged.
- The `--run-status` poll must not match `running`: the gate log's `running N tests` line makes
  "still running" true forever. Read `FINISHED, exit code` instead.

Proof: `cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run` red with `cannot
find function try_walk` (the only error), then green, 0 warnings; `build-test-shadgpu.sh
--build` 0 warnings; run `20260915T151528-111106`, `PCK_RUN_CPP=0`, `peacockdb_core_gpu_lib`
`gpu_tests::` `running 306 tests` → `test result: ok. 305 passed; 0 failed; 1 ignored`
(query 11 stays ignored), `test_gpu_corpus` 8 passed, `--run-status` rc 0, no `[rmm] pool`
line. `build-test.md`'s "Recipe walk driver" row is now 4 cases and the `gpu_tests::` line 306
— the coordinator's edit.

### 2026-09-15 — completing: round 1 clean

Round 1 on `0681fa84` (PR #154 against `ENS-empty-build`): 0 blocking, 0 important, 5 nits.
Two fixed here, both markdown: `build-test.md:7` said 79 `bug_` tests against the table's 82
(pre-existing on the parent), and the run record's capitals. Three recorded, no code change:
`Refusal::call` and `Firing::target` are both `Option<(Seq, FbKind)>` with `None` meaning
`begin_plan` in one and a bare call in the other — an enum the day `slice_handle` reaches
`answered`; the `begin_plan` arm of `Display` and the scan refusal path have no test, and the
first plan the verifier rejects (#169) is the case for the `None` arm; the `ProbeKeys | Narrow`
message names the semi family only, where `ProbeKeys` is also Left's and Full's first call
(#152). The reviewer also noted the gpu block's two tables do not close — 227 lib coverage
rows + 82 `bug_` = 309 against the 306 measured — pre-existing and not this task's. The
in-body comment says why `rc == 1` is a pin and not a guard: every `catch` in
`gpu_executor.cpp` returns 1. Completeness pass dispatched.

### 2026-09-15 — completeness pass, both readings closed

Reviewer (what is wrong), on `f1bc1983`: 0 blocking, 0 important. Confirmed against
`gpu_executor.cpp` that every failure returns 1 and `execute_node`/`execute_scan_rowgroups`
reset the session, so `Drop for Session` after a refusal is sound; no `should_panic` depended on
the old assertion text. Analyst (what is missing): 0 blocking, 1 important —
`declared-schemas-derived.md`'s blocker note said the walk task "now sits between this and task
1", which resolved to nothing once task 13 taught no shape; one sentence added there naming the
shrink and pointing at the 2026-09-12 per-row standing. `architecture.md`: no sentence falsified
(the three candidate sections read and held). Seen by both and not raised: `join()`'s #136
message; `refcounted-tables.md`'s expectation of a panic from the walk, stale since tasks 4 and
10 and the human's when that chain starts. The signoff is on the spec. Awaiting CI on run
`35032672205` (the code push `74672b43`; the later doc-only pushes are skipped by the changes
gate and prove nothing).

### 2026-09-15 — done

CI run `35032672205` on `74672b43`: every job green — both dataset-matrix legs, the 25.02 GPU
build, the remote GPU tests, cost-report, s3 — Pages skipped as on every PR. The later
documentation-only pushes (`f1bc1983`, `f7ba99a0`, this one) are skipped by the changes gate,
so `74672b43` is the run that proves the PR. Task 13 is the chain's last task; nothing else
on the board can progress until the human merges.
