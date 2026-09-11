# Review of 10-fixes-v1.md

Read at master `188c23ce`, read-only. Every file:line the list cites was opened; the cuDF
headers at `/media/data/miniforge3/envs/{rapids-cuda-12.2,rapids}/include/cudf` (25.02 and
26.02), the vendored `arrow-*-54.2.1`, `datafusion-*-45.0.0` and `parquet-54.2.1` under
`~/.cargo/registry/src/*/`, the tpch/tpcds goldens, `testdata/cost-registry.csv`, the wiki pages
the template names, and the 186/188/175/192 records. Nothing built or run.

## 1. Verdict

**Needs changes.** The two decisions are right and well evidenced — F13's planner-one-batch
design and F10's retitling of #185 both survive the code — but the coverage arithmetic books
cells on the wrong step in three places (four gpu cells at step 7 that #152 still refuses, ten
gpu cells at step 10 that #198 turns into wrong answers, and ten cpu cells F15 creates from the
`na` pool that the table never counts), F12 is the cast-at-the-unload half of the approach the
human rejected and is booked as a closed ticket rather than as the decision it is, and the
chain-placement rule ("after task 10") lands every C++ fix on top of tasks 11-13.

## 2. Findings

### F-1 — important — F12 is casts.md's cast at the unload, minus the prediction; it cannot be booked until the human rules

Evidence. `archive/archived-tasks.md:286`: *"Predicting the export type at plan time and casting
at the unload builds the divergence into the plan; the operator harness reports it instead."*
The same commit (`188c23ce`) rewrote `operator-harness-impl.md:148-151` to expect that "a fix for
#187 would add" a per-column precision to `TableResult` — a C++-side fix — and
`operator-harness.md:58` now reads "#187, open and owned by no task". F12 maps every decoded
device batch through `declared_as` at `gpu_backend/mod.rs:176-183`: that is the unload cast,
generalised from casts.md's per-column list to the CPU's whole relabel-and-narrow rule.

