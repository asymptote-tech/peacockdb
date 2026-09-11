# #173 — the frozen surface cannot build a table out of nothing

Read at master `188c23ce` (three doc commits past the `c18e063a` the board names; no code moved).
Paths relative to `/media/data/peacockdb`. Nothing built or run. Two measurements are Python
parses of the committed goldens (scripts left in the session scratchpad, not the repo):
`find_empty_probe.py` — every hash join in the ten `*-mini.cpu.txt` files whose build or probe
child has a lane with no batch at all; `finishing_joins.py` — every finishing-type hash join in
the tp4-single plans and what feeds each side.

**Relation to the specs and to the sibling, up front.**

- `SCRATCH/175-proposal.md` exists and this reading agrees with it on every fact we both checked:
  #175 is the `without_build` refusal, `tpcds/q77` is reached through an Inner join that drains
  and not through a scatter, `empty-build.md`'s first-join climb therefore does not reach q77, and
  no existing golden section moves under a build-side-only guard. **This proposal builds on the
  sibling's Change 1 (`OwedProbe`) and Change 2 (the kept build-side empty) and does not
  re-propose them.** It contradicts the sibling in one place, stated in §3 Fix B and §4: the
  sibling rejects climbing past a non-owing join because of the per-probe-batch cost; Fix B
  climbs past it and bounds the cost to one call with a driver rule.
- `llm-wiki/tasks/empty-build.md` (frozen, task 12): its §4 says #173 "blocks zero cells" and
  should be "cut to its one real site". Agreed on the cells (§1). Not agreed that the ticket is
  one site: the site refuses because something upstream turned a stream into *nothing*, and
  there are exactly two nodes in a planned tree that do that. This proposal fixes those two and
  the finish arm, rather than the arm alone.
