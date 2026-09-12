# empty-build — run record

Spec: [`empty-build.md`](empty-build.md). Plan: [`empty-build-impl.md`](empty-build-impl.md).
Branch `ENS-empty-build` off `ENS-typed-nulls`; PR against it when reviewing.

### 2026-09-12 — building: plan tasks 1 and 2 dispatched

The index's derived field and the conditional drop first, with their tests at the rust rung.
#175's handoff from task 9 stands over the spec: the three `without_build` pins in
`gpu_tests/join_cases.rs` (`bug_right_with_no_build_batch_is_refused_on_both` and its Full and
RightAnti siblings) are reached by no driver, so whichever of §2's two shapes wins, they are
retargeted or deleted by hand in the change that fixes the cause, and the known-wrong table moves
with them.

### 2026-09-12 — plan tasks 1–3 done: the index field, the conditional drop, the end-to-end pad

**Task 1 step 1, the three facts, all in place at HEAD `2c5b3a47`:** `IndexedNode` carries
`parent: Option<usize>` and `children: Vec<usize>` (`executor/mod.rs`); `PROBE_CHILD` is 1
(`driver/index.rs`); `index/tests.rs:78` records "the join's build side is its first child,
which is what makes `BUILD_SLOT` zero". None had moved.

**What landed.** `IndexedNode::feeds_owing_build`, computed in `driver/index.rs::build` after
the walk by a parent climb to the first `Join`; `PlanIndex::feeds_owing_build(node)` reads it.
The climb reads the join's type through `as_node_ref` — `NodeRef::Join` carries `join_type`;
cross and nested-loop joins fall to `false`. `driver/partitioned.rs` drops an empty scatter
output only where that field is false. No plan field, no recipe field, nothing on the wire.

**The design choice from §2: the lane is routed to `SetBuild` first; `without_build`'s `false`
branch stays as it is.** Reasoning: the branch cannot "stop erroring" without a signature
change, because `without_build` returns `Result<(), _>` and has nothing to emit the padded
probe rows into — and the spec forbids the signature change. Routing earlier needs the driver
to know the consumer at the scatter, which the index field gives it. So the branch is not
touched, and its doc comment on both backends now says what still reaches it.

**The branch is reachable, not unreachable — by a shape this task did not anticipate.** An
upstream that emits no batch at all, as opposed to a zero-row batch. Confirmed by a scratch
end-to-end query (not kept): `(SELECT ... FROM nation OFFSET 100) n RIGHT JOIN region r`
plans `GpuCoalesceAllBatches <- GpuLimit skip=100` under the build side, `LimitStream` emits
nothing for every batch, the coalesce holds nothing and emits nothing, and the lane takes
`NoBuild` and refuses — at tp1-single, with no scatter anywhere. Filed as #212. The existing
driver test `a_join_that_owes_its_probe_side_without_a_build_side_is_refused` pins that route
through `AccRule::EmitAtDone(0)` and now cites #212.

**The three `bug_` pins stay as they are.** `bug_right_with_no_build_batch_is_refused_on_both`
and its Full and RightAnti siblings reach `without_build` with `build: None`, which is the
#212 route and still what a driver does for a limit that skips everything. They are retargeted
by ticket only — their header comment and their known-wrong rows in `build-test.md` now name
#212 — and stay green refusals. Their device run is plan task 6's.