What is verified in F12's favour: on 25.02 `cudf::interop::column_metadata` is `{name,
children_meta}` only (`rapids-cuda-12.2/include/cudf/interop.hpp:108-119`); 26.02 adds
`std::optional<int32_t> precision` (`rapids/include/cudf/interop.hpp:110`). So on shad-gpu the
exporter cannot be told a precision through cuDF's API at all, and a "fix at its source" there
means the C++ rewriting the Arrow C-data `format` string (`d:38,2` → `d:15,2`) from a declared
precision it must first be handed — by the wire (rejected: "not carried as a per-column width")
or by a new ABI symbol (owned by nobody). `declared-schemas.md:136-137` ("`declared_as` already
asks it on the CPU side, and that is where the production fix will start") is an approved
sentence that points at F12's shape. And F12 leaves the harness's view intact: the walk and the
catalog export through `peacock_result_from_handle`, not the unload, so the raw divergence is
still what they measure.

What is wrong in the list: F12 is written as "closes #187", priced M, and its cells are booked
at step 7 of the table, with the decision tucked into one sentence. F12's own text also
overstates the alternative: "the only remaining shape is a seventeenth ABI symbol handing the C++
the declared schema" — that symbol alone does nothing on 25.02; the C++ would still have to
patch the exported schema by hand.

Correction. Move F12 out of the fix list into "Decisions for the human" beside F18's fbs
question, stating the three shapes (relabel via `declared_as` at the unload; a declared
precision handed to the C++ by ABI, with the C++ patching the C-data format string; give up
and keep #187 as `bug_` tests until 26.02 is the floor). Book its cells as conditional and say
which other rows land on #187 if it is not taken (F14's decimal-sink rows, F11's 29-row
re-read).

### F-2 — important — coverage arithmetic, gpu: four cells at step 7 are still behind #152, and ten at step 10 are behind #198

Evidence, step 7: `tpch.sf1/tp4-single-mini.cpu.txt`, `== anti-join`, the probe emit's line:
`GpuEmitPartitions: hash=[o_custkey@1] … batch_rows=[[92337,91970,92436,98520],[92437,…],…]` —
**four probe batches per lane** at tp4-single (the four merged input batches, each scattered
once), and `== semi-join` is the same shape. A RightAnti/RightSemi call copies its build side
per probe batch (`gpu_backend/join.rs:304-314`), so the second probe batch refuses. F11's text
already says this for the rowgroup and sized modes ("13 at tp1-rowgroup") but F11, F12 and the
table all count `anti_join, semi_join × 2 (tp1-single, tp4-single)` before F14. Only tp1-single
(one probe batch: `partition_groups=[[[0..11]]]`) is F10+F11+F12's.

Evidence, step 10: `tpch.sf1/tp1-single.plans.txt` q13 is `GpuHashJoin: join_type=Left`
under `count(o_orderkey)`; `tpcds.sf1/tp1-single.plans.txt` q97 is `join_type=Full` under
`CASE WHEN … IS NOT NULL AND … IS NULL`. Both pad their unmatched build rows through the at-done
project (`wire/join.rs:113-121`, `null_literal` per probe column), whose bare numeric NULL is
`is_ast_able` (`expr.cpp:414`) and is built valid by `build_expr` — #198, `typed-nulls.md`
(task 11). A typed zero is counted by `count(o_orderkey)` and fails `IS NULL`, so q13 and q97
answer wrongly on a device until task 11 lands; their result goldens would catch it. F14's own
text says "land it after task 11"; the table's step-10 assumptions do not.

Correction. Step 7: 19 → 17 (`anti_join`, `semi_join` × tp1-single only). Step 10: 20 → 22,
with "after task 11" in the assumptions column for q13/q97 (and for every Left/Full row in the
79). Total stays 80.

### F-3 — important — coverage arithmetic, cpu: the `na` cells F15 creates are never counted, and q72's next wall names a ticket the same table closed eight steps earlier

Evidence. `cost-registry.csv`: q27 and q72 are `plan_status=fail`, all cells `na`; the table's
closing line counts only q28's five `na` ("156 disabled + 5 na (q28) = 161, all accounted
for"). F15 makes q27 and q72 plan, which turns ten more `na` cells into cpu cells. The "never"
row then says "q27 cpu on #163 then #152/#183" — but #163 is F19 (step 9) and #152/#183 are
device tickets; q27's cpu cells (`avg` ×4 under `ROLLUP(i_item_id, s_state)`) are candidates
after F15 + F19 + F2, all of which are in the table. F15's text sends q72's cpu tp4 cells to
"#180 (F3)": #180 is closed at step 4, and q72's counts are grouped (`group by i_item_desc,
w_warehouse_name, d_week_seq`), which is not #180's keyless shape.

Correction. Step 12 becomes `0 (+5 q27 after F19, +3 q72 tp4, +2 q72 tp1 if it fits)`; the
closing line becomes "156 disabled + 15 na (q27, q28, q72) = 171 reachable, 3 walled (q77),
90 → 75 na"; the "never" row drops q27.

### F-4 — important — F6's part C is #199's, not #173's, and it fixes the aggregate's gap at the limit

Evidence. F6 C makes `LimitStream` (both backends) keep a zero-row spare and emit it at done
"iff nothing was emitted", so that a keyless `GpuAggregate{final}` above it gets a batch and
answers its identity row — the list's own ticket 13 ("a `GpuAggregate{final}` shortcut over a
lane with no batch emits no row … F6's C hands it a zero-row batch, which then owes the
identity row"). The node that owes the row is the finalizing aggregate; F3 already states that
rule ("only the node that finalizes a global aggregate owes the SQL row") and puts it on
`AggregateBatches`. Making a limit manufacture a batch so a different node's shortcut fires is
the shape `coding-style.md` calls building around a bug, and it changes `architecture.md:194`'s
"streams and holds nothing" for a case #173 never named. It also collides with the recipe rule
the 188 review argued: an executor should not make a per-call ABI symbol its recipe does not
name (the `GpuLimit` recipe says `slice_handle` per straddling batch, not at done).

Correction. Split C out of F6. Either fold it into F3's family as "the single-node shortcut
over no batch owes the identity row" (the driver knows a lane ended with no batch) or leave it
on #199 as the residual F3 names. F6 then is A (device `finish_without_keys` seeded from the
build handle) plus the one-call finishing types' at-done answer on both backends — and that is
still two backends, ~8 tests and a `backend.rs` signature change: **M**, not S.

### F-5 — important — "before task 4 or after task 10, never during" is wrong by three tasks

Evidence. `tasks.md`: task 11 (`typed-nulls`) edits `cpp/src/expr.cpp` and
`cpp/tests/gpu/test_plan_executor.cpp`; task 12 (`empty-build`) edits `cpu_backend/join.rs`,
`gpu_backend/join.rs`, `cpu_backend/accumulate.rs` and regenerates six `.cpu.txt`; task 13
(`walk-drives-every-plan`) rewrites `test_gpu_recipe_walk.rs`. F4, F5, F7, F8, F17 edit
`expr.cpp`; F6 and F10 edit both `join.rs`; F10 regenerates all ten `.cpu.txt`; F7, F9 and the
five stale pins edit the walk. The list's per-fix notes name these collisions individually and
the chain-placement paragraph then says "after task 10".

Correction. "Before task 4, or after task 13." State the practical consequence in one line:
with tasks 3-13 approved and the chain unattended, nothing here lands on master without either
the human holding the chain at task 3 (F10 first, with F1/F2/F3/F13's sections) or waiting for
task 13.

### F-6 — minor — F13's doc argument counts sentences the design also rewrites, and its edit list omits three of them

Evidence. `architecture.md:377-378` ("`CudfScan.limit` becomes `set_num_rows` on every scan
call"), `:814`, `:1043` and `:1073` (the three tables naming `set_num_rows(limit)`) all become
false when F13 (d) deletes `scan.cpp:61-63` and slices instead — the same four the "counter
design rewrites six" bullet lists against the alternative. F13's fix section names only
`:374-379` and `:97-100`. Only `:763` (a `slice_handle` recipe arm) is genuinely avoided.

Correction. Add `:377-378`, `:814`, `:1043`, `:1073` to F13's edits; reduce the "two
decisions" bullet to the two claims that hold — one owner (the planner), and no arm in
`a_loaders_batches_line_up_with_the_row_groups_that_made_them` (`test_corpus_goldens.rs:289-320`,
which demands `emitted == count` under `early_exit=none`).

### F-7 — minor — two of F13's supporting sentences are overstated

- "injection never re-cuts a mapping (`injection.rs:558-572`)": those lines are `drained`,
  which does re-cut a mapping (lane 0's batches move to lane 1). What holds is that a one-lane
  mapping is left alone (`groups.len() > 1` guard), and the `Empties` dimension wraps the
  executor, not the plan. Say that.
- "hacks-audit production bug 1 stays unreachable — scan-limit's sink has five strings": the
  counter design keeps it unreachable in the corpus too (its §7: reachable only for a limit
  larger than the first batch, which no query has). Not a discriminator; drop it.
- The restructured e2e assertion "`pulled == non-empty source lanes` (two at tp4-single: `part`
  is four lanes, two non-empty)" forgets `region`'s one lane — three at tp4-single. Assert the
  formula, not the number.

### F-8 — minor — F10's "`mini.result.txt` byte-identical" has a second caveat

Evidence. `architecture.md:715-720`: no sort preserves tie order, and a partial-aggregate's
group order depends on arrival order. After F10 a `GpuAggregate` above a join sees one batch
instead of 36, so the merged group order can move, and an `ORDER BY … LIMIT n` whose sort key
does not determine the boundary row can return different rows — a different set, not different
low bits. Verify the `.result.txt` diff per query rather than expecting none; the Float64
caveat is the same instruction for a different reason.

Also worth one sentence: `concat_batches` over a 733-chunk join output (tpch `hash-join` at
tp1-single, ~1 GB) is a 2× transient on the CPU tier, which runs with no budget; fine on a
15 GiB host, and the reason `q64` should not be re-enabled on the same day.

### F-9 — minor — F11's ".cpu.txt expected byte-identical" rests on a property arrow does not guarantee

Evidence. `common.rs:107-119`: `Utf8` content is `offsets[rows] - offsets[0]`; `Utf8View`
content is Σ over *valid* values. They agree only where a `Utf8` array's null slots carry no
bytes. Kernels usually produce zero-length nulls, but `nullif` keeps the values and only masks
them, and a `RecordBatch::slice` of such an array carries them too. Read the regen diff for
string columns with NULLs (outer-join pads, CASE without ELSE) before calling the goldens
unchanged.

### F-10 — minor — #47's "stale" verdict is by inference until the pin runs on a device

Evidence. e5d2c0e7 and d3c92de5 (2026-06-11) postdate the filing commit 2d07e908 (2026-06-10)
and are in HEAD; today's CPU answer is 44 rows (`tpcds.sf1/mini.result.txt`, `== q77`, 45 lines
with `|` including the header). "44 − 5 = 39 matches the recorded loss modes" is arithmetic,
not evidence that a device now answers 44. #46 (the wrong figure reproduced under the old
semantics), #45 (`expr.cpp:912-934` carries the arm and the ticket number), #55/#56
(`aggregate_writer.rs:78-81` never writes `Final`, `:177` never sets `distinct`) and #60 (the
five rows re-emulated) are stronger. Archive #47 only after `ROLLUP_OVER_LANES` is green on
shad-gpu, in the same step, and say so in the closure.

### F-11 — minor — F15 groups two unrelated DataFusion defects because they share a ticket number

`MergeInputSort` (a physical-optimizer bug around a union of constants) and
`DateDayArithmetic` (a type-coercion gap) share nothing but `build_session_state` and #23.
Each has its own rule, tests and goldens. One branch is fine; the grouping check below marks
it as a convenience, not a mechanism.

### F-12 — minor — F14 is M as code and L as a task

Seven files and one ABI symbol are M; the 79-row registry pass "in batches of about five" is
sixteen shad-gpu rounds, the same order as F19's 115 sections, which the list bands L for that
reason. Say the same for F14 and recommend the same split: code + the walk + q19 in one branch,
enablement in batches.

### F-13 — minor — the F13/F4 handoff on `nested_limits` should name what the device does with `region` today

`wire/node_writer.rs:80-100` writes the projected fields as `file_schema` and no
`projection`; `scan.cpp:37-52` then builds `projected_names` from an empty `file_schema` and
passes `.columns({})`, a present empty selection, so the device reads a zero-column, zero-row
table where the CPU's `ProjectionMask::roots(schema, [])` reads zero columns with a row count.
The list has this right in ticket 1 and F4 (c); F13's "cells re-enabled" row should say
`nested_limits` gpu fails at the `region` scan the first time it is run after F13, so nobody
files it twice.

## 3. Grouping check

- **F1** (#190) — holds. The cross-join projection (ticket 7) is a different mechanism (no
  fbs field on `CudfCrossJoin`), so the split is right.
- **F2** (#189) — holds.
- **F3** (#180, narrows #199) — holds. Take F6 C into this family (F-4).
- **F4** (#63 + the zero-column scan) — holds; one placeholder, one spelling.
- **F5** (#191) — holds.
- **F6** (#173 A + the one-call finishing types + C) — **split**: C is #199's (F-4). A plus the
  finishing-type answer stay together; both are "the finish holds a typed table and drops it".
  No contradiction with `empty-build.md`: that spec lists `finish_without_keys` under "Not
  touched" as #173's surviving site (`empty-build.md:166-168`), so F6 A edits exactly what task
  12 leaves alone, and neither adds a probe call (its §5 hazard).
- **F7** (#60) — holds; the DESC-nulls defect is its own S change and is verified (cuDF flips
  the null precedence with the column order: `row_operators.cuh:647-650`; the engine maps
  `nulls_first` to `BEFORE` regardless, `sort.cpp:45-46`, `node_session.cpp:314-315`).
- **F8** (#57) — holds.
- **F9** (#65) — holds; `aggregate.cpp:388-392, :414-415` fold LSB-first into `int32_t`,
  DataFusion folds MSB-first (`aggregates/mod.rs:1274`) at `UInt8/16/32/64` by key count
  (`plan.rs:3223-3233`).
- **F10** (#185 retitled) — holds, and it is the strongest reading in the list: the q93 golden
  shows one probe batch (`in_rows=[[287867],[2880404]]`) chunked into 36 by DataFusion's
  per-chunk bound (`hash_join.rs:1471-1479`), and the device answers one table per call
  (`gpu_backend/join.rs:164-179`). #185's "every `batch_rows` entry agrees" cannot be true of
  that section.
- **F11** (#183 + #192) — holds; #192's dissolution is a prediction with a measured re-enable
  gate, which is the right way to carry it.
- **F12** (#187) — **not a fix yet**; a decision (F-1).
- **F13** (#186 + #188) — holds; one design.
- **F14** (#152) — holds as the strict prefix of `refcounted-tables.md` §2-§4 with `retain` as a
  deep copy; the human must carve the spec.
- **F15** (#23 q27/q72) — holds by ticket, not by mechanism (F-11).
- **F16** (#62) — holds.
- **F17** (#168) — holds; the two refusals it leaves are correctly filed rather than folded in.
- **F18** (#184 + #95) — holds; same kernel switch, same missing arm. Its fbs field is a
  per-key precision on the wire and the list rightly hands the shape to the human.
- **F19** (#163 CPU + the avg finalize scale) — holds; 3b is discovered by reading and would
  stop 19 of 23 one step later, so the same branch is right.
- **Stale closures** — #45, #46, #55, #56 hold on git or code evidence; #47 is the weak one
  (F-10).
- **Duplicates** — `#175 → empty-build.md` holds, and amendment (i) is verified against the q77
  tp4-single plan: the `Right`'s build is `Project ← AggregateBatches ← Aggregate ← Project ←
  HashJoin{Inner}` over the `store` scatter (`partition_groups=[[[0]],[],[],[]]`), and the
  spec's climb stops at the Inner join. `#198 → typed-nulls.md` holds. `#152 →
  refcounted-tables.md` holds.

## 4. Ordering check

The landing order stands except for these moves:

1. **F12 leaves the sequence** until the human rules (F-1). If it is taken as written it keeps
   its place at step 7; if not, F11's 29-row re-read and F14's decimal-sink rows land on #187
   and the table says so.
2. **Task 11 (`typed-nulls`) enters the table before F14** — as a row, since ten of F14's
   twenty cells depend on it (F-2). F8 and F6's Left/Full arms already wait on it.
3. **`anti_join`/`semi_join` × tp4-single move from step 7 to step 10** (F-2).
4. **F15 gains its cells** (F-3); its place after F19 is right because q27 needs both.
5. **The chain gate** (F-5): either F10 (+ F1/F2/F3/F13's sections, all cpu-only) before task
   4 with the chain held at task 3, or everything after task 13. The list's "after task 10" is
   the one order that cannot be executed.
6. **#47 archives after its pin's device run**, not in the same commit as the pin (F-10).

## 5. Verified

- Registry today: cpu 444/156/90, gpu 6/594/90 (counted from `cost-registry.csv`); the 156 cpu
  disabled cells sum to the list's attribution (115 #163, 10 #190, 9 #189, 9 #180, 2 #186/#188,
  5 #192, 6 #175).
- F1: `cpu_backend/join.rs:140-146` passes `None`; `nodes.rs:505-512` keeps the projection;
  `join.cpp:519-529` applies it; `hash_join` at `:303-313` passes it.
- F2: `aggregate.rs:39-67` copies the hash keys; `:370` consumes the shuffle unchanged;
  `tpch.sf1/tp4-single.plans.txt:2738` hashes `__grouping_id@2`; q77's tp4 shuffle hashes the
  gid too, so #189 is q77's second wall as the list says; q5/q80's tp4 build sides are big
  scattered return tables, so no #175 behind F2's nine cells.
- F3: `Count::state_fields` is `Int64, false` (`count.rs:163-166`); `RecordBatch::try_new`
  refuses nulls in a non-nullable field (`record_batch.rs:296-298`); `mark_done_and_fetch`'s
  `!self.grouped` clause at `accumulate.rs:372`; no enabled tp4 golden has a keyless
  `GpuAggregateBatches` over an empty lane, so "no golden regenerated" holds.
- F4: `project.cpp:20-31` emits `__rowcount__`; `join.cpp:390-398` appends both name lists;
  the scan writes no `projection` and an empty `file_schema` becomes `.columns({})`.
- F5: `expr.cpp:659-660` returns the component uncast; `infer_expr_type` (`:395-397`) is the
  only reader of `return_type`; `is_ast_able` refuses every scalar function (`:403-406`).
- F6: `finish_without_keys` at `gpu_backend/join.rs:209-238`; the filtered LeftSemi/LeftAnti/
  LeftMark take `answers_in_one_call` (`plan/join.rs:427-435`, `plan/mod.rs:772-774`) and
  `finish: None`, so `finish_and_fetch` returns nothing over no probe batch on both backends
  (ticket 3 is real); the pad and narrow projects read the build schema (`wire/join.rs:261-322`).
- F10: `declared` at `cpu_backend/join.rs:269-273`; `CpuExec::exec` concatenates
  (`mod.rs:177-205`); the device tier compares the section before the result
  (`corpus_gpu.rs:101-105`), `line_difference` prints the first differing line
  (`golden_text.rs:158-187`); `nested-limits` is the only golden carrying `abandoned=` and
  `test_golden_format.rs` has no case for the arm; the nine #185 rows' sinks carry no
  `Utf8View` and no narrow decimal (q93's is `Decimal128(38,2)`), so they did pass the unload.
- F11: `lib.rs:52` uses `ParquetFormat::default()`; `schema_force_view_types` defaults `true`
  (`config.rs:430`); `infer_schema` applies `transform_schema_to_view` (`parquet.rs:370-371`);
  `as_declared` casts at `source.rs:112-139`; the comet key arms at `spark_partitioning.rs:53-71`;
  the four `Utf8View` assertions at `schema_tests.rs:157,286,306` and `plan_text/tests.rs:151`;
  no integration test asserts `Utf8View`.
- F12: `widened_decimal` at `cpu_backend/mod.rs:499-506` is also read by `check_state_layout`
  (`:478`), so a private move would not compile; 25.02 has no `precision` member (above).
- F13: `lanes_for` returns 1 for a limit and `source()` hands the mode's batching to
  `partition` (`nodes.rs:374-396`); `plan/source.rs:21-50` validates nothing about a limit;
  `pipeline.rs` validates every tree it returns (`checked`); `test_layout_injection.rs`
  validates only `merge_over_sorted()` (built on `source(None)`); `scan.cpp:61-63` sets
  `num_rows` and `:77-78` the row groups; `NodeStats` are computed from the returned view
  (`node_session.cpp:238-242, 502-505`); the e2e test's `most_offered > 2` rests on `part`'s
  two batches at the rowgroup modes (`test_cpu_end_to_end.rs:426-526`, the tp1-rowgroup golden's
  `early_exit=GpuUnload@6,GpuLimit@3` over `[[[0],[1]]]`); the `region` loader carries
  `projections=[], limit=23` at every mode.
- F14: `copy_of`/`build_copy` at `gpu_backend/join.rs:293-314`; no test pins the ABI symbol
  count; `refcounted-tables.md` §2 is the same symbol with the same new-id semantics.
- F17: `serialize.rs:100-102`, `expr_writer.rs:32-37`, `test_plan_goldens.rs:375-380` and
  `NOT_RUNNABLE` as cited.
- F18: `spark_hash_partition.cu:163-185` lists STRING/INT32/INT64 with `CUDF_FAIL` in `default`;
  q15's tp4 emit hashes `total_revenue@4:Decimal128(38,4)`.
- F19: `Avg::state_fields` is `[count: UInt64, sum: input type]` (`average.rs:154-167`);
  `Sum::return_type` widens a decimal by ten (`sum.rs:158-162`); arrow's decimal `Div` is
  `s1 + 4` and `p1 + 4 + s2` (`arrow-arith/numeric.rs:791-798`), so `(19,6)/(19,0)` is
  `(23,10)` and the second wall is real.
- Stale pins: `aggregate_writer.rs:78-81` never writes `Final`, `:177` never sets `distinct`;
  `expr.cpp:912-934` carries the string identity arm with `#45`; d3c92de5 landed
  `NULL_LOGICAL_AND/OR` (`expr.cpp:118-119, :513-514`); the four commits cited are in HEAD.
- Walls: q77's shape (b) as above; `cost-registry.csv` carries `65` on nine rollup rows.
- The claim that no enabled device cell sorts (q6, q19 tp1-single) and that the nine #185
  candidates have no NULL sort keys, so the DESC-nulls defect is inert for every cell the table
  books.
