# #175 — review of `175-proposal.md`

Read at master `188c23ce` (code identical to the proposal's `c18e063a`; only `llm-wiki/` moved
between them). Every file:line the proposal cites was opened. Golden claims were re-measured with
awk over the committed `*-mini.cpu.txt` files, not taken from the proposal's scripts. Nothing built
or run.

## 1. Verdict

**Needs changes.** The diagnosis is right on every point where it corrects the frozen spec —
q77 is shape (b) and the spec's Change 2 alone never reaches it, the goldens move by zero rather
than "the low hundreds", and the RightAnti C++ arm does need a guard on the 25.10+ leg — but the
fix as written names a C++ form that does not compile on the local 26.02 toolchain, claims a device
proof the 25.02 host cannot give, misses one golden that goes red (`mini.result.txt`'s q16 mode
line), pins Change 2 with a driver test the mock makes vacuous, and treats a frozen-spec violation
and a still-open refusal as settled.

## 2. Findings

**F1 — important — the "smallest form" of the RightAnti guard does not compile on 26.02.**
Proposal §3 Change 2 offers two forms and names first "take the `#else` branch's free call —
`cudf::left_anti_join(right_keys, left_keys, EQUAL)`". Inside the `#ifdef PEACOCK_HAVE_FILTERED_JOIN`
branch that call is built against a cuDF that has `filtered_join.hpp`, and on those versions the
free semi/anti functions are `[[deprecated]]` (vendored 25.10, `third_party/cudf/cpp/include/cudf/join/join.hpp:216,251`)
or gone: the local `rapids` env is `libcudf-26.02.01` (`scripts/build-test.sh:38`, conda-meta) and
its `include/cudf/join/join.hpp` declares no `left_semi_join`/`left_anti_join` at all. The
developer's first `build-test.sh --build` fails; on the CI 25.10 leg it is a new deprecation
warning, which the definition of done forbids. Correction: use the second form only — when the
table `filtered_join` would be built from has zero rows, fill `single_indices` with
`thrust::sequence` over the probe rows for anti and leave it empty for semi. Guard the row count of
the table handed to `filtered_join`, not `ltv` by name: for `LeftSemi`/`LeftAnti` that table is
`right_keys`, the probe batch (`join.cpp:126-128, :140-142`), which a filter that passed nothing
also makes zero-row — the same latent launch in all four arms, so either guard all four (four
`if`s) or file the other two as a ticket.

**F2 — important — the guard cannot be proven on the device this repo tests on.** §8 says
"Device rollout on shad-gpu proves the RightAnti guard through test_gpu_executors/join.rs". shad-gpu
binaries are built against `rapids-cuda-12.2` (`scripts/lib/shadgpu-env.sh:14`), which is
`libcudf-25.02.02` with only `include/cudf/join.hpp` — no `cudf/join/filtered_join.hpp`, so
`PEACOCK_HAVE_FILTERED_JOIN` is undefined there (`join.cpp:18-20`) and the `#else` free-function
branch runs, which cuDF guards itself (`semi_join.cu:56-64` on 25.10; the same guard on 25.02). The
proposed device test proves the 25.02 path and never enters the guarded branch; the 25.10 CI leg
only compiles (`build-test.md`: "26.02 needs only to compile"). State it: the guard is a by-reading
change, unprovable until a ≥25.10 device run exists. The device test is still worth writing — it
proves `Right` (`hash_join::_is_empty`, `hash_join.cu:516,785-787`) and the 25.02 anti path.

**F3 — important — `.result.txt` does move, and a test goes red until it does.** The proposal's
golden table says `<tier>.result.txt` "no change (live_cpu gpu oracle)". The gpu oracle is
irrelevant: the section exists (`tpch.sf1/mini.result.txt:451-453` — an over-cap marker with
`mode=tp1-rowgroup`), and its `mode=` line is the last cpu mode the registry enables
(`tests/common/corpus.rs:394-405`). Enabling `cpu_tp4_sized` for q16 makes that `tp4-sized`, and
`test_corpus_goldens.rs:574-600` `every_result_section_names_the_mode_that_would_author_it_now`
asserts the two agree. Correction: the regen at tp4-sized rewrites the line; list
`tpch.sf1/mini.result.txt` (one line) among the goldens that move.

