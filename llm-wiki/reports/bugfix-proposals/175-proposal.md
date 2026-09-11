# #175 — an empty build side leaves Right / Full / RightAnti owing rows they cannot make

Read at master `c18e063a`. Paths relative to `/media/data/peacockdb`. Nothing built or run; the
one measurement below is a Python parse of the committed `.cpu.txt` goldens (script under
`175-work/`).

**Relation to the existing specs, up front.** `llm-wiki/tasks/empty-build.md` (frozen, board task
12, `approved to build`) is the current spec for #175 and its mechanism — keep the scatter's typed
zero-row batch where a join above owes rows — is half of what follows (Change 2 below). This
reading finds it has the corpus wrong: it does not reach `tpcds/q77` at all, its golden prediction
does not match the goldens, and its "not touched" list includes a C++ arm that does need one line.
The other half (Change 1) is a driver change the spec does not contain and that q77 needs. The
archived `empty-answers.md` (rejected 2026-09-10) proposed a new `ProjectRole`, a routing change and
a make-empty-of-schema call; nothing here needs any of the three, so the rejection stands.

## 1. Issue

A join lane whose build side ended with no batch is asked what it owes (`LaneCall::NoBuild`,
`executor/driver/single_partition.rs:183-185`). Six types owe nothing and drain; `Right`, `Full`
and `RightAnti` owe their probe rows, and both backends refuse by name instead of answering:
`executor/gpu_backend/join.rs:103-114`, `executor/cpu_backend/join.rs:184-192`, the same message
in each. The lane never finds out whether any probe row exists to be owed, and it never sees the
typed zero-row table the scatter already built for it.

Two distinct shapes reach the refusal in the corpus, and they need different fixes:

- **(a) `tpch/q16`** — the `RightAnti` at `testdata/goldens/tpch.sf1/tp4-single.plans.txt:961`
  has build = 4 filtered suppliers (`tp1-single-mini.cpu.txt`, `== q16`: `GpuFilter … output_rows=4`)
  scattered into 4 lanes on `s_suppkey`, probe = `partsupp ⋈ part` with rows in every lane. A lane
  with no supplier owes every one of its probe rows, unmatched. The typed empty table exists
  (`cpu_backend/emit.rs:73-75`; `cpp/src/node_session.cpp:414-417`) and is dropped at
  `driver/partitioned.rs:383-385` before the coalesce under the join sees it.
- **(b) `tpcds/q77`** — the `Right` at `tpcds.sf1/tp4-single.plans.txt` (`== q77`, line 15 of the
  block) has **no scatter feeding its build side**: build is `Project ← AggregateBatches ←
  Aggregate ← Project ← HashJoin{Inner}(store ⋈ store_returns)`, probe is the same chain over
  `store_sales`, both co-partitioned on `s_store_sk` by the `store` scatter below each Inner join.
  `store` is 12 rows over 4 lanes and lane 2 gets none (`tp4-single-mini.cpu.txt:1272`, under `== q19`,
  `batch_rows=[[5],[5],[],[2]]`; every `store` scatter in the three tp4 files shows lane 2 empty). So in lane 2
  the Inner join takes `NoBuild`, drains (`single_partition.rs:187-192`), emits nothing; the
  grouped merge above it emits nothing (`cpu_backend/accumulate.rs:371-377`); the Right join gets
  no build **and no probe**. It owes nothing, and it refuses before looking.

Cells this disables (from `00-tickets.md`, corrected): `tpch/q16` and `tpcds/q77` at
`cpu_tp4_single`, `cpu_tp4_rowgroup`, `cpu_tp4_sized` (`testdata/cost-registry.csv:116`, `:78`;
`tests/common/corpus_cases.inc:95`, `:243`), and their gpu cells behind those. `00-tickets.md`
also cites registry line 17 — that is `tpcds/q16`, whose tickets column carries no `175`; only
lines 78 and 116 do. Also off: the injection matrix's whole hash dimension for any plan with an
owing join (`tests/common/injection.rs:664-667`, `:767-777`), and `tpcds/q77` out of the
end-to-end join cover with `q2` standing in (`tests/test_cpu_end_to_end.rs:357-365`).

The ticket text (`llm-wiki/tickets.md:244-256`) names `q21 at tp4-single`; `tpch/q21` is enabled
at all five cpu modes (`corpus_cases.inc:115`, registry `:121`). The two queries are q16 and q77.