**A finding the coordinator needs for task 4: only `q16` goes green; `q77` does not.** A scratch
end-to-end run (not kept) of both at all five modes: tpch `q16` answers like the oracle at all
five; tpcds `q77` still refuses at tp4-single, "GpuHashJoin lane 2". Its Right outer's build
child is `GpuProject <- GpuAggregateBatches <- GpuAggregate <- GpuProject <- GpuHashJoin(Inner)`;
the only scatter under it climbs to the Inner join first, which owes nothing, so its empty lane
drops (the spec's own rule, and the guard test), the Inner lane emits nothing, and the grouped
merge emits nothing where nothing arrived. The Right join gets no batch at all — the #212
route. Fixing it needs a lane that owes nothing to hand up a zero-row table where a join above
owes rows, which on the device is a table out of nothing (#173). Out of this task's scope by
the spec's restriction; #212 names q77. Task 4's registry move is therefore `q16`'s three tp4
cells; `q77`'s cells move from #175 to #212, not to green.

**The goldens did not move at this rung.** `test_cpu_corpus` (448) compares every enabled
cell's `.cpu.txt` section against the fresh render and passed unchanged: no enabled cell has an
owing join over an empty scatter lane, which is what the registry says by carrying #175 on
`q77` and `q16` alone. The delta the spec expects appears when `q16`'s tp4 cells are enabled.
`test_corpus_goldens` 20 and `test_cost_model` 3 also green.

**The mock had to be made honest first.** `MockAcc` under `AccRule::CoalesceAll` emitted one
batch at done whatever arrived, so the driver test for `SetBuild` went green before the fix:
the mock invented the build batch the driver had dropped. It now emits nothing where no batch
arrived, like `cpu_backend/accumulate.rs::one_batch`; `an_empty_build_lane_reaches_set_build…`
was watched red under that mock and green after the drop became conditional.

**Tests added (rust rung, all green).** `driver/index/tests.rs`:
`the_index_marks_only_lanes_feeding_a_build_side_that_owes_rows` (build child of Right → true;
probe child of Right, build child of Inner, no join → false; the join is never the scatter's
parent, so a one-step lookup fails it). `driver/tests/flow.rs`:
`an_empty_build_lane_reaches_set_build_rather_than_no_build` (Right, Full, RightAnti: 4
`SetBuild`, 0 `NoBuild`, mock join refusing `without_build`),
`a_join_type_that_owes_nothing_still_drops_its_empty_lanes` (the six types: every `Emit`
queues one lane, 1 `SetBuild`, 3 `NoBuild`), `a_scatter_feeding_a_probe_side_still_drops_its_empties`
(one `Probe` call, four `SetBuild`), `a_build_batch_does_not_become_a_probe_call` (four probe
calls for four probe batches, no `ReleaseUnwanted`),
`holds_and_releases_balance_over_a_plan_with_empty_lanes` (the emitter's lanes read
`(4,32),(0,8),(0,8),(0,8)` and holds equal releases). `tests/end_to_end/dimensions.rs`:
`a_right_join_with_an_empty_build_pads_every_probe_row` (tpcds q93) and
`a_right_anti_join_with_an_empty_build_returns_every_probe_row` (tpch anti-join), each at
tp4-single with the degenerate hash, the answer against DataFusion on the same SQL, a scatter
feeding an owing build side seen emitting a zero-row lane, and no owing join reaching
`NoBuild`; both watched red with the drop unconditional. They replace
`a_degenerate_hash_under_a_right_outer_is_refused_by_name`, and the `owes_probe_when_empty`
exclusion in `injection.rs::candidates` and `requirements` is lifted, so the injected matrix
now runs the degenerate hash over q93, q97 and anti-join too — all green. `run_and_check`
returns the `RunReport` for those two.

**Wiki kept in step in this change:** `architecture.md`'s hash-skew paragraph and the three-
refusals paragraph; `build-test.md` counts (`--lib` 553, driver 95, end to end 27, internals
44) and the three pins' rows; `tickets.md` #212. Plan task 5's items (`accumulate.rs` comments,
#173's text, #175's corpus reach) are untouched.

**Proof:** `cargo test --features rust-only -p peacockdb-core --lib` → 551 passed, 0 failed,
2 ignored (553); `--test test_module_layout` → 17; `--test test_cpu_corpus` 448,
`--test test_corpus_goldens` 20, `--test test_cost_model` 3; rustfmt `--check` clean on every
touched file with `skip_children=true` (the tree is not rustfmt-clean and is formatted per
touched file); comment caps counted over the diff, longest doc 9 lines, longest body 4.

### 2026-09-12 — plan task 4 done: the goldens, read

**Order.** `tpch q16`'s three `cpu_tp4_*` cells enabled in `testdata/cost-registry.csv` and
its `corpus_query!` line in `tests/common/corpus_cases.inc` widened to all five modes
together — `the_registry_matches_the_cpu_corpus_in_both_directions` holds both ways. `q77`'s
row moves #175 → #212 with the reason (its Right outer's build side is a grouped aggregate
over an Inner join whose empty lane drops as it should), and the two comments in
`corpus_cases.inc` that named #175 for q77 and q80 say so. Then
`UPDATE_CANONICAL=1 cargo test --features rust-only -p peacockdb-core --test test_cpu_corpus
--test test_cost_model` (451 + 3, green), then the diff.

**What moved.** `tpch.sf1/tp4-{single,rowgroup,sized}-mini.cpu.txt` and the three `.cost.txt`
beside them gained a `== q16` section each, and nothing else in any of the six changed — the
diff has no other `==` line. `mini.result.txt` moved one line: q16's over-cap marker now says
`mode=tp4-sized`, because the authoritative mode is the last one declared and that is now
tp4-sized rather than tp1-rowgroup; the rows are unchanged. `.plans.txt`,
`recipe-payloads.txt` and the `--- memory ---` sections did not move.

**The batch-count delta, against the spike's 244: six.** Two kept zero-row batches per tp4
mode, on lanes 1 and 3 of the `supplier` scatter under q16's RightAnti —
`GpuEmitPartitions: hash=[s_suppkey@0] … batch_rows=[[2],[0],[2],[0]]`, the coalesce above it
the same, and the join's `in_rows=[[2,0,2,0],…]`. The spike's 244 counted every permanently
empty lane in the goldens whatever consumed it; the guard keeps only those feeding a build
side that owes rows, and among enabled cells that is q16 alone. Far from 244 on the small
side, which is the guard answering `true` narrowly rather than widely.

**A harness race, seen once.** The whole-corpus regeneration wrote all six cost/cpu sections
but `tp4-single-mini.cpu.txt` came back still reading `skipped: not enabled at this mode`
for q16 while its `.cost.txt` had the section. `corpus_golden::merge_section` locks the inode it
opened and `publish` renames a sibling onto the path, so a case that opened the old inode
before another case's rename locks a file nobody else will lock again, reads stale text, and
publishes without the other's section — a lost update, visible only on a section that is new.
Refilled with `PCK_UPDATE_SECTIONS=1 … -- --exact cpu_tpch_q16_tp4_single`. Test
infrastructure, not engine behaviour, so no ticket; noted here for the next regeneration.

**#175's cells.** No registry cell names #175 now: q16's are enabled, q77's carry #212.

### 2026-09-12 — plan task 5 done: the comments and the two tickets

`cpu_backend/accumulate.rs::one_batch`'s doc no longer rests on "the device's collapse of no
handles is a refusal": both sides answer nothing before any call, and a lane whose one arrival
was a zero-row batch holds a batch and emits it, which is how a kept empty reaches the join.
`gpu_backend/accumulate.rs::SortedRuns` and `gpu_backend/mod.rs::GpuPartitionAccumulator`
said "the device refuses that (#173)" where the code returns `Ok(Vec::new())`; both now say
the answer is given here before any call. The C++ guard in `node_session.cpp` is left as it
is: its comment already says reaching it is a driver bug. #173 is cut to its one refusing
site, `finish_without_keys` on the probe side, blocking no cell; #175 records the fix, its
corpus reach (q16, q77, never q21), q77's handover to #212 and the pins' move. `architecture.md`'s
hash-skew and three-refusals paragraphs were rewritten in tasks 1–3 and read again: right.

### 2026-09-12 — plan task 6 done: the device rollout

`scripts/build-test-shadgpu.sh --build --push-binaries --patch` (exit 0, 0 warnings), then
`--run-detached` with the rust filter empty; run `20260912T140917-414448`, exit 0. Every
binary ran whole: C++ 12 + 6 + 34 + 4 + 4 all passed, each under `[rmm] pool on a discrete
device: … reserved` (no pool failure); `peacockdb_core_gpu_lib -- gpu_tests::` 304 passed,
1 ignored, 0 failed; `test_gpu_corpus` 8 passed.

**The five things, each read off a green case in that run.** (1) `filtered_join` constructed
from a zero-row build: `right_anti_over_a_zero_row_build_keeps_every_probe_row`,
`right_semi_/left_semi_/left_anti_over_a_zero_row_build_answers_nothing`. (2) `left_join`
over a zero-row build: `right_over_a_zero_row_build_pads_every_probe_row`; `left_anti_join`
over one: the RightAnti case. (3) `concatenate`/`merge` with zero-row members:
`one_zero_row_batch_coalesces_to_zero_rows`, `a_zero_row_batch_among_others_adds_nothing`,
`a_zero_row_batch_among_sorted_others_adds_nothing`,
`a_merge_over_one_zero_row_arrival_is_zero_rows_on_both`. (4) `to_arrow_host` on a zero-row
string column: `a_zero_row_batch_types_its_string_column_as_a_populated_one`,
`an_unload_of_a_zero_row_batch_is_zero_rows_under_the_schema_on_both`. The three `bug_` pins
on #212 (`bug_right_/bug_full_/bug_right_anti_with_no_build_batch_is_refused_on_both`) green.

**(5) q16's device cells — a scratch run, reverted.** Its five gpu cells enabled temporarily
in the registry and the declaration, `--build --push-binaries --patch`, then
`PCK_TEST_FILTER=q16 --run-detached` (run `20260912T141215-415154`). All five red, on causes
the row already names: tp1-rowgroup, tp4-single, tp4-rowgroup and tp4-sized on #152 — "this
join's recipe copies its build side per probe batch and the ABI has no copy: probe batch 2
has no build side left, since the call for batch 1 erased it (#152)" — and tp1-single earlier,
at the unload, on #183 — "expected Utf8View but found Utf8 at column index 0". So the cells
stay disabled and no ticket moves; the causes are ordered as the spec expects. The scratch
enable was reverted and the branch's binaries rebuilt and pushed back to the host.

**build-test.md.** `test_cpu_corpus` 448 → 451 (three cells re-added, 447 cells + 3 checks +
the registry test), cpu header 1024 → 1027, Rust 1344 → 1354 and the grand total 1787 → 1797
(the +7 of tasks 1–3 had been written into the rows and the cpu header but not into the
grand total; corrected here). The known-wrong table's three pins are on #212 since tasks 1–3.