**F4 — important — the driver-level test for Change 2 cannot go red.** §3 proposes
"`EmitRule::ToLane(0)` under `join_of(Right, coalesce_all(emit(..,4)), emit(..,4))` gives four
`SetBuild`, and under `Inner` gives one `SetBuild` and three `NoBuild`". The mock coalesce under
`AccRule::CoalesceAll` emits one batch at done whether or not anything arrived
(`driver/mock.rs:421-430`: `vec![MockBatch { rows: self.held_rows, bytes: … }]`), which is why the
existing NoBuild tests force the path with `EmitAtDone(0)` (`flow.rs:210,233`). So today, before
any change, `ToLane(0)` gives four `SetBuild` for `Inner` too — the `Right` assertion passes
before the fix and the `Inner` assertion is false before and after it. `empty-build-impl.md` Task 2
("assert the join is asked to SetBuild rather than NoBuild") carries the same trap. Correction:
make `CoalesceAll` emit nothing when it held nothing, which is what both real backends do
(`cpu_backend/accumulate.rs:138-141`, `gpu_backend/accumulate.rs:209-212`) — then the two
assertions mean what they say — or assert the emitter's own trace instead: `CallKind::Emit`
produced 4 under `Right` and 1 under `Inner`, and the coalesce lanes 1–3 `in` counts.

**F5 — important — Change 1 is outside the frozen spec, and the proposal does not say what that
requires.** `empty-build.md` Restriction: "no `without_build` signature change … Code changes are
limited to the conditional drop, whichever of the two shapes §2 settles on, the two doc comments,
and the tests"; its scope table names neither `driver/single_partition.rs` nor `executor/mod.rs`.
Change 1 deletes a trait method, adds a `LaneState`, a `LaneCall`, a `LaneSite` field and a new
refusal site. The spec's own rule for this case is "a finding to report rather than a scope
increase" — i.e. the human amends the frozen spec (Scope, Restriction, §6 tests, "Sequencing")
before a developer is dispatched. Say so, and give the amendment text. Also say plainly what §6
implies but never states: Change 1 re-enables no cell today — q77 moves from #175 to #189 — its
value is (a) q77's refusal gone, (b) making the injection gate's removal sound for shape-(b) plans
(under a degenerate hash a shape-(b) lane is empty on both sides and Change 2 alone still refuses
it at `NoBuild`), (c) hacks-audit items 4, 5, 8. That is enough to justify it, but the human
decides, and the fallback the proposal names (keep `without_build`, defer its `Err`) is also
outside the Restriction.