## 2. Root cause

Trace, driver first:

1. `driver/partitioned.rs:380-391` — the scatter loop drops every zero-row output before it is
   held or queued. Both emitters return exactly N batches and the empty ones are real, typed
   tables: CPU `RecordBatch::new_empty(schema)` at `cpu_backend/emit.rs:73-75`; device
   `cudf::slice(pv, {start, start})` deep-copied into its own `cudf::table` with column names at
   `node_session.cpp:414-417`, one handle per lane, refused if fewer (`gpu_backend/emit.rs:66-74`).
2. The coalesce under the join then receives nothing for that lane and emits nothing at done —
   `cpu_backend/accumulate.rs:138-141` (`held.is_empty()`, a batch count, so one kept empty
   batch would be concatenated and emitted), `gpu_backend/accumulate.rs:209-212` (same; the C++
   collapse of one zero-row view is a plain `cudf::concatenate`, `node_session.cpp:329`).
3. `single_partition.rs:183-185` — `awaits_build && !avail.has[BUILD_SLOT]` selects `NoBuild`
   the moment the build side is `done`, whatever the probe side has or will have; `:318-330`
   calls `without_build()` and any `Err` ends the query (`:323-325`, `failed`).
4. `plan/join.rs:452-461` — `empty_build_answers_nothing` is `false` for `Right | Full |
   RightAnti`; `cpu_backend/join.rs:184-192` and `gpu_backend/join.rs:103-114` turn `false` into
   the refusal. The CPU carries a field for nothing else: `Calls.empty_build_answers_nothing`
   (`:40-43`), set at `:79`, `:99`, `:163`.

What is actually true at that moment, per shape:

- (a) The join could be given the scatter's zero-row table and both operators compute the answer
  with no new call. CPU: `HashJoinExec` (`cpu_backend/join.rs:307-319`) has no empty-build
  shortcut — `datafusion-physical-plan-45.0.0/src/joins/hash_join.rs:1463-1530` looks up an empty
  map and the alignment appends every probe row for `Right`/`RightAnti`. Device, `Right`:
  `cpp/src/operators/join.cpp:295-302` is `cudf::left_join(right_keys, left_keys)`, whose
  `hash_join` ctor records `_is_empty` (`third_party/cudf/cpp/src/join/hash_join.cu:516`, `:538`)
  and `probe_join_indices` returns the trivial left-join indices (`:785-787`); the
  `NULLIFY` gather at `join.cpp:312-329` is the pad. Device, `RightAnti`: **not safe as written** —
  `join.cpp:176-178` constructs `cudf::filtered_join` directly over the empty build, and
  `filtered_join` has no empty-build path: `filtered_join.cu:105-130` launches its insert kernel
  with `grid_size(_build.num_rows())`, zero blocks for zero rows. cuDF's own free
  `left_anti_join` guards exactly this before constructing one (`semi_join.cu:56-64`, and
  `is_trivial_join`, `join_utils.cu:34-45`); the `#else` branch at `join.cpp:180-181` is that
  guarded function, but the 26.02 leg takes the `filtered_join` branch (`join.cpp:18-20`).
- (b) Nothing is owed: no probe row will ever arrive in that lane. The refusal is a false
  positive of asking at `NoBuild` rather than at the first probe batch.

