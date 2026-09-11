# #173 — review of `173-proposal.md`

Read at master `188c23ce`, the commit the proposal names. Every file:line the proposal cites was
opened. The golden claims were re-measured with awk over the six tp4 `*-mini.cpu.txt` files and the
four tp1 ones, independently of the proposal's scripts. Nothing built or run. `175-proposal.md`,
`175-review.md`, `tasks/empty-build.md` and the archived `empty-answers.md` entry were read for the
contradiction the job names.

## 1. Verdict

**Needs changes.** The diagnosis is better than the ticket — the three accumulator arms emit
nothing rather than refuse, the one refusing site is the finish, no registry cell carries #173, and
the code does support the proposal's side of the contradiction with the sibling (both engines emit
exactly one batch per probe call over an empty build, so the sibling's "would have to agree"
objection is void and B3 does bound the cost). But Fix A's Left/Full arms hand a device the pad
project, which has never run on a device and whose numeric NULLs are typed zeros (#198) and whose
Decimal128 NULLs are FLOAT64 — the listed device test cannot pass as written; the shape-(i) minimum
query never reaches `finish_without_keys` on a device (#152 refuses first); Fix B is outside the
frozen `empty-build.md` Restriction and needs the human's amendment, not a developer's; a fourth
site the §1 table misses answers *wrong* on both engines rather than refusing; and "#173 closes"
is false because a planned tree still reaches the LeftMark finish through a merge.

On the job's two questions: **A and C survive 188c23ce** — neither builds a table from nothing,
neither touches the wire, and both use a handle the lane already holds, which is exactly the reason
`empty-answers.md` was rejected. **B survives the rejection but not the frozen spec**: `empty-build.md`
says the probe side keeps dropping and code is limited to the conditional drop; B does the opposite,
buys a shape the corpus does not contain, and costs q77's lane 2 about six device calls that the
sibling's `OwedProbe` makes for free. That trade is the human's to make.

## 2. Findings

**F1 — important — Fix A's Left/Full arms give a wrong device answer, and the listed test would
show it.** The pad project (`wire/join.rs:261-286`) writes one bare typed-NULL literal per padded
probe column (`node_writer.rs:299-306`). In `project.cpp:45-49` a bare *numeric* literal is
`is_ast_able` (`expr.cpp:409-414`: only string literals are refused) and goes through
`build_expr`'s literal arm, which builds every scalar with `valid = true` (`expr.cpp:157-255`) — so
an Int64 pad is `0`, a Date32 pad is epoch, and a Decimal128 pad is a **FLOAT64** column
(`:212-225`, "Promote to float64"). That is #198, and the ticket's and `typed-nulls.md`'s sentence
"a bare literal short-circuits to `build_scalar`" is true only of `build_column:830`, which a
project reaches only for string literals. The pad project has never run on a device (Left/Full
refuse at the first probe batch, `gpu_backend/join.rs:293-299`), so Fix A's Left/Full arms would be
its first run. The proposed device test "Left pads every build row" on the `k:Utf8, v:Int64`
fixture (`test_gpu_executors.rs:76-80`, `join.rs:16-23`) would see `v2 = 0`, not NULL; a Decimal
probe column would change type across lanes and trip the merge or the sink's type check. Correction:
land Fix A's LeftAnti-narrow and LeftSemi arms now; land Left/Full after `typed-nulls.md`, or as
`bug_` tests naming #198 with the Decimal case called out separately, since typed-nulls keeps the
double promotion by design (`typed-nulls.md` §1 "One arm differs on purpose"). Say in §7 that the
Left/Full arms are unreachable on a device from a planned tree until #152 in any case: a lane with a
probe batch refuses on `copy_of` first.

**F2 — important — the shape-(i) minimum query does not reach the failing code on a device.** §5's
`orders LEFT JOIN (… CASE … END AS k …)` puts every probe row in lane `pmod(seed,4)`. That lane's
first probe call runs `Input::BatchCopy` → `copy_of` → refused (#152), and the driver runs the probe
lane as soon as its batch lands (push model), long before any lane's probe side is done — so
`finish_without_keys` never runs; the device fails with #152, not #173. The proposal's "on a device
refused by name … (#173)" is wrong. Correction: use a type whose probe call is the key project alone.
`SELECT count(*) FROM orders WHERE o_orderkey IN (SELECT CASE WHEN l_quantity > 100 THEN l_orderkey
END FROM lineitem)` plans a LeftSemi (orders is the smaller side, so no swap to RightSemi), three
probe lanes get no batch, and the device refuses at `finish_without_keys` with the "no rows" text
today; expected answer 0. The LeftMark query in §5 is fine as it stands. The `LEFT JOIN` form is
still worth keeping as the query the Left arm answers *after* #152.

**F3 — important — Fix B is outside `empty-build.md`, and the proposal does not say what that
requires.** The frozen spec's Restriction: "a scatter feeding a probe side still drops its empties —
the condition that stops this task causing #152", "Code changes are limited to the conditional
drop, whichever of the two shapes §2 settles on, the two doc comments, and the tests", and its §6
demands a test that the probe side drops. B1 keeps probe-side empties, B2 adds driver state, B3 adds
a lane rule in `single_partition.rs`. The proposal says B "extends the sibling's Change 2" — but the
sibling's own Change 1 is already outside the spec (175-review F5), and B extends further. Under the
spec's own rule this is "a finding to report rather than a scope increase": the human amends Scope,
Restriction and §6, or B gets its own spec. State that, and give the amendment. On the contradiction
itself, the code supports the proposal's feasibility claims and not the sibling's objection: an Inner
join over a zero-row build emits one zero-row batch per probe call on both engines — DataFusion's
`process_probe_batch` returns `Ready(Some(result))` whatever the row count
(`datafusion-physical-plan-45.0.0/src/joins/hash_join.rs:1567`) and `execute_single_node` filters
nothing (`cpu_backend/single_node.rs:52-56`); the C++ map arm returns one handle per call — so
"CPU/device would then have to agree" is already true, and B3 bounds it to one call. What the code
does *not* support is that B is worth its cost: on q77 lane 2 it adds `SetBuild(0)`, one
`inner_join` over a zero-row build, a groupby, a concat, a merge and a finalize project where the
sibling's `OwedProbe` makes no call, and every enabled golden has zero guarded lanes under either
climb (re-measured, §3), so B changes nothing the corpus runs. Its whole yield is shape (ii), the
LeftMark finish, and F4's silent wrong answers.

**F4 — important — a fourth site answers wrong on both engines, and the §1 table does not have
it.** The one-call finishing types — LeftSemi/LeftAnti/LeftMark **with a residual filter**
(`plan/join.rs:427-435`: `probe_streams: false, needs_finish: true`, so
`answers_in_one_call`, `plan/mod.rs:772-774`) and `GpuNestedLoopJoin{Left}` — publish no at-done
call. A lane with build rows and no probe batch takes `SetBuild` then `Finish`, and
`finish_and_fetch` returns `Ok(Vec::new())` at `gpu_backend/join.rs:186-188` and
`cpu_backend/join.rs:255-257`. For a filtered LeftAnti (tpch q21's shape), a filtered LeftMark and a
nested-loop Left the correct answer is every build row (padded), so this is a **silent wrong
answer, not a refusal, and not a divergence** — worse than anything the ticket names. Not reached in
the enabled corpus (no finishing join has an empty probe lane, §3). B1's probe-side rule covers it
by accident (`needs_finish` is true for the filtered forms, and B1 names nested-loop Left) and C
covers the limit route, but nothing in "Pinning tests" tests it and the ticket disposition does not
name it. Correction: add it to §1 and §2; add the CPU and device unit cases (filtered LeftAnti,
`set_build` then `finish_and_fetch` with no probe → every build row); and if B is not taken, this
stays open under #173 (narrowed) or a new ticket.

**F5 — important — "#173 closes" and "a planned tree does not reach this" are false.** B1 stops
at forwarders ("the other lanes carry the answer"). They do not when every lane is empty: a filter
that keeps nothing emits zero-row batches (`an_operator_emitting_empty_batches_is_carried_through`),
the scatter below a non-owing join then produces N zero-row outputs, all N lanes drain, and a
`GpuMergePartitions` above forwards nothing — so a one-lane finishing join over that merge (the
broadcast shape, `architecture.md` "Equal lane counts are not co-partitioning") reaches
`finish_without_keys` with no batch, and for LeftMark it is refused by name. A refusal is production
behaviour and keeps a ticket (prompts.md). Correction: either narrow #173 to "a LeftMark finish over
a probe that produced no batch, reached through a merge whose every lane drained" rather than close
it, and drop the "so a planned tree does not reach this" clause from the error text; or extend B1 to
continue through `GpuMergePartitions`/`GpuUnion`/`GpuInterleave` and stop at `GpuEmitPartitions`
(a scatter above redistributes, so emptiness below it is irrelevant). Re-measured: continuing the
climb through merges guards **zero** additional lanes in any enabled golden — every one of the 189
merge-stopped climbs ends at a scatter above the merge — so the extension is free.

**F6 — minor — B3 as written does not compile.** `select` is `fn select(&self, category, avail)`
(`single_partition.rs:164-168`); it has no `site`, and `site.owes_probe_when_build_empty` is not
in scope. Correction: record the verdict on the lane in `run`'s `SetBuild` arm (`:301-317`, where
`site` and `n_rows` are both in hand) as one flag `probe_once = n_rows == 0 && !owes`, and have
`select` read the lane's own state; sweep the new flag(s) in the cross-product test
(`single_partition/tests.rs:328-380`). Also: the B1 pseudo-rule "cross / nested-loop:
false-and-continue" is self-contradictory as text — say "not owing; continue".

**F7 — minor — the ≥25.10 `filtered_join` hazard applies to B's probe-side spare, and §7 names the
wrong table.** For LeftSemi/LeftAnti/LeftMark the C++ builds `filtered_join` over `right_keys`
(`join.cpp:122-124, :143-145, :238-240`) — the **keys**, not the build. A single zero-row spare
makes the concatenated keys zero-row, which is the zero-block launch 175-review F1 describes. On
shad-gpu (25.02) the free functions guard it; on the 25.10 CI leg it only compiles. Pre-existing for
any probe that emits only zero-row batches, so not new, but B makes it the ordinary path for a
guarded lane. Correction: take 175-review F1's four-arm guard, and rewrite §7's bullet to say
"keys of zero rows".

**F8 — minor — the driver test for B inherits 175-review F4.** `MockAcc` under `CoalesceAll`
emits a batch at done whether or not anything arrived (`driver/mock.rs:421-430`), so "the Inner
takes `SetBuild` with zero rows" is true before any change. Fix the mock first, as that review says,
or assert the emitter's `Emit` produced count and the coalesce `in` lists.

**F9 — minor — three numbers.** (a) Finishing joins at tp4-single: 30 (24 tpcds + 6 tpch), not
38; the conclusion (none with an empty probe lane) holds. (b) "At most 244 spares corpus-wide" under
the unconditional alternative: 244 is the number of emitter *sites* with an empty lane across the
six tp4 files; the permanently-empty *lanes* — one spare each — number 451. (c) "Fix A alone …
touches one file": it needs `build: SchemaRef` threaded from `executors_for`
(`gpu_backend/backend.rs:128-136`), so two.

**F10 — minor — "widens the same cover" is overstated.** The injection hash dimension runs on
`test_cpu_end_to_end` only, and the CPU finish never starved (`cpu_backend/join.rs:258-260`,
`concat_batches` over no batches is an empty batch, and DataFusion answers). B changes nothing in
that cover; only the sibling's owing-join gate does.

**F11 — minor — Fix C costs one `slice_handle` per mid-plan limit with an offset, on every device
run.** The spare is taken from the *first* out-of-interval batch (there is no batch at done), so
`tpch/nested-limits` and every `OFFSET` query gains one FFI call and one short-lived handle before
the straddling batch releases it. Not visible in any golden. Say so at the site. Also worth one
line in §6: C incidentally hands the device's global aggregate a zero-row batch on the limit
route, which is the route `cpu_backend/accumulate.rs:366-370` names for #199.

**F12 — minor — §7's LeftSemi unknown is settled by reading.** DataFusion's
`process_unmatched_build_batch` returns `Ready(Some(result))` for every `need_produce_result_in_final`
type whatever the row count (`hash_join.rs:1573-1618`), so the CPU's LeftSemi finish over no keys
is `[0]`, matching the device arm. Drop the "if the CPU yields nothing" fallback.

## 3. Claims verified

- The four sites and what each does: `gpu_backend/accumulate.rs:209-212`, `:247-250`, `:387-389`
  return `Ok(Vec::new())`; `cpu_backend/accumulate.rs:138-141`, `:196-199`, `:270-272` the same;
  `gpu_backend/join.rs:209-238` is the one refusal (LeftAnti hands the build up, four types refuse);
  `cpu_backend/join.rs:253-266` answers all five through DataFusion; the C++ throw at
  `node_session.cpp:277-282` is unreachable from Rust.
- `GpuProbingJoin::build` (`:136`) is `Some` whenever `finish_without_keys` runs: every probe call
  pushes to `accumulated` (`:174`), so `accumulated.is_empty()` means no probe call ran.
- The at-done recipe: concat over `AccumulatedKeys` (`wire/join.rs:94-100`), finish join with no
  projection (`:194-237`), pad project reading ordinals `< build_width` from the build and typed
  NULLs above it (`:261-286`), narrow project over `finish_output` = build schema for non-mark
  (`:288-322`). Feeding the build handle to the pad/narrow project is type-correct.
- Today's LeftAnti arm ignores `projection` (returns the raw build handle) — hacks-audit
  "propping up" 11 is real and Fix A's narrow fixes it.
- `slice_handle(h, 0, 0)`: `clamp_row_range` gives `{0,0}` and `cudf::slice` then `cudf::table`
  makes a zero-row owning table (`node_session.cpp:513-541`) — the same construction the scatter
  uses for an empty lane (`:405-421`). `slice_handle` consumes its input.
- `LimitStream` drops an out-of-interval batch at `gpu_backend/accumulate.rs:420-424` and
  `cpu_backend/accumulate.rs:409-411`; `range_of` (`plan/interval.rs:6-18`) also returns `None` for
  a zero-row batch, so a limit drops those too; `HeldBytes` reports `Limit => 0` on both backends
  (`gpu_backend/backend.rs:334`, `cpu_backend/backend.rs:306`).
- The drop at `partitioned.rs:379-386`; the emitter's done arm at `:348-353`; `release_in_flight`
  at `:640-649` walks `out_queues` only; the Python twin at `exec_model/partitioned_driver.py:296-301`
  and CI runs it (`pipeline.yml:766`).
- `Draining` at `single_partition.rs:187-192`; `NoBuild` at `:183-185`; `DropProbe` exists
  (`:53`, `:331-339`) and records `ReleaseUnwanted`; `SetBuild` has `n_rows` and `site` in hand
  (`:301-317`).
- `empty_build_answers_nothing` (`plan/join.rs:452-461`), `per_call_join_type`,
  `finish_join_type` (`:470-491`), `capability` (`:404-449`), `hashed_on` for `n > 1`
  (`:523-536`); coalesce insertion at `translator/nodes.rs:207`, `:275`, `:278`, `:460`, `:499`.
- Zero survivors is a plan-time error (`scan_mapping/partition.rs:25-28`), so "every source lane
  empty" is not a run-time shape and the claim that an empty *source* lane never meets a join holds.
- A zero-row batch passes through Exec and BatchAccumulator nodes as a zero-row batch on both
  engines: `CpuExec::exec` concatenates its (possibly empty) output (`cpu_backend/mod.rs:193`);
  the CPU grouped `AggregateBatches::compact` wraps DataFusion's no-group `None`
  (`row_hash.rs:947-949`) in `concat_batches` (`cpu_backend/accumulate.rs:356-359`); the device
  compacts and finalizes one handle (`gpu_backend/accumulate.rs:292-322`).
- Goldens, re-measured over all six tp4 files: 244 emitter sites with an empty lane (451 lanes),
  and **none** is guarded under the sibling's climb, under B1's climb, or under B1 extended through
  merges (every merge-stopped climb ends at a scatter above); no join has a `[0]` build batch; no
  finishing join has an empty probe lane; joins with an empty build lane are all Inner. No emitter in
  any tp1 file. So no existing `.cpu.txt`/`.cost.txt` section moves under A, B or C.
- `testdata/cost-registry.csv` carries no `173`; `#173` appears in six code files, all comments and
  messages; no `corpus_cases.inc` comment names it.
- `architecture.md:476-479`, `:606-607`, `:853-858`, `:194`, `:393` are the sentences the
  proposal says move.
- Tests cited exist at the lines given: `test_gpu_executors/join.rs:337-378`,
  `test_gpu_executors/accumulate.rs:296-329`, `cpu_backend/tests/join.rs:600-618`,
  `cpu_backend/tests/accumulate.rs:66-72`, `driver/tests/flow.rs:262-277`,
  `single_partition/tests.rs:328-380`, `driver/plans.rs:101-112`.
- `operator-cases.md:59` carries the "only zero-row probe batches then the finish" row.

## 4. Corrected proposal

Only the sections that change.

### 1. Issue — amendments

Add a fourth row to the site table: the one-call finishing types — filtered LeftSemi/LeftAnti/
LeftMark and nested-loop Left — whose `finish_and_fetch` returns nothing at
`gpu_backend/join.rs:186-188` / `cpu_backend/join.rs:255-257`. For a filtered LeftAnti, a filtered
LeftMark and a nested-loop Left with build rows and no probe batch this is a silent wrong answer on
both engines (every build row owed, none emitted). Not in the enabled corpus; ordinary SQL (q21's
`NOT EXISTS` with a residual over a small build).

### 3. Localized fix — amendments

- **Fix A, split.** Land LeftAnti-with-projection and LeftSemi now. The Left/Full arms are
  correct only after `typed-nulls.md`: until then the pad's numeric NULLs are zeros and its
  Decimal128 NULLs are FLOAT64 on a device. Either sequence those two arms after typed-nulls, or
  land them with `bug_` tests naming #198 (and a separate `bug_` for the Decimal type change, which
  typed-nulls keeps). Thread `build: SchemaRef` from `executors_for` (`gpu_backend/backend.rs:128-136`).
- **Fix B needs a spec decision first.** Before dispatch the human amends `empty-build.md` — Scope
  gains `driver/single_partition.rs`, `driver/partitioned.rs` beyond the one line, `driver/index.rs`
  (the climb through joins), `exec_model/partitioned_driver.py`; the Restriction drops "a scatter
  feeding a probe side still drops its empties" and "code changes are limited to the conditional
  drop"; §6's probe-side-drops test becomes "one spare, delivered at done, on a guarded probe lane"
  — or B becomes its own spec. If the human declines B, A + C still close the device divergence at
  the finish for the types A covers, and #173 narrows (below).
- **B1, if taken:** continue through `GpuMergePartitions`, `GpuUnion` and `GpuInterleave`; stop
  at `GpuEmitPartitions`, `GpuMergeSortedPartitions`, `GpuUnload`. This closes F5's merge route at
  no golden cost.
- **B3:** the verdict is recorded on the lane at `SetBuild` from `site` and `n_rows`; `select`
  reads lane state only; the cross-product test sweeps the new flag.
- **C++ guard:** 175-review F1's four-arm `filtered_join` zero-row guard, because a guarded probe
  lane makes the finish's `right_keys` zero-row on the ≥25.10 leg.
- **Pinning tests, additions:** filtered LeftAnti with build rows and no probe batch → every build
  row, on both backends (goes red today on both: that is the F4 case); the Left/Full device cases
  written as above; the B driver test after the mock's `CoalesceAll` emits nothing for nothing
  (175-review F4); the LeftMark-through-a-merge case if B1 climbs through merges, else a `bug_`
  test for it.

### 5. Minimum corpus query — amendments

Shape (i) on a device: `SELECT count(*) FROM orders WHERE o_orderkey IN (SELECT CASE WHEN
l_quantity > 100 THEN l_orderkey END FROM lineitem)` — LeftSemi, key project only per probe, three
empty probe lanes, refused today at `finish_without_keys` ("no rows, which is a table of no rows …
(#173)"), expected 0. The `LEFT JOIN` form is the query the Left arm answers after #152 and after
typed-nulls, and fails today on #152 before any finish runs. Shape (iii) as proposed, with the
swap risk noted; if DataFusion swaps it to Right, it is #175's residual instead.

### 6. Cells re-enabled — amendments

None on #173's account, as the proposal says. Add: Fix C reaches #199's limit route on the device
(the global aggregate gets a zero-row batch rather than nothing). Drop the "widens the same cover"
sentence.

### 7. Risks and unknowns — amendments

- #198 makes the pad project wrong on a device today; Fix A's Left/Full arms are the first thing
  that would run it.
- The `filtered_join` zero-row launch on ≥25.10 is over the **keys** for the build-side semi
  family; a guarded probe lane reaches it as a matter of course.
- **#173 does not close** unless B1 climbs through merges: a finishing join over a merge whose every
  lane drained still reaches `finish_without_keys`, and LeftMark refuses there. Narrow the ticket to
  that shape, or close it only with the merge climb landed.
- Fix C adds one `slice_handle` per device run of any mid-plan limit with an offset.

### 8. Complexity

M for A + B + C stands for the code. Add what the estimate leaves out: a spec-amendment round with
the human before B can be dispatched; a sequencing dependency on `typed-nulls.md` for A's Left/Full
device proof; the four-arm C++ guard; and the mock fix the B driver test needs. A + C alone is S
(three files, no spec conflict, no dependency except that the Left/Full device test is a `bug_`
test until typed-nulls).

## 5. Complexity

**M**, agreeing with the proposal for A + B + C, and **S** for A + C — with the caveat that the
part of B the corpus can see is zero and the part it cannot see is what the human has to decide to
pay for. My recommendation, since the code supports the feasibility of both sides: land A (split as
above) and C on their own; put B to the human as an amendment to `empty-build.md` with the q77
call-count trade stated plainly, and if taken, with the merge climb so #173 can actually close.