**F6 — important — "#175 closes" is premature.** After both changes a `Right`/`Full`/`RightAnti`
lane whose build side received nothing because a join below it drained (`single_partition.rs:187-192`),
or a mid-plan `GpuLimit` dropped every batch (`cpu_backend/accumulate.rs:407-410`), while its probe
side has rows, is still refused by name — Change 1 moves the refusal to the first probe batch. A
refusal is production behaviour and keeps a ticket (prompts.md, "Only production behaviour gets a
ticket"). Correction: narrow #175 to that shape rather than close it, or close it and file the
residual; `architecture.md:855-857` is then narrowed, not dropped. (A filter that matched nothing
is *not* residual: the filter emits a zero-row batch, the merge forwards it, the scatter is called
once and Change 2 keeps its empties — only the scatter drops empties in the driver, hacks-audit
"propping up" 1.)

**F7 — minor — `OwedProbe` refuses a zero-row probe batch that owes nothing.** The proposal's own
root cause is "the lane never finds out whether any probe row exists to be owed"; a probe batch of
zero rows answers that question with no. Release it (`DropProbe`'s path, `CallKind::ReleaseUnwanted`)
and refuse only when `rows > 0`. Not reachable in the enabled corpus (the `Empties` injection is
source-side and no injected plan has shape (b)), but it is the arm's stated principle.

**F8 — minor — the deletion list misses a fixture.** `driver/single_partition/tests.rs:259`
`empty_build_owes_its_probe: false` is a fourth `JoinRule` initializer beside the three named
(`failure.rs:94`, `flow.rs:404`, `memory.rs:164`). Compile error, one minute.

**F9 — minor — §5's `collect_statistics` claim is wrong.** `ListingOptions::new(format)` defaults
`collect_stat: true` (DF 45 `datasource/listing/table.rs:298`; `lib.rs:53` never turns it off), and
those statistics are what put the smaller side on build — why the corpus shows `RightAnti` with the
supplier subquery as build. The plan-shape claims stand on the goldens regardless
(`tpch.sf1/tp4-single.plans.txt:946-975`, `:29-38`).

**F10 — minor — the injected set is misquoted, and `Full` has no unit test.** The injected set is
`anti_join`, `q97`, `q93`, tpcds `q16` and seven others (`test_cpu_end_to_end.rs:378-390`); tpcds
`q87` is `end_to_end!` only (`:351`). `Full`'s empty-build path — per-call `Right` then a `LeftAnti`
finish over an empty build with `need_produce_result_in_final` — is therefore proved only by the
injection cover on q97. Add `Full` to the proposed `cpu_backend/tests/join.rs` case beside `Right`
and `RightAnti`.

**F11 — minor — the q77 → #189 re-attribution is a prediction.** The proposal reads it off the
plan (`tpcds.sf1/tp4-single.plans.txt`, `== q77` line 7, `__grouping_id:UInt8` in the top scatter's
keys — confirmed) and the schedule (the top scatter runs only after every lane below is done, so
#175 refuses first today). Have the developer run q77 at one tp4 mode by hand with the cell still
disabled and record the `Unsupported data type in hasher: UInt8` signature in the detail file
before writing 189 into the registry.

**F12 — minor — say what Task 4 of the impl plan should now expect.** The spec's 244 counted every
permanently-empty lane whatever its consumer; under the guard it is 0 (re-measured, F-verified
below). `empty-build-impl.md` Task 4 tells the developer to treat a number far from 244 as worth
understanding; the corrected expectation is: no existing section moves, and the six placeholder
`== q16` sections ("skipped: not enabled at this mode", e.g. `tp4-single-mini.cpu.txt:991-992`)
become real ones.

## 3. Claims verified

- Refusal sites and routing: `single_partition.rs:183-185` (`NoBuild`), `:187-192` (`Draining`),
  `:318-330` (`without_build`, `Err` ends the query); `cpu_backend/join.rs:184-192`,
  `gpu_backend/join.rs:103-114`, same message; `Calls.empty_build_answers_nothing` at `:40-43`,
  set at `:79`, `:99`, `:163`; `plan/join.rs:452-461` table; `executor/mod.rs:204-211` trait method,
  `:453-455` `CallKind::NoBuild` doc, `:578-600` `IndexedNode`.
- The drop and the typed empties: `partitioned.rs:380-391`; `cpu_backend/emit.rs:73-75`
  `RecordBatch::new_empty`; `node_session.cpp:414-417` deep-copied `cudf::slice(pv,{start,end})`
  with names; `gpu_backend/emit.rs:66-74` refuses fewer than N handles; hold lift at
  `partitioned.rs:331-333`; `LaneSite` filled at `:296-303`.
- Coalesce over one empty: `cpu_backend/accumulate.rs:138-141` (`held.is_empty()` is a batch
  count), `gpu_backend/accumulate.rs:209-212`, C++ concat at `node_session.cpp:329`; the throw at
  `:280-283` is unreachable from Rust; `gpu_backend/accumulate.rs:248`, `:387` return `Ok(empty)`;
  `finish_without_keys` (`gpu_backend/join.rs:209-238`) is #173's one refusing site.
- q16 shape (a): `tpch.sf1/tp4-single.plans.txt:946-975` — `RightAnti`, build
  `CoalesceAllBatches ← EmitPartitions[s_suppkey] ← Merge ← Filter(LIKE) ← supplier`, probe
  `partsupp ⋈ part` scattered on `ps_suppkey`; 4 filtered suppliers
  (`tp1-single-mini.cpu.txt`, `== q16`, `GpuFilter … output_rows=4`); 799,680 = 800,000 − 4×80.
- q77 shape (b): `tpcds.sf1/tp4-single.plans.txt`, `== q77` lines 13-26 and 36-46 — the `Right`'s
  build is `Project ← AggregateBatches ← Aggregate ← Project ← HashJoin{Inner}(store ⋈ store_returns)`,
  probe the same over `store_sales`, both `hashed_on=[s_store_sk]`, no scatter between the
  aggregates and the join; every `store` scatter in all three tpcds tp4 `.cpu.txt` files shows
  lane 2 empty (`[[5],[5],[],[2]]` ×15, `[[5],[5],[],[1]]` ×1 per file).
- Goldens: 9 owing joins per tpcds tp4 file (q40, q78 ×4, q87 ×2, q93, q97) and 1 per tpch tp4
  file (`anti-join`); on every one the build-side emitter's per-lane batch count equals its call
  count (e.g. q93 `[3,3,3,3]` over 3 calls, q40 `[2,2,2,2]` over 2) — zero kept empties in existing
  sections. No emitter in any tp1 file. `.plans.txt`, `recipe-payloads.txt`, `--- memory ---`
  untouched (nothing plan-time changes). The cost-regression gate omits labels with no base
  (`cost-report/src/main.rs:1214-1222`), so the new q16 tp4 sections cannot fail it.
- Registry and corpus: line 17 is tpcds q16 (tickets `59 62 80 97 152 187`, no 175); 78 tpcds q77
  and 116 tpch q16 carry `175`; 121 tpch q21 is enabled at all five cpu modes, no 175.
  `corpus_cases.inc:95` (q16 tp1 only, gpu oracle `live_cpu`), `:89-93`, `:235-238`, `:243`, `:115`;
  `test_cpu_end_to_end.rs:357-365` (q2 stands in for q77), `:784-843`; `build-test.md:42`;
  `tickets.md:244-256` names q21.
- Scaffolding: `injection.rs:576-615` field and walk, `:664-667`, `:767-777` filters;
  `test_layout_injection.rs:96,:102`; `driver/mock.rs:85-91`, `:149`, `:519-525`;
  `driver/tests/flow.rs:229-249` (sets the knob, no join type — hacks-audit "tests that would not
  catch the bug" 1); `executor/tests.rs:97-99`; `injection.rs:308-310`; `cpu_backend/backend.rs:245-247`;
  `gpu_backend/backend.rs:273-275`; `index/tests.rs:78` quote; `driver/plans.rs:101-112` `join`
  builder (real `GpuHashJoin`, so `as_node_ref` in the index is safe — `category_of` already uses
  it, `plan/mod.rs:1185`); `exec_model/single_partition_driver.py:170` raises.
- C++ and cuDF: `join.cpp:170-185` builds `filtered_join` directly over `left_keys` for
  `RightAnti` (`:153-168` `RightSemi`); `:295-302` `Right` = `left_join(right_keys, left_keys)`,
  `:312-329` NULLIFY gather; `filtered_join.cu:105-130` `grid_size(_build.num_rows(), CGSize)` with
  no empty path, ctor `:219-231`; `semi_join.cu:56-64` guards `LEFT_ANTI && right empty` with
  `thrust::sequence`; `join_utils.cu:34-45` `is_trivial_join`; `hash_join.cu:516,538,785-787`
  `_is_empty` → trivial left-join indices; `join.cu:70-91` free `left_join` builds `hash_join` on the
  right table. `wire/join.rs:40-91`: `Full` is a per-call `Right` plus a `LeftAnti` finish.
  DataFusion 45 `hash_join.rs:1463-1530` has no empty-build shortcut.
- The two lying comments: `cpu_backend/accumulate.rs:129-137`, `gpu_backend/accumulate.rs:186-191`,
  `cpu_backend/tests/accumulate.rs:67-69`.
- Accounting: a zero-row Int64 batch prices at 0 bytes (`common.rs:23-41`); `hold`/`release` count
  calls, not bytes (`accounting.rs:120-137`), so kept empties pair like any other. Zero-row batches
  already appear in goldens (`tpcds.sf1/tp4-single-mini.cpu.txt:3170`), and
  `test_corpus_goldens.rs:209-282` sums rows, so a kept empty satisfies both laws.
- The hash injection: `Empties` is source-side only (`injection.rs:193-222`); the murmur3 path
  casts `Utf8View` to `Utf8` (`spark_partitioning.rs:56-70`), so q16's string keys at tp4 hash.

## 4. Corrected proposal

Only the sections that change.

### 3. Localized fix — amendments

- **Spec first.** Before dispatch the human amends `empty-build.md`: the Scope table gains
  `executor/driver/single_partition.rs` (one state, one call, one arm), `executor/mod.rs` (the
  `JoinExecutor` trait loses `without_build`; `CallKind::NoBuild` doc), `driver/mock.rs`,
  `tests/common/injection.rs`, `cpp/src/operators/join.cpp` (the zero-row guard); the Restriction
  drops "no `without_build` signature change" and "code changes are limited to the conditional
  drop…"; §6 gains Change 1's two flow tests; "Coverage" says q77 lands on #189, not #152. If the
  human declines Change 1, ship Change 2 alone, leave the injection gate, and re-attribute q77 to
  "#175, then #189" in the corpus comment.
- **Change 1, `OwedProbe` arm:** a zero-row probe batch is released (`ReleaseUnwanted`), not
  refused; the refusal fires on the first batch with rows.
- **Change 1, deletions:** add `driver/single_partition/tests.rs:259`.
- **Change 2, C++ guard, one form only.** In each semi/anti arm that constructs `filtered_join`,
  when the table it would be built from has zero rows: anti → `single_indices` =
  `thrust::sequence` over the other side's row count; semi → an empty `device_uvector`. For
  `RightSemi`/`RightAnti` that table is `left_keys` (build); for `LeftSemi`/`LeftAnti` it is
  `right_keys` (the probe batch). No free `cudf::left_*_join` call anywhere inside the
  `PEACOCK_HAVE_FILTERED_JOIN` branch — it is deprecated on 25.10 and absent on 26.02.01. Clang-format
  the changed lines only.
- **Change 2, driver test:** `MockAcc::mark_done_and_fetch` under `CoalesceAll` returns nothing
  when `held_rows == 0 && nothing arrived` (track an arrival count; `held_rows` alone cannot
  distinguish an empty arrival from none) — mirroring both backends — and the test then asserts
  four `SetBuild` under `Right` and one `SetBuild` + three `NoBuild` under `Inner`. Also assert the
  emitter's `Emit` produced count (4 vs 1), which is the line that actually changed.
- **Unit, CPU:** `cpu_backend/tests/join.rs` covers `Right`, `RightAnti` **and `Full`** over
  `set_build(RecordBatch::new_empty)`: Full's finish is a different DataFusion path.
- **Goldens:** three tpch tp4 `.cpu.txt` + `.cost.txt` (the `== q16` placeholders become sections),
  **and `tpch.sf1/mini.result.txt` q16's `mode=` line → `tp4-sized`**; nothing else.
- **Device:** the shad-gpu run proves `Right` over a zero-row build and the 25.02 free-function
  anti path. It does not reach the `filtered_join` guard; record that in the detail file as
  unproven-by-reading, with the ≥25.10 run as the outstanding proof.
- **q77:** run at one tp4 mode by hand (cell disabled) and paste the `UInt8` hasher signature into
  the detail file before the registry says 189.

### 6. Cells re-enabled — amendments

- Change 1 re-enables nothing today; say so. `tpch/q16` × three cpu tp4 cells is the whole gain.
- The injection hash dimension returns for `anti_join`, `q97`, `q93` (and tpcds `q16` if it carries
  an owing join); `q87` is not injected.

### 7. Risks and unknowns — amendments

- **#175 does not close.** Residual: a `Right`/`Full`/`RightAnti` lane whose build received no
  batch because a join below drained or a mid-plan limit dropped everything, with probe rows.
  Refused by name at the first probe batch on both engines. Narrow the ticket to that shape (or
  close and file), and narrow `architecture.md:855-857` to match.
- The `filtered_join` zero-row launch is latent in all four semi/anti arms on ≥25.10, and no device
  in the workflow runs ≥25.10.

### 8. Complexity

Unchanged at **M**, with two additions the estimate should carry: a spec amendment round with the
human before dispatch, and a shad-gpu run that proves less than §8 claims.

## 5. Complexity

**M**, agreeing with the proposal. The Rust is small and mostly deletion; the C++ is ten lines; the
goldens are additive plus one result line. What the proposal's M leaves out is not code: the frozen
spec has to move first, and the one device proof it promises is not available on the host the repo
tests on.