`empty-build.md` §2 assumes every corpus instance is shape (a) ("the lane's table already exists
and is thrown away"). For q77 the dropped table is the `store` scatter's, whose consumer is an
Inner join, and the spec's own guard says drop it. The spike's "244 kept batches" is not the
count under that guard either — see §3, goldens.

## 3. Localized fix

Two changes, independent, each small. Both re-enable something on its own; together they cover
every empty-build lane the corpus has, and what remains refused is a shape the corpus does not
contain (§7).

### Change 1 — a lane that owes its probe side waits for a probe batch before refusing

**Where the verdict lives.** The verdict "does this join owe its probe side when its build is
empty" is a plan fact, `empty_build_answers_nothing(join_type)`, and the driver's index already
reads plan facts off the node (`driver/index.rs:82`, `:90`). Read it there, once per join node,
and stop asking the executor. `JoinExecutor::without_build` (`executor/mod.rs:204-211`) is
deleted; with it go both backend refusals and the CPU's carrier field. That is hacks-audit
"propping up" items 4 and 8 verbatim: *"the field, its three initializers, the refusal arm and the
message go, and `without_build` … disappears. The device half … goes with it."*

- `plan/join.rs`, next to `empty_build_answers_nothing` (`:452`): 
  ```rust
  /// The lane-level reading of the table above, off the node: cross and nested-loop joins
  /// carry no type and owe nothing, every row they emit being built from a build row.
  pub(crate) fn owes_probe_when_build_empty(node: &dyn GpuNode) -> bool {
      match as_node_ref(node) {
          NodeRef::Join(join) => !empty_build_answers_nothing(join.join_type),
          _ => false,
      }
  }
  ```
  (re-export through `plan/mod.rs` beside `empty_build_answers_nothing`, `:1287`.)
- `executor/mod.rs:578-600` `IndexedNode` gains `owes_probe_when_build_empty: bool`;
  `driver/index.rs:83-94` fills it in `walk` from the function above (only a `Join` category node
  can be `true`).
- `driver/single_partition.rs`:
  - `LaneState` (`:104-116`) gains `OwedProbe` beside `Draining`: *a join whose build side was
    empty and whose type owes its probe side; nothing is owed until a probe batch exists, so the
    lane waits — a probe side that also ends empty owes nothing, and one that produces is refused.*
  - `LaneCall` (`:41-59`) gains `OwedProbe` — consumes `PROBE_SLOT` (`consumes`, `:66-75`).
  - `LaneSite` (`:80-88`) gains `owes_probe_when_build_empty: bool`; `partitioned.rs:296-303`
    fills it from `self.index.nodes[node]`.
  - `select` (`:187-192`): a second arm mirroring `Draining`:
    `Join if matches!(self.state, LaneState::OwedProbe) => match avail.has[PROBE_SLOT] { true =>
    LaneCall::OwedProbe, false => LaneCall::EndOfInput }`.
  - `run`, the `NoBuild` arm (`:318-330`): `mem::replace(&mut self.state, if
    site.owes_probe_when_build_empty { LaneState::OwedProbe } else { LaneState::Draining })`, no
    executor call, `acct.forget(site.slot)` as today, outcome `CallKind::NoBuild` unfinished as
    today. The hold still lifts at `partitioned.rs:331-333`, so no deadlock: the probe subtree
    runs and this lane reads what it produces.
  - `run`, new `OwedProbe` arm: `let batch = self.expect_input(input, site)?;` then
    `Err(failed(site, acct, Some(batch.bytes), BackendError::new("this lane's build side is
    empty and a probe batch arrived, and what this join owes is that batch's rows — a call over a
    build table that does not exist (#175)")))`. `failed` (`:471-485`) releases the batch's bytes
    and names node and lane; the text keeps "build side is empty" and "#175", which
    `tests/test_cpu_end_to_end.rs:839` and `driver/tests/flow.rs:244` grep for.
- Deletions: `executor/mod.rs:204-211`; `cpu_backend/join.rs:40-43` field, `:79`, `:99`, `:163`
  initializers, `:182-192` fn, the `empty_build_answers_nothing` import at `:34`;
  `gpu_backend/join.rs:101-114` and the import at `:23` (`join_type` stays — `finish_without_keys`
  reads it); `cpu_backend/backend.rs:245-247`; `gpu_backend/backend.rs:273-275`;
  `executor/tests.rs:97-99`; `tests/common/injection.rs:308-310`; `driver/mock.rs:85-91`
  `JoinRule.empty_build_owes_its_probe`, `:149`, `:519-525`, and the three fixture lines
  `driver/tests/failure.rs:94`, `flow.rs:404`, `memory.rs:164`.
- `CallKind::NoBuild` doc (`executor/mod.rs:453-455`) — "what the lane owed was nothing" becomes
  "what it owes is the type's answer, and a lane that owes its probe side waits for one".

CPU and device agree by construction: the decision is made once, in the driver, from the plan;
neither backend has an arm. When nothing arrives, no call is made on either side.

### Change 2 — the scatter keeps its typed zero-row batch where the join above owes rows

This is `empty-build.md` §2 / impl Tasks 1–2, restated with what this reading adds.

- `executor/mod.rs` `IndexedNode` gains `feeds_owing_build: bool`; `driver/index.rs::build`
  computes it after `walk` has filled `parent` and `children` (`:20`), by a climb: from the node,
  follow `parent` until a `Join` category node; `true` iff you arrived through
  `children[0]` (the build child — `index/tests.rs:78`: "the join's build side is its first child,
  which is what makes `BUILD_SLOT` zero") **and** that node's `owes_probe_when_build_empty`.
  Anything else, or no join above, is `false`. Only `PartitionEmitter` nodes are read, but
  computing it for every node keeps the field total. Not per batch: a climb in the emitter loop
  would be a tree walk in the hot path to answer a question whose answer cannot change.
- `driver/partitioned.rs:380-391`: `if out.num_rows() == 0 && !self.index.nodes[node].feeds_owing_build
  { continue; }`, comment rewritten: empties are dropped so hash skew does not send them up a
  chain — except where the join above owes rows for an empty build side, which is the one place
  the drop turned a typed table into a refusal. Kept batches take `Held::of` and `acct.hold`
  exactly like the others (`:386-390`); zero rows is not zero bytes (`architecture.md:699-700`)
  and the hold/release pairing is untouched.
- Why the guard is exactly this: a scatter feeding a **probe** side must keep dropping — a kept
  empty is a probe call per empty lane, and the device refuses the second probe call (#152,
  `gpu_backend/join.rs:304-313`); a scatter feeding a build side of a join that owes nothing must
  keep dropping — it would trade one `NoBuild` for a real join call per probe batch in that lane
  (q77's `store` lane 2 would make one per `store_sales` batch). The climb passes through
  non-join nodes because the corpus has the build child three deep: `AggregateBatches` /
  `Project` under q78, q87, q97 (`tpcds.sf1/tp4-single-mini.cpu.txt`, listed by `175-work/owing_joins.py`).
- **`cpp/src/operators/join.cpp:170-185`, the `RightAnti` arm (and `:153-168` `RightSemi` for
  symmetry): guard the empty build before `filtered_join`.** Smallest form: when
  `ltv.num_rows() == 0`, take the `#else` branch's free call — `cudf::left_anti_join(right_keys,
  left_keys, EQUAL)` / `cudf::left_semi_join(...)` — which cuDF guards itself
  (`semi_join.cu:56-64`); or fill `single_indices` with `thrust::sequence` over
  `rtv.num_rows()` for anti and leave it empty for semi. Operator body only; no ABI, no
  wire, no fbs. `Right` needs nothing (`hash_join::_is_empty`). `Full` never reaches the C++ as
  `Full` (the recipe is `Right` per call + `LeftAnti` at done, `wire/join.rs:64-91`) and is #152
  on the device anyway.

With the batch kept, the lane takes `SetBuild` (`avail.has[BUILD_SLOT]`, `single_partition.rs:186`)
and Change 1's `OwedProbe` state is never entered on this route; the two changes meet only in
the residual shape (§7).

### What each deliberately does not touch

`operators/join.cpp` beyond the one guard; `finish_without_keys` (`gpu_backend/join.rs:209-238`,
#173's probe-side site); the six empty-lane arms hacks-audit "propping up" item 3 lists (all
#173's — the coalesce over nothing still emits nothing; Change 2 merely stops it *being* nothing
on this route); `empty_build_answers_nothing` itself; the ABI; the wire; the fbs schema; the
Python exec model (`scripts/exec_model/single_partition_driver.py:170` raises where the Rust driver
has `NoBuild`, so it neither pins nor contradicts either change).

### Pinning tests, goldens, registry, comments that move

Tests that go red and are rewritten:

- `driver/tests/flow.rs:229-249` `a_join_that_owes_its_probe_side_without_a_build_side_is_refused`
  — hacks-audit "tests that would not catch the bug" item 1: it sets the mock knob. Rewrite over a
  real `Right` node (`driver/plans.rs:101-112` gains `join_of(join_type, build, probe)`) with the
  probe producing; still refused, `CallFailed` containing "build side is empty". Sibling, new:
  same plan, probe source with no batches — completes, `rows_returned == 0`, one `NoBuild`, one
  `EndOfInput` on the join, `assert_accounted`.
- `tests/test_cpu_end_to_end.rs:784-843` `a_degenerate_hash_under_a_right_outer_is_refused_by_name`
  (hacks-audit item 6) — flips to a positive: q93 at `MODES[2]` under `Hash::Degenerate` answers
  the DataFusion oracle. Under Change 2 lanes 1–3 take `SetBuild` on a kept empty and their probe
  empties are dropped (probe side) so they `Finish` with nothing; under Change 1 alone they end at
  `EndOfInput`. Either way green.
- `tests/common/injection.rs:576-615` `PlannedMode.owes_probe_when_empty`, the walk arm `:598-601`,
  the filters `:664-667` and `:767-777`, and `tests/test_layout_injection.rs:96`, `:102` — deleted
  (hacks-audit item 5). The hash dimension is back in the cover for every shuffling plan.
- `driver/single_partition/tests.rs:329-380` cross-product — extend `probe_phase` to the four
  post-build states so `OwedProbe` and `Draining` are swept; `:295-314` unchanged.

New tests: `index/tests.rs` — the climb over three hand-built trees (`Right` via build child
through a coalesce → true; probe child → false; `Inner` → false); `driver/tests/flow.rs` —
`EmitRule::ToLane(0)` under `join_of(Right, coalesce_all(emit(..,4)), emit(..,4))` gives four
`SetBuild`, and under `Inner` gives one `SetBuild` and three `NoBuild`; a probe-side scatter under
`Right` still drops (`emitted` lists empty); holds equal releases on each;
`cpu_backend/tests/join.rs` (fixtures at `:20-140`) — `set_build(RecordBatch::new_empty)` then
one probe batch: `Right` pads every row, `RightAnti` returns every row;
`tests/test_gpu_executors/join.rs` — the same two through `GpuJoin` over a zero-row lane obtained
from `GpuEmitter` (pattern at `:315-334`, scatter fewer rows than lanes), which is the device proof
the spec's "device workflow" steps 1–2 ask for and the only place the `filtered_join` guard is
exercised.

Goldens — **measured, not predicted**: `175-work/count_empties.py` over all ten `*-mini.cpu.txt`
files counts 389,331 dropped scatter empties (the spec's figure, so the parse agrees with the
spike) and **0** of them on a lane feeding an owing join's build child — every such scatter in the
enabled corpus (q40, q78 ×4, q87 ×2, q93, q97, `tpch/anti-join`, `175-work/emit_detail.py`) fills
all four lanes on every call. So no existing `.cpu.txt` / `.cost.txt` section moves. What changes
is additive: `tpch.sf1/tp4-{single,rowgroup,sized}-mini.cpu.txt` and `.cost.txt` gain a `== q16`
section each (`live_cpu` gpu oracle, so no `.result.txt`). `.plans.txt`, `recipe-payloads.txt`,
`--- memory ---`: unchanged, nothing is plan-time. `empty-build-impl.md` Task 4 tells the developer
to expect "the low hundreds" and to treat a number far from 244 as worth understanding; the number
is zero and this is why.

Registry and corpus: `testdata/cost-registry.csv:116` `tpch/q16` `cpu_tp4_*` → `enabled`, tickets
`59 62 80 97 152 175 183` → drop `175`; `corpus_cases.inc:95` gains `tp4_single | tp4_rowgroup |
tp4_sized`, comment `:89-93` rewritten. `:78` `tpcds/q77`: `175` → `189` — its top scatter hashes
`__grouping_id:UInt8` (`tpcds.sf1/tp4-single.plans.txt`, `== q77` line 7), the arm the murmur3
hasher lacks (`active-tickets.md:224-237`), exactly as q5/q80; cells stay disabled, comment
`:235-238` rewritten (it already anticipated this). `the_registry_matches_the_cpu_corpus_in_both_directions`
(`tests/test_cpu_corpus.rs:226`) holds both files to each other. `build-test.md:42` "q77 three by
#175" → #189. `test_cpu_end_to_end.rs:357-365`: q77 stays out of the cover, reason → #189.

Wiki: `architecture.md:606-607` ("empty scatter outputs are dropped at the emitter, so nothing
empty traverses a chain") gains the one exception; `:855-857` drops the #175 clause; `tickets.md`
#175 corrected (q16/q77, not q21) and closed when q16's cells are on; #173 text cut to its one
refusing site (`gpu_backend/join.rs:209`) as the spec's §4 says — it blocks zero registry rows
(no tickets cell carries `173`).

Comments that lie, corrected in passing (hacks-audit "propping up" items 2 and 3):
`cpu_backend/accumulate.rs:129-137` rests on "the device's collapse of no handles is a refusal" —
`gpu_backend/accumulate.rs:209-212` returns `Ok(empty)` and the C++ throw at
`node_session.cpp:280-283` is unreachable from it; `gpu_backend/accumulate.rs:186-191` says the
empty batch "is the driver's to supply" — after Change 2 the driver passes the scatter's through in
the one case that matters and supplies none itself; say that. `cpu_backend/tests/accumulate.rs:67-69`
carries the same false premise.

### hacks-audit scaffolding: removed, respected, left

Removed: "propping up" 4 (CPU refusal, `Calls` field, three initializers, device twin), 5
(injection dimension), 8 (mock knob), the flow.rs test in "tests that would not catch the bug", and
6's first test flips. Respected: 1 (the drop line — re-decided, stays the default, one exception),
2 and 3 (comments corrected, arms untouched). Left, correctly: 9–13 (#152, #183, #173's finish).

## 4. Alternatives rejected

- **Spec as written (Change 2 alone).** Closes q16; q77 still refuses in lane 2 (the scatter
  whose lane is empty feeds an Inner join, and the guard drops it — climbing past that join would
  keep the empty and cost a join call per probe batch in that lane, and CPU/device would then have
  to agree on whether an Inner join over an empty build emits a zero-row batch or none).
- **Change 1 alone.** Closes q77's refusal (it lands on #189) and the degenerate-hash test; q16
  keeps refusing, because its empty-build lanes do have probe rows and owe them.
- **Keep `without_build` and store its `Err` as the deferred verdict** — zero trait change, all in
  `single_partition.rs`. Works, but an error kept for later is "a call can fail, and failing ends
  the query" (`architecture.md`, Traits) bent into a verdict, and leaves audit items 4 and 8
  standing. Named as the fallback if the reviewer wants no trait change.
- **The coalesce emits an empty batch of its schema when nothing arrived.** CPU trivial; the
  device cannot make it — #173's wall.
- **Make the empty build table at the join.** Same wall on the device.
- **Remove the drop unconditionally** (hacks-audit 1's "cheapest experiment"). 389,331 batches
  and a probe call per empty probe lane, the second refused (#152). The guard is the task.
- **`empty-answers.md`'s route / pad (`ProjectRole`, `without_build(table)`).** Buys what the C++
  and DataFusion already compute over a zero-row build.

## 5. Minimum corpus query

Shape (a), the q16 mechanism — tpch sf1:

```sql
SELECT count(*) FROM partsupp
WHERE ps_suppkey NOT IN (
  SELECT s_suppkey FROM supplier WHERE s_comment LIKE '%Customer%Complaints%')
```

This is `testdata/tpch-queries/anti-join.sql`'s shape with q16's subquery: DataFusion
decorrelates `NOT IN` into an anti join, `collect_statistics` is off (`lib.rs:25-36` sets only
`target_partitions`), so at `target_partitions = 4` both sides get a hash `RepartitionExec`,
which the translator turns into `GpuEmitPartitions` (`planner/translator/nodes.rs:180-211`) —
`== anti-join` at `tpch.sf1/tp4-single.plans.txt:29-38` shows `RightAnti`, build = the filtered
subquery through `CoalesceAllBatches ← EmitPartitions`. The four filtered suppliers over four
lanes leave at least one lane empty — q16's registry row is the evidence, since the same filtered
set feeds q16's `RightAnti`. Plans today (`plan_status ok`); refused at run time on the CPU at
`tp4-single`, `tp4-rowgroup`, `tp4-sized` with `RunError::CallFailed("GpuHashJoin lane N: this
lane's build side is empty, and what this join owes is its probe side — … (#175)")`; passes at the
two tp1 modes (one lane, four build rows). Device: refused the same way, and after the fix at the
second probe call (#152). Expected answer 799,680 (800,000 − 4 × 80). For a deterministic empty
lane use `WHERE s_suppkey = 1` instead of the `LIKE` (one build row, three empty lanes).

Shape (b), the q77 mechanism — tpcds sf1, needs Change 1:

```sql
SELECT s.s_store_sk, s.sales, r.returns_
FROM (SELECT s_store_sk, sum(ss_net_paid) AS sales
      FROM store_sales JOIN store ON ss_store_sk = s_store_sk GROUP BY s_store_sk) s
LEFT JOIN (SELECT s_store_sk, sum(sr_net_loss) AS returns_
      FROM store_returns JOIN store ON sr_store_sk = s_store_sk GROUP BY s_store_sk) r
ON s.s_store_sk = r.s_store_sk
```

q77's channel block minus the rollup. Check the plan shows `join_type=Right` with the returns
aggregate as build and no scatter between the aggregates and the join (q77's does; the swap is
DataFusion's). Lane 2 has no `store`, both sides are empty there, today refused by name at
tp4 modes on the CPU; after Change 1 it answers, and q77 itself then meets #189.

## 6. Cells re-enabled

- `tpch/q16` × `cpu_tp4_single`, `cpu_tp4_rowgroup`, `cpu_tp4_sized` — on (Change 2).
- `tpcds/q77` × the same three — off, re-attributed `175 → 189` (Change 1 removes #175's
  refusal; the rollup scatter's `UInt8` key refuses next).
- All gpu cells of both rows stay off: `152` (build copy per probe batch, both queries), `183`
  (q16), `187` (q77).
- The injection hash dimension returns for every shuffling plan with a `Right`/`Full`/`RightAnti`
  join (tpcds q93, q97, q87 in the injected set), which is coverage rather than a cell.
- #175 closes when q16's cells are on and q77's row says 189. #173 is unchanged and blocks no cell.

## 7. Risks and unknowns

- **Device, `RightAnti` over a zero-row build** — the `filtered_join` zero-block launch is read,
  not run; whether it throws, or silently inserts nothing and returns every probe row by luck,
  decides only how the missing guard would have shown up. The guard is required either way.
- **Device, `Right`** — `cudf::gather` with `NULLIFY` over a zero-row source table is read as
  every index out of bounds → every value null (`detail/gather.cuh:76-100`, `:569-577`); not run.
  Unverified on string columns (q16's probe has `Utf8View`, but the build there is `s_suppkey:Int64`
  alone, so q16 does not need it).
- **Residual refusal, stated so it is not mistaken for a gap**: a `Right`/`Full`/`RightAnti` lane
  whose build is empty because a join *below* drained (not a scatter) and whose probe side has
  rows. Neither change reaches it; Change 1 refuses it by name at the first probe batch. No corpus
  query has it (the co-partitioned probe side derives from the same empty dimension lane in q77);
  a NULL foreign key on the probe side is how one would appear.
- **DataFusion swap for the shape-(b) query** is assumed from q77's plan, not re-derived.
- **q16's tp4-rowgroup/sized batch counts** — the supplier filter may emit several small batches
  under those modes, so the emitter is called more than once and keeps more than one empty per
  empty lane; harmless (the coalesce concatenates), visible in the new golden sections.
- **Trait deletion touches five impls**; the `injection.rs` wrapper and the `executor/tests.rs`
  macro are the two easy to miss.
- Cost of a kept empty on the device: one `handle` per empty lane per emitter call, one
  `concatenate` at the coalesce. In the enabled corpus that is zero today (measured).

## 8. Complexity

**M.** Rust: `single_partition.rs` (~40 lines: two variants, one state, two arms), `index.rs`
(~30: two fields, one climb, one predicate call), `partitioned.rs` (2), `plan/join.rs` (~10),
deletions across `cpu_backend/join.rs`, `gpu_backend/join.rs`, two `backend.rs`, `mock.rs`,
`executor/mod.rs`, `executor/tests.rs`, `injection.rs` (~60 lines net removed). C++: one guard in
`operators/join.cpp` (~10). Tests: ~8 new/rewritten across five files, injection scaffolding
removed (~60 lines). No frozen surface: no ABI symbol, no fbs field, no wire change, no declared
schema; the internal `JoinExecutor` trait loses a method. Goldens: additive only — three tpch
`.cpu.txt` and three `.cost.txt` gain a `q16` section, nothing existing moves; registry two rows;
corpus two lines and two comments; four wiki sentences. Device rollout on `shad-gpu` proves the
`RightAnti` guard through `test_gpu_executors/join.rs`; no device corpus cell changes.