- `llm-wiki/tasks/refcounted-tables.md` (#152, #145): independent. Fix B's driver rule removes
  the one place where this work would otherwise meet #152 on a device; after refcounting the
  rule is a cost rule only.
- `llm-wiki/archive/archived-tasks.md`, the rejected `empty-answers.md`: wanted `output_schema`
  on the wire plus a C++ `make_empty_column` arm. Nothing here needs either; the rejection stands.
- `llm-wiki/tasks/operator-cases.md` expects `bug_` tests on #173 for "no batch at all" shapes.
  After this proposal one remains (§6).

## 1. Issue

The ticket names four sites. What each does today, by reading:

| site | what it does when the lane has no batch | CPU twin |
|---|---|---|
| `executor/gpu_backend/accumulate.rs:209-212` collapse | `Ok(Vec::new())` — emits nothing | `cpu_backend/accumulate.rs:138-141`, nothing |
| `gpu_backend/accumulate.rs:247-250` sorted runs | nothing | `cpu_backend/accumulate.rs:196-199`, nothing |
| `gpu_backend/accumulate.rs:387-389` merge of lanes | nothing | `cpu_backend/accumulate.rs:270-272`, nothing |
| `gpu_backend/join.rs:209-238` `finish_without_keys` | **refuses** Left, Full, LeftSemi, LeftMark by name; hands the build up for LeftAnti | `cpu_backend/join.rs:253-266` answers all five through DataFusion |
| `cpp/src/node_session.cpp:273-282` collapse of no handles | throws; unreachable, the Rust arms short-circuit first | — |

So the only engine divergence is the finish arm, and the three accumulator arms are a
consistent "nothing in, nothing out" that violates only the `SingleBatch` declaration of the
node. Two things follow from that and both are wrong in the ticket's text and in
`00-tickets.md`:

- **No corpus cell is disabled by #173 itself.** `finish_without_keys` is reached only by a
  finishing join (Left, Full, LeftSemi, LeftAnti, LeftMark) whose probe lane got no batch while
  its build lane got one. `finishing_joins.py` lists every such join at tp4-single: 38 of them,
  every probe side a fact-table scatter (`store_sales`, `lineitem`, `orders`, `catalog_sales`…)
  or a coalesce over one, and `find_empty_probe.py` finds no finishing join with an empty probe
  lane in any enabled golden section. The cells `00-tickets.md` attributes to #173 —
  `tpch/q16` and `tpcds/q77` at the three tp4 modes (`testdata/cost-registry.csv:116`, `:78`) —
  refuse in `without_build`, which is #175, and the sibling proposal re-enables q16 and moves
  q77 to #189 without touching any #173 site.
- **#158 is a different capability.** `PlaceholderRowExec` is a source of one row of literals,
  not an empty table; the "make-empty-of-schema" call the ticket asks for would not answer it.
  Out of scope here; its refusal at `tests/test_planner_join_refusals.rs:123-127` stays.

What #173 does cost, once #175 is fixed the sibling's way:

- (i) the finish arm: a finishing join whose probe lane carries no batch answers on the CPU and
  refuses on a device, for four of five types from information the device already holds;
- (ii) the sibling's §7 residual: an owing join (Right, Full, RightAnti) whose build lane is
  empty because a join *below* drained, and whose probe lane has rows — refused by name at the
  first probe batch under Change 1;
- (iii) a mid-plan `GpuLimit` whose interval names no row of its stream (`OFFSET` past the end)
  emits nothing (`gpu_backend/accumulate.rs:420-424`, `cpu_backend/accumulate.rs:409-411`), and a
  join above it then meets (i) or (ii) at one lane, where no scatter guard can help.

None of the three is in the sf1 corpus. All three plan today and are ordinary SQL.

## 2. Root cause

The surface builds a table only by reading one, so a node handed no handle cannot answer with an
empty table of its schema. That is the ticket's sentence and it is true. What it leaves out is
that the engine *does* build a zero-row table of the right schema at every place a stream can
become nothing, and throws it away:

1. **The scatter.** Both emitters return exactly N typed batches, empty where the hash sent
   nothing (`cpu_backend/emit.rs:57-75`; `cpp/src/node_session.cpp:405-421`, a deep-copied
   `cudf::slice(pv, {start, start})`). `driver/partitioned.rs:379-386` drops the empty ones
   before anything holds them. The sibling's Change 2 keeps them on a lane whose *first* join
   above owes rows for an empty build. A lane whose first join above owes nothing drains
   (`single_partition.rs:187-192`, `LaneState::Draining`) and emits nothing, and so the next
   join up — which may owe — gets nothing on that lane. That is q77's shape and shape (ii).
2. **The mid-plan limit.** A batch outside the interval is released uncalled
   (`gpu_backend/accumulate.rs:420-424`). The executor had the batch in hand; a
   `peacock_executor_slice_handle(h, 0, 0)` would have been a zero-row table of the stream's
   schema for the price of one existing call, and the CPU has `RecordBatch::slice(0, 0)`.
3. **The join's finish.** With no accumulated keys the recipe's first at-done call — the concat
   (`wire/join.rs:94-100`, `Input::AccumulatedKeys`) — has no handle. But the build handle is
   still held (`GpuProbingJoin::build`, `gpu_backend/join.rs:136`: nothing consumed it, since no
   probe call ran) and the recipe's *later* at-done calls take exactly that schema: the pad
   project (`:113-121`) reads build columns and appends typed NULLs, the narrow project
   (`:122-135`) reads build columns. The anti join those calls normally follow would, over no
   keys, return every build row unchanged. So the finish's answer for Left, Full and LeftAnti is
   the build handle fed to the calls after the join; for LeftSemi it is the build sliced to zero
   rows. Only LeftMark needs a table the lane does not have — a zero-row table in the *keys*
   schema, to run the mark join against.

A third origin of an empty lane is a source lane with no row groups (`partition_groups=[[[0]],[],[],[]]`
on every dimension table at tp4). It never reaches any of the sites: a multi-lane unhashed layout
cannot feed a hash join (`plan/join.rs:523-536` demands `hashed_on` above one lane) and the
planner puts a coalesce only under a join side or a scatter (`planner/translator/nodes.rs:203-209`,
`:274-279`, `:455-461`, `:498-500`), so an empty source lane meets only forwarders, the partition
accumulator (which merges the other lanes) and per-lane accumulators whose consumer is one of
those. The one exception is `GpuAggregateBatches` for a global aggregate, which is #199 and a
batch-list divergence rather than a refusal.

So the root cause, one sentence: **two nodes turn a stream into nothing and keep no evidence of
its schema, and one node refuses a question it can answer from the handle it holds.**

## 3. Localized fix

Three pieces, in the order they should land. A and C are independent of everything, including
the sibling. B extends the sibling's Change 2 and needs it.

### Fix A — the finish answers from the build side (device only, `gpu_backend/join.rs`)

Rewrite `finish_without_keys` (`:200-238`):

```rust
fn finish_without_keys(mut self) -> CallResult<Vec<GpuBatch>> {
    let build = self.build.take().ok_or_else(|| BackendError::new(
        "the build side was consumed before the finish, and no probe call ran to consume it"))?;
    let seed = match self.join.join_type {
        // Nothing matched, so the anti join the recipe names would hand every build row on.
        Some(JoinType::LeftAnti | JoinType::Left | JoinType::Full) => build,
        // No rows: the build side cut to zero rows is a table of its schema.
        Some(JoinType::LeftSemi) => self.slice_to_nothing(build)?,
        Some(JoinType::LeftMark) => return Err(BackendError::new(
            "this lane's probe was empty, so its finish has no keys to run the mark join \
             against — every build row with a false mark takes a zero-row table of the KEY \
             schema, which this lane never held; the driver hands a finishing join's probe \
             lane one zero-row batch, so a planned tree does not reach this")),
        other => return Err(BackendError::new(format!("{other:?} publishes no finish"))),
    };
    // The calls after the finish join — pad or narrow — read the build schema, which is
    // what `seed` is; the concat and the join are skipped, having nothing to read.
    let mut prior = Some(seed);
    let mut none = None;
    for call in self.join.at_done.clone() {
        if matches!(call.kind, FbKind::Project(_)) {
            prior = Some(self.make(call, &mut none, &mut prior)?);
        }
    }
    Ok((prior.into_iter().collect(), CallStats::default()))
}
```

`slice_to_nothing` is `peacock_executor_slice_handle(executor, handle, 0, 0, &mut out)` wrapped
exactly as `LimitStream::accumulate_and_fetch` does at `:430-456`, priced
`logical_size_from_schema(build_schema, 0, 0)`; the input handle is consumed by the call, which is
what `slice_handle` does (`node_session.cpp:525-541`). The build schema is not stored on
`GpuJoin` today (`keys_schema` and `output` are, `:46-47`); add `build: SchemaRef` from
`executors_for`'s inputs beside them.

Why this is right per type, against the CPU: LeftAnti — every build row (today's arm, now also
narrowed where the node projects; today it hands every build column up whatever `projection`
says, which is hacks-audit "propping up" 11's function answering wrong on a projecting anti
join). Left, Full — every build row padded, which is `pad_project`'s output over the build
(`wire/join.rs:256-286`, ordinals `< build_width` read from the build, the rest typed NULLs).
LeftSemi — no rows in the declared schema, narrowed where the node projects. Row order: the
anti join's gather map is not ordered and results are compared row-sorted
(`architecture.md`, "Determinism rules"), so the build's own order is an answer.

Doc comment rewritten: the finish computes from the handle it holds; the one type it cannot is
named with the reason (a keys-typed table); `#173` leaves the message.

### Fix B — nothing is handed up a chain that ends at a join owing rows (driver)

Extends the sibling's Change 2 in three ways, each one rule.

**B1 — the climb passes through a join on the side that passes emptiness.** The sibling's
`feeds_owing_build` (`driver/index.rs`, computed after `walk`) stops at the first `Join` node.
Replace its body:

```
climb from the emitter; child = emitter
  Exec | BatchAccumulator            → continue (1:1 nodes and per-lane accumulators pass nothing through as nothing)
  Join, entered through children[0]  → hash join: !empty_build_answers_nothing(t) ⇒ true
                                       cross / nested-loop: false-and-continue (owe nothing, emit nothing)
                                       otherwise continue — the lane drains and emits nothing
  Join, entered through children[1]  → hash join with a finish (capability().needs_finish) ⇒ true
                                       nested-loop Left ⇒ true (owes every build row padded; today emits nothing)
                                       otherwise continue — no probe batch, no at-done call, nothing emitted
  anything else                      → false  (forwarders, partition accumulator, unload: the other lanes carry the answer)
```

The field is renamed to what it now says — `feeds_a_lane_that_owes` — and is read at the one
site the sibling reads it, `partitioned.rs:383`.

**B2 — one spare per lane, delivered at the emitter's done, not every empty.** On a guarded
emitter the first zero-row output for lane p is kept aside in `NodeState` as
`spare: Vec<Option<Held<B::Batch>>>` (held: `Held::of`, `acct.hold`); later empties for that
lane are dropped as today; a non-empty output for lane p releases the spare (`acct.release`,
drop). At the emitter's done (`run_emitter`, `:342-352`, the arm that sets `emitter_finished`)
every spare still held is `record_emitted` and pushed to its lane's queue before `out_done` is
set. `release_in_flight` (`:640-649`) releases spares as it releases queued batches, so early
exit and failure balance.

Why one and not all: on a **probe** lane every kept empty is a probe call, and the device
refuses the second (`gpu_backend/join.rs:304-313`, #152). One spare is exactly one probe
batch, which is what makes the probe side guardable at all — the sibling and `empty-build.md`
drop probe-side empties for that reason and so cannot reach LeftMark's finish or nested-loop
Left. On a build lane one spare is one handle to concatenate instead of k, where the sibling's
form keeps k (its §7 notes "keeps more than one empty per empty lane; harmless").

**B3 — a join whose build side is zero rows and whose type owes nothing for it probes once.**
In `single_partition.rs`: at `SetBuild` (`:294-316`) the driver has `n_rows`; record
`build_was_empty = n_rows == 0` on the lane. In `select` (`:170-202`), before the `Probe` arm:
`Join if build_was_empty && !site.owes_probe_when_build_empty && probed_once && avail.has[PROBE_SLOT]
=> LaneCall::DropProbe`, and when the probe side is done, `Finish` as today. `probed_once` is set
by the first `Probe`. The finish still runs, because the build-side semi family emits its answer
there (`finish_and_fetch` over the keys the one probe left), so this is not `Draining`, which
ends at `EndOfInput`. `site.owes_probe_when_build_empty` is the sibling's field on `LaneSite`.

Why: every type that owes nothing for an empty build answers every probe batch with zero rows
(`plan/join.rs:446-461`'s reasoning: every output row is built from a build row). One call
produces the zero-row batch of the node's output schema the lane above needs — the batch the
surface cannot make from nothing — and every further call would produce another one. This is
what bounds B1's cost to one call per guarded non-owing join lane, and what keeps a device off
#152 there. It is a driver rule, so both backends obey it by construction and neither grows an
arm. It also applies to a build that is legitimately zero rows (a filter that kept nothing,
coalesced): no enabled golden has one (`find_empty_probe.py` variant: no join with a `[0]` build
batch anywhere), so nothing moves.

**What B does not touch.** The drop stays the default everywhere the climb says false; the
sibling's `OwedProbe` stays as the refusal for a hand-built tree that hands an owing join no
build and then a probe batch; `empty_build_answers_nothing`; the ABI; the wire; the plan.

### Fix C — the mid-plan limit keeps one zero-row spare (both backends)

`LimitStream` on each backend gains `spare: Option<Batch>` and `emitted: bool`. In
`accumulate_and_fetch`, the arm that releases a batch outside the interval
(`gpu_backend/accumulate.rs:420-424`, `cpu_backend/accumulate.rs:409-411`): if nothing has been
emitted and no spare is held, keep the batch sliced to zero rows — device
`peacock_executor_slice_handle(h, 0, 0)` (the code at `:430-456` with `rows = 0..0`), CPU
`batch.slice(0, 0)`; otherwise release as today. Any emitted batch sets `emitted` and drops the
spare. `mark_done_and_fetch` (`:460-462`, `:420-422`) emits the spare iff nothing was emitted.
`HeldBytes for GpuAccumulator` (`gpu_backend/backend.rs:334`) and the CPU twin report the spare
(zero fixed-width bytes, 4 per string column, `common.rs:23-41`). The spare is priced at zero
var-length bytes, which for zero rows is exact — hacks-audit production bug 1 is about the
non-empty slice on the same line and is not touched.

Unconditional rather than guarded: a limit is one lane, its consumers are one lane, and the only
cost of a spare nobody needed is one zero-row batch through the nodes above it.

### CPU and device stay one engine

- A: the device answers what `cpu_backend/join.rs:253-266` answers; the CPU twins of each new
  device test assert the same rows. LeftSemi's batch count (`[]` or `[0]`) on the CPU is decided
  by DataFusion's stream and is a harness finding (§7); the device arm produces `[0]`.
- B: the guard, the spare and the probe-once rule are all in the driver, which is generic over
  `Backend`; no backend has an arm to disagree with.
- C: the same rule written twice, once per backend, as `LimitStream` already is; the CPU
  accumulate tests and the device accumulate tests get the same case.
- The Python model (`scripts/exec_model/partitioned_driver.py:296-301`) carries the drop;
  B1/B2 belong there too or the model stops modelling the rule (build-test.md says it runs in
  CI). The sibling notes the model raises where the Rust driver has `NoBuild`.

### Pinning tests, goldens, registry, comments

Tests that change:

- `tests/test_gpu_executors/join.rs:335-378` `a_finish_over_no_probe_keys_hands_the_build_side_up`
  stays (LeftAnti, no projection). New beside it, same fixture: Left pads every build row;
  LeftSemi answers zero rows; LeftAnti with a projection answers the declared columns; LeftMark
  is refused naming the key schema. CPU twins next to
  `cpu_backend/tests/join.rs:600-618`.
- `tests/test_gpu_executors/accumulate.rs:296-329` `a_collapse_with_no_input_handles_is_refused_by_the_device`
  stays as a guard test; its doc drops "(#173)" and says what it pins: the C++ refuses a call no
  driver makes.
- `cpu_backend/tests/accumulate.rs:66-72` `a_coalesce_that_received_nothing_emits_nothing`: the
  doc's premise ("the collapse of no handles is a refusal there") is false today; rewrite to the
  rule — an accumulator handed nothing emits nothing, and the driver hands a batch to the lanes
  whose consumer owes rows for it.
- New CPU and device accumulate tests: a limit whose interval names no row of its stream emits
  one zero-row batch; one whose interval is met emits no spare.
- `driver/tests/flow.rs`: `a_skewed_shuffle_drops_the_empty_lanes_at_the_emit` (`:262-277`)
  unchanged — a merge above, so the climb says false. New: a scatter under a coalesce under an
  Inner join under a Right join's build (`plans.rs:101-112` gains `join_of(JoinType, …)`, as the
  sibling proposes) delivers one spare per empty lane at the emitter's done, the Inner takes
  `SetBuild` with zero rows, probes once, drops the rest, finishes, and the Right join takes
  `SetBuild`; the same plan with the probe side producing rows on that lane answers instead of
  refusing; a lane that receives rows after a spare was kept releases it; holds equal releases
  on each, including a run that exits early with spares held.
- `driver/index/tests.rs`: the climb on four hand-built trees (through an Inner build side to a
  Right build side → true; through an Inner build side to a merge → false; a Left join's probe
  side → true; an Inner's probe side to an unload → false).
- `driver/single_partition/tests.rs:329-380` cross-product: `build_was_empty × probed_once`
  added to the swept states.

Goldens: `find_empty_probe.py` and the sibling's `count_empties.py` agree that no enabled section
has a guarded lane under either climb, so **no existing `.cpu.txt` or `.cost.txt` section
moves**. `tpcds/q77` gains sections at the three tp4 modes if #189 is not in its way; the
sibling reads its top scatter as hashing `__grouping_id:UInt8` and I have not re-read that, so
expect q77 to land on #189 as the sibling says. `.plans.txt`, `recipe-payloads.txt`, the
`--- memory ---` sections, `.result.txt`: untouched, nothing is plan-time and no recipe changes.
The developer should still read the batch-count delta as `empty-build-impl.md` Task 4 says.

Registry and corpus: no row carries `173` today, so nothing moves on this ticket's account. The
q16/q77 rows move on #175's (sibling §3).

Wiki: `architecture.md:477-479` ("A concat of nothing throws (#173), so the finish has to
answer from the build side alone — LeftAnti over an empty key table is every build row") — the
finish answers four types from the build and the driver hands a finishing join's probe lane a
batch; `:606-607` ("empty scatter outputs are dropped at the emitter, so nothing empty
traverses a chain") gains the one exception and its form (one spare, at done); the node table's
`GpuLimit` row and "The limit lowering rule" ("streams and holds nothing") — it holds one
zero-row spare; `:854-858` drops the #173 clause. `tickets.md` #173: closed when A, B, C land,
with LeftMark-without-a-probe-batch recorded as reachable only from a hand-built tree.
`build-test.md:42-44` corpus counts move on #175's account only.

hacks-audit scaffolding: "propping up" 1 — the drop line is re-decided (default drop, one spare
on guarded lanes, delivered at done), which is what that finding asked for at close; 2 — the
sentence "the empty batch is the driver's to supply" becomes true in the narrow form and is
rewritten to say which lanes; 3 — the six arms stay and their docs stop citing a refusal that is
not there; 11 — `finish_without_keys`'s `self.build.take()` becomes `ok_or_else`, inside Fix A;
"tests that would not catch the bug" first item and items 4, 5, 6, 8 are the sibling's. Left
standing, correctly: 9, 10, 13 (#152).

## 4. Alternatives rejected

- **`output_schema` on the wire plus a C++ empty-table arm** (the archived `empty-answers.md`).
  Moves every recipe payload's bytes; the C++ then answers with a table nothing above can pair
  with a build side anyway. Rejected once already; nothing here needs it.
- **A `make_empty` ABI symbol.** Unnecessary: every place a stream becomes nothing already holds
  a table of the right schema at that moment.
- **Fix A only, leave B and C.** Closes the device divergence at the finish for four types; (ii)
  and (iii) keep refusing by name and LeftMark stays unanswerable. Named as the fallback if the
  human wants #173 narrowed rather than closed — then it is S and touches one file.
- **Keep every empty on guarded lanes** (the sibling's form of Change 2, extended to the probe
  side). A probe lane then makes k probe calls and the device refuses the second (#152); on the
  build side k handles are concatenated where one would do.
- **Remove the drop unconditionally, spare form.** At most 244 spares corpus-wide (the spike's
  count of permanently-empty lanes), so the call count is fine; but every one of ~90 Inner joins
  with an empty dimension lane at tp4 then takes `SetBuild` and one probe call where it drains
  for free today, and every tp4 golden section with such a lane moves. The climb costs 40 lines
  and keeps the goldens still.
- **Probe-once in the executors instead of the driver.** The same rule twice, one per backend,
  where the driver can state it once; the driver already knows the build's row count and the
  join's verdict.
- **A probe-side pad `ProjectRole` for Right/Full and a narrow for RightAnti** (the archived
  spec's §2), so an owing join with no build could answer from its probe batch alone. Correct,
  and it would make B1's climb unnecessary for (ii); but it adds a seq to every Right/Full
  recipe, so every `.plans.txt` recipe line and the payload golden move, for a shape the corpus
  does not contain.
- **Answer LeftMark from the build by slicing the build to zero rows as the finish's right
  input.** The mark join's right-side key ordinals `0..k` would index build columns of the wrong
  types; cuDF checks key types and throws. Not a table of the key schema.

## 5. Minimum corpus query

Shape (i), the finish arm, tpch sf1, `tp4-single` (also `tp4-rowgroup`, `tp4-sized`), device:

```sql
SELECT count(*), sum(l.l_quantity), sum(o.o_totalprice)
FROM orders o
LEFT JOIN (SELECT CASE WHEN l_quantity > 100 THEN l_orderkey END AS k, l_quantity
           FROM lineitem) l
  ON o.o_orderkey = l.k;
```

`testdata/tpch-queries/left-join.sql` with the probe key made NULL for every row without letting
the optimizer fold it (`l_quantity` is at most 50, but the planner cannot know). The plan is
`left-join`'s (`tpch.sf1/tp4-single.plans.txt:196-238`): `GpuHashJoin{Left}`, build = orders
coalesced over a scatter on `o_orderkey`, probe = lineitem projected and scattered on `k`. The
probe side is not smaller than the build by any statistic DataFusion has, so the join is not
swapped to Right, and it is partitioned because orders exceeds the collect threshold. Every
probe key is NULL, and comet's murmur3 leaves a NULL out of the hash, so every probe row lands
in the one lane `pmod(seed, 4)` (`architecture.md`, "Grouping sets"): three lanes have build rows
and no probe batch, and `finish_without_keys` runs on each. Today: plans; on the CPU answers
1,500,000, NULL, the sum of `o_totalprice`; on a device refused by name —
`… what it owes is every build row padded with a typed NULL per probe column … (#173)` — at
`GpuHashJoin` lane p, and the same query at `tp1-single` answers on both (one lane, one batch).
After Fix A the device answers; after B the three probe lanes each carry one zero-row batch and
`finish_without_keys` is not reached at all. Not verified by running; the swap decision is read
from DataFusion's rule and the NULL-hash placement from the wiki.

The LeftMark form of (i) — the one type Fix A leaves refusing and Fix B answers — tpch sf1, tp4:

```sql
SELECT count(*) FROM orders o
WHERE o.o_orderkey IN (SELECT l_orderkey FROM lineitem WHERE l_orderkey = 1)
   OR o.o_custkey = 1;
```

`IN … OR` plans a `LeftMark` (no `RightMark` exists, so no swap); the subquery has one key and so
one lane. Expected answer: the orders of customer 1 plus order 1.

Shape (iii), the limit route, tpch sf1, every mode, both backends:

```sql
SELECT count(*) FROM region r
LEFT JOIN (SELECT n_regionkey FROM nation LIMIT 5 OFFSET 100) n ON r.r_regionkey = n.n_regionkey;
```

The limited subquery is a `GpuLimit` over the nation scan at one lane (the `nested-limits`
lowering, `tpch.sf1/tp4-single.plans.txt:270-294`); with 25 rows and an offset of 100 it emits
nothing, and the Left join's probe lane has no batch: the device refuses at the finish, the CPU
answers 5. After Fix C the limit emits one zero-row batch and both answer.

## 6. Cells re-enabled

- On #173's own account: none — no registry row carries it, and the device cells that would
  first reach a #173 site are behind #152 (`gpu_tp4_*` everywhere) and #183/#187 (`tp1-single`).
- On #175's account, through the sibling's changes which B extends: `tpch/q16` × three cpu tp4
  modes on; `tpcds/q77` × three moves to #189 (sibling §6). Fix B changes how q77's lane 2 is
  answered (one probe call on the drained Inner instead of `OwedProbe` ending the lane) and not
  whether.
- The injection hash dimension (`tests/common/injection.rs:664-667`, `:767-777`) and
  `a_degenerate_hash_under_a_right_outer_is_refused_by_name` flip on #175's account; with B the
  degenerate hash under a **finishing** join's probe side is also answered rather than starving
  the finish, which widens the same cover.
- Stays off behind another ticket: every gpu cell (#152, #183, #187); q77's tp4 cells (#189);
  `count(*) FROM nation` (#158, a source of literals, not this wall); the global aggregate over
  an empty lane (#199).
- Stays refused, narrowed: a LeftMark finish with no probe batch reached from a hand-built tree —
  the harness's `bug_` test for #173 (`operator-cases.md`, "only zero-row probe batches then the
  finish" row) shrinks to that one row.

## 7. Risks and unknowns

- **The pad project over the build handle** (Fix A, Left/Full) assumes the anti join emits every
  build column in build order with no projection, which `finish_join` writes (`wire/join.rs:194-237`,
  no `projection`) and `pad_project` indexes (`:268-284`). Read, not run.
- **`slice_handle` to zero rows on a device** (A for LeftSemi, C): `cudf::slice(view, {0, 0})`
  then `cudf::table(view)` over an empty slice, and later `to_arrow_host` on a zero-row string
  column — `empty-build.md`'s device checklist items 3–4, unproven here.
- **DataFusion's batch count for a LeftSemi finish over one zero-row keys batch** — whether
  `HashJoinStream` yields a zero-row batch or nothing decides whether the CPU's answer is `[0]`
  or `[]`; the device arm produces `[0]`. The operator harness's LeftSemi row is where it shows;
  if the CPU yields nothing, the device arm returns `Ok(Vec::new())` instead and the narrow
  project is skipped.
- **B3's interaction with `OwedProbe`**: a lane with `build_was_empty` from a real zero-row
  batch and an owing type never drains (the guard is `!owes_probe_when_build_empty`), so
  `OwedProbe` and `DropProbe` cannot both apply; asserted in the cross-product test, not proved
  here.
- **Accounting under early exit with spares held** (B2): `release_in_flight` must see them or
  `hops()` reports holds ≠ releases; the failure path drops them with the `Driver` and the
  session reset makes the release a no-op, which is the existing rule for queued batches.
- **The minimum query's plan shape** rests on DataFusion not swapping the Left join and on the
  CASE key not being simplified; both read from rules, neither run.
- **q77 after B**: if `pmod(seed, 4)` for NULL `sr_store_sk` is lane 2, the drained Inner there
  has probe batches and B3's one call runs on a device before #152 would have — fine, and behind
  #152 and #189 anyway.
- **Cost of B1 on a device** for a chain the corpus does not have: one `inner_join` over a
  zero-row build per guarded non-owing lane — the `filtered_join` construction the sibling flags
  for RightAnti is the same class of risk for LeftSemi/LeftAnti/LeftMark builds of zero rows, and
  the sibling's C++ guard covers only the RightAnti/RightSemi arms.

## 8. Complexity

**M** for A + B + C; **S** for A alone or A + C.

- Files: `executor/gpu_backend/join.rs` (A, ~40 lines net), `executor/driver/index.rs` (B1, ~40),
  `executor/driver/partitioned.rs` (B2, ~35: a `NodeState` field, the keep/replace/deliver
  arms, `release_in_flight`), `executor/driver/single_partition.rs` (B3, ~15: two lane flags,
  one `select` arm), `executor/gpu_backend/accumulate.rs` and `executor/cpu_backend/accumulate.rs`
  (C, ~20 each), `gpu_backend/backend.rs` and `cpu_backend/backend.rs` (C, one line each),
  `scripts/exec_model/partitioned_driver.py` (B, the twin rule), doc comments in six places,
  `cpp/src/node_session.cpp:277-282` comment only. Roughly 200 production lines, half of them in
  the driver, plus ~250 of tests across seven files.
- No frozen surface: no ABI symbol, no fbs field, no wire bytes, no recipe, no plan node, no
  declared-schema change. The internal `JoinExecutor` trait is the sibling's to change, not
  this proposal's.
- Goldens: none regenerated on this ticket's account; the `.cpu.txt`/`.cost.txt` sections q16
  and q77 gain are #175's. The developer still reads the batch-count delta.
- Sequencing: A and C can land before the sibling's work or after; B after it, on the same
  branch or the next, since it edits the field and the drop line the sibling introduces.
