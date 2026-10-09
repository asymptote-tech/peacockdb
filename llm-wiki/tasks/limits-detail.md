# limits — working record

## Standing facts

- Third of chain K, branch `ENS-limits` off `ENS-distinct-companions`, PR will target that
  branch. Closes [#186](../archive/archived-tickets.md#t186) and
  [#234](../archive/archived-tickets.md#t234).
- **Chain K runs without a GPU.** No device build, no device run, no GPU cycle. Every build and
  run is local — verda does not resolve from this host (checked again at this dispatch). The
  `scan.cpp` edit and the four `gpu_tests/source_cases.rs` cases are built, not run; #281 holds
  them. The task reaches `done` when every CI job but the GPU tests is green.
- **The cost gate's rises are pre-accepted by the human** (2026-10-08, on the board): this task
  is `done` when the cost-report job's only regressions are the sections this file lists from a
  local `--cost-diff` run — expected `nested-limits` at tp1-rowgroup and tp4-rowgroup, +228
  bytes. Any other regression is an ordinary finding. So the local `--cost-diff` output has to
  land in this file before CI is read, or there is nothing to compare the job against.
- The chain's base is master `31c56bea`. Both tasks below this one were rebased onto it and
  re-proved today; `guard-checks-detail.md` records what master brought across. Nothing in this
  task's scope overlaps master's `64ced62e`, which touched `plan_text` and the join-projection
  rendering.

## Dispatch log

### 2026-10-09 — round 1, developer dispatched

Branch cut from `ENS-distinct-companions` at `aa0335bf`, board moved
`approved to build` → `building` in the same commit.

#### Round 1 evidence

**Per-target counts, measured at the branch head.** rust-only `--lib` 631 passed, 2 ignored
(#182), 633 cases; `test_cpu_corpus` 569; `test_corpus_goldens` 26; `test_cost_model` 3;
`test_module_layout` 17; `test_golden_format` 26; `test_ci_coverage` 9; `cargo test -p
cost-report` 36. C++ `scripts/build.sh --build` against cuDF 25.02 rc=0 with no warning, and
`ctest -L cpu` 1/1. `build-test.md`'s rows were re-summed against these, not deltaed: cpu block
1231, Rust 1902, grand total 2377, and each header equals its own row sum.

**The row-group prefix, mode by mode.** scan-limit's loader maps `[[[0]]]` at all five modes
where it mapped all 49 (tp1-single, tp4-single, tp4-sized) or 49 one at a time (the rowgroup
modes); nested-limits' part loader maps `[[[0]]]` where it mapped `[[[0,1]]]` or `[[[0],[1]]]`.
`a_limited_scan_maps_only_the_row_groups_that_reach_its_limit_at_every_mode` pins the boundary
on the minimal part (122,880 + 77,120): `LIMIT 122880` maps `[0]`, `LIMIT 122881` maps `[0, 1]`,
no limit maps `[0, 1]`.

**Golden sections that moved**, by the section differ, in all fifteen tpch files: exactly
`nested-limits` and `scan-limit`, and nothing else. `mini.result.txt`, `recipe-payloads.txt` and
every `tpcds.sf1` golden are untouched. Deletion audit by `git diff --numstat` rather than `grep
-c '^-[^-]'`, which silently misses a deleted line beginning with `-` (it scored
`nested-limits.sql`'s rewritten SQL comment as zero deletions): every deletion is accounted for
— in `source_cases.rs` the four `bug_` cases with `gpu_refuses_with`, `cpu_answered`,
`four_sixteens` and `GROUPS_AND_LIMIT`; in `end_to_end/limits.rs` `most_offered` and
`limit_node`; in `rebuild.rs` only rewritten `source(...)` calls and the split fixture, with 57
`Box::new(Gpu…)` fixtures before and after.

**The device-only code: what was proved.** No device ran. `cpp/src/operators/scan.cpp` compiles
against cuDF 25.02 (the local 25.02 build, which is what CI's `cpp-build-2502` compiles). The
gpu rung was built with `scripts/cargo-cudf.sh test --features gpu --no-run` and
`--test test_gpu_corpus --no-run`, both rc=0, and the staged lib binary's `--list gpu_tests::`
reports 536 cases — unchanged, the four `bug_` cases out and four agreement cases in — and names
all four new ones under `tests::gpu_tests::source_cases`. So they compile and are collected, not
merely compiled. `the_registry_matches_the_gpu_corpus_in_both_directions` needs no device and was
run from the built `test_gpu_corpus` binary: it passes, so the registry's gpu column still agrees
with `corpus_cases.inc` after the `186` → `281` edit. **Not proved:** that either backend answers
correctly for the four cases, and that `scan.cpp` without `set_num_rows` reads right on a device.
#281 holds both.

**The gpu rung's build found a caller the rust-only build cannot see**:
`executor/gpu_backend/gpu_tests/mod.rs:141` passed the deleted `limit` argument. The rust-only
`--lib` was green with it still there, so the `--no-run` gpu build is load-bearing for a
signature change, not a formality.

**`Empties::fires` now fires every source's first call** (`tests/injection.rs`). The injected
stamp is `mix(mix(seed, post_order), lane)`, so inserting the two `GpuLimit`s moved
nested-limits' part source from post-order 3 to 4; both sources make exactly two calls, four
coin flips at 50%, and under the new stamps none landed. `run_and_check`'s `empty_batches() > 0`
then failed at `tp1-single rebatch=sources/empties=50%` — a guard going red for a reason with
nothing to do with the engine. Proved by instrumenting the wrapper (`fires=false` on every call
of both sources, stamps and post-orders printed) rather than inferred. The guard is still
reachable: forcing the wrapper to `Empties::Never` turns it red, and restoring it green.

**The impl plan's Task 4 has two steps in the wrong order.** Its Step 3 writes scan-limit's two
tp1 sections and its Step 4 enables them in `cost-registry.csv`, but the skipped placeholder is
driven by the registry (`test_support/corpus_golden.rs`, the `enabled`/`skip` filter), not by
`corpus_cases.inc`. Under `PCK_UPDATE_SECTIONS=1` with the registry still `disabled` the writer
republishes `skipped: not enabled at this mode` and the cell passes against it — green, with the
section never written. The registry edit has to come first.

## Cost gate

`cargo run -q -p cost-report -- --cost-diff --base aa0335bf` (the merge-base with
`ENS-distinct-companions`) at the branch head: 563 sections compared, 7 changed, 2 regressions,
5 improvements, `rc=1`. CI's cost-report job fails this PR on the regressions until the human
accepts them.

| section | base | branch | why |
|---|--:|--:|---|
| tpch/nested-limits, tp1-rowgroup | 999,872 | 1,000,100 | the `GpuLimit 0..+28` over the part scan emits 28 Int64 rows (228 bytes); part was already read one row group at a time here, so nothing offsets them |
| tpch/nested-limits, tp4-rowgroup | 999,872 | 1,000,100 | the same |

These are the two the board pre-accepted, at the +228 bytes it predicted, and no other section
regressed.

Improvements: nested-limits at tp1-single, tp4-single and tp4-sized, 1,626,472 → 1,000,100 (the
part scan maps one row group, not two). scan-limit at tp4-single and tp4-sized, 1,048,882,882 →
21,476,312 (lineitem maps one row group, not 49). scan-limit's two new tp1 sections are new
against a `skipped` base and the gate omits them; both read `peacockdb_cost=21476312`, as the
tp4 modes do.

### 2026-10-09 — round 1 green, PR #172 open

Committed as `7fdaeb22`, pushed, PR #172 against `ENS-distinct-companions` — base verified,
two commits, so the diff under review is this task and not the chain.

Three things from round 1 worth carrying forward rather than re-deriving:

- **The deletion audit this chain has been running is unsound.** `git diff <file> | grep -c
  '^-[^-]'` scores a deleted SQL or markdown comment as zero, because the line itself starts
  with `--`. It read `testdata/tpch-queries/nested-limits.sql` as 0 deletions where it has 1.
  Use `git diff --numstat`.
- **The impl plan's Task 4 has steps 3 and 4 the wrong way round.** The `skipped: not enabled
  at this mode` placeholder is written from `testdata/cost-registry.csv`, not from
  `corpus_cases.inc`, so a `PCK_UPDATE_SECTIONS=1` run with the registry still `disabled`
  republishes the placeholder and the cell passes against it — green with the section never
  written. The registry edit has to come first. Fixed in `limits-impl.md`.
- **The cost-report job on PR #172 will be red**, and that is the pre-accepted case: exactly
  the two `nested-limits` rowgroup sections at exactly +228 bytes, with nothing else
  regressed. `## Cost gate` above has the rows.

Outside this task's scope and deliberately not fixed: an unused `AsArray` import at
`peacockdb-core/src/tests/gpu_tests/aggregate_dimension_cases.rs:9`, a warning on the gpu rung
alone and so invisible to every CPU build and to CI. Pre-existing from `0e7804ef`, cosmetic,
nothing behaves wrongly — no ticket under the house rule. Worth fixing by whoever is next in
that file.

### 2026-10-09 — review round 1: 0 blocking, 2 important, 6 nits

The reviewer could not construct an input on which the branch answers wrongly, and most of the
report is that negative result. Four things it checked and found right, recorded so nobody
re-checks them:

- **`RowInterval::over` is correct, and no golden can test it.** DataFusion computes a scan's
  limit as `global_fetch + global_skip` from the same requirements that produced the limit node
  it kept, so `inner.fetch ≥ outer.skip` always and `over` is the identity on the root interval
  for every shape DF45 can produce. The unit case `an_interval_over_another_keeps_the_rows_both_keep`
  is the only cover there is, which is what the branch wrote.
- **`unload_input`'s descent set is exactly `node()`'s erasing set.** Read arm by arm: a
  fetch-less `CoalesceBatchesExec`, a round-robin `RepartitionExec` and a one-lane
  `CoalescePartitionsExec`, none of which can change a row count in DF45. Two of the three are
  unreachable with a pushed cut, so defensive, but harmlessly — they are `node()`'s behaviour
  either way. Anything else becomes a node and the limit lands legally beneath it.
- **The prefix trim holds under pruning** because the engine never applies a predicate inside
  the scan: `parquet.predicate()` feeds row-group pruning only, `pushdown_filters` is never
  enabled, and a surviving group's rows are all returned. Bigger-than-the-table keeps every
  group; `n == 0` keeps one so `partition` has something to address, and never reads it. No
  interaction with the small-table rule: `lanes_for`'s limit arm returns 1 before the byte
  threshold is consulted.
- **#234 cannot change an answer.** `emitted = clamp(seen − skip, 0, fetch)`, so
  `emitted ≥ fetch ⟺ seen ≥ skip + fetch`, and the two rules diverge only at `fetch == 0` with
  `skip > 0` — the case the spec names and a test pins. The unload's path is untouched:
  `rows_emitted` cannot reach it, because an unload's outputs are `LaneOutputs::Host`.
- The wire deprecation leaves vtable slot 12 empty, `VT_ROW_GROUPS` at 14 and `VT_BATCHES` at
  16, read off the branch's own regenerated `gpu_plan_generated.h`. No C++ caller passes an
  argument at or past `limit`'s old position.

#### The two important findings

**I1, mine — #186 and #234 were still open tickets.** The spec's Scope row says they are
archived here and carries no "on merge" qualifier, and the chain's precedent is in-branch:
distinct-companions archived #62. Done in the commit that carries this section: both bodies moved
to `archive/archived-tickets.md` with a `Closed by limits (chain K)` paragraph that says what
landed instead of what each ticket proposed — neither was fixed the way it argued for, which is
the part a later reader needs. Dropped from `corpus-coverage.md`'s index, `tickets.md` to 116
open and corpus-coverage to 33. The board's and the working docs' links repointed at the
archive; the spec's are left, since the spec is frozen.

Three resolved findings also left `reports/hacks-audit.md`, under the pruning rule its own
preamble states — §2 (`Some(0)` and `None` are one scan limit on the wire: the field is gone from
both writer and reader), §8 (two live counters of one mid-plan limit), and the tests section's
`GpuLoadParquet.limit` entry. Numbers are not renumbered, since tickets cite them; the section's
own "only two things here behave wrongly" is now one.

**I2, the developer's — the hold's effect is no longer covered outside the mock tier.** Only its
detection is. `most_offered > 2` went and `assert_eq!(offered, pulled)` took its place, which is
a different and weaker claim, and the test is called
`a_limit_slices_at_most_two_batches_and_stops_the_scan`. The reviewer proved it rather than
asserting it: across all ten `*-mini.cpu.txt` goldens only `nested-limits` and `scan-limit` carry
an `early_exit` at all, and in both, at every mode, every mapped batch is pulled — so making the
subtree hold a no-op reddens nothing outside `driver/tests/limit.rs`. Before the branch, part
mapped two row groups at the rowgroup modes and one was pulled, which is what the old assertion
pinned.

#### N7, mine — a line for #281

`nested-limits`' region loader carries `projections=[]` and an empty declared schema while
`scan.cpp` reads an empty projection as "read every column", so the device table there has three
columns against a schema of none, and the new `GpuLimit` is the first node in that path that must
read a row count from it. Added to #281, which is where whoever runs the device tier will look.

#### N5, mine to carry into the signoff

`Empties::fires` firing every source's first call is outside the spec's Scope table. The
reasoning is measured and the change strengthens the dimension assertion rather than weakening
it, but it changes behaviour for every injected-layout run rather than only the limit ones, so it
belongs in the signoff's list of shortcuts and deviations.

#### N6, not acted on

`scripts/exec_model/partitioned_driver.py:190` keeps the old rule. `architecture.md` now records
the divergence, so it is not drift. The reviewer enumerated skip 0–39 × fetch 0–39 at three batch
sizes: the two rules differ only at `fetch == 0` with `skip > 0`, and no prototype case sits
there. Outside this task's Scope table, and the prototype is where a rule is argued with rather
than where it is enforced.

### 2026-10-09 — review round 1 addressed

The markdown half (I1, the `hacks-audit.md` pruning, #281's new line) was done by the human. This
developer closed I2, N3, N4 and N8. N5 and N6 stay as the review left them.

#### I2 — the hold's effect, covered outside the mock tier

New case: `a_limit_stops_a_scan_that_maps_every_row_group_at_every_mode`
(`tests/end_to_end/limits.rs`). `SELECT count(l_quantity) FROM (SELECT * FROM lineitem WHERE
l_quantity > 0 LIMIT 10)`. The `FilterExec` is what makes it bite: DataFusion pushes no limit
through one, so `base_config().limit` is `None`, `source()` builds no cut and `covering_prefix`
has nothing to trim. Measured at every mode: the loader's `partition_groups` cover all 49 of
lineitem's row groups, `l_quantity`'s minimum keeps every group past pruning, and the answer is
10. Batches offered against batches pulled: tp1-single 1/1, tp1-rowgroup **49/1**, tp4-single
4/4, tp4-rowgroup **49/1**, tp4-sized 1/1. The test asserts 49 groups at every mode, `pulled <=
target_partitions` at every mode, and `(49, 1)` for offered/pulled at the two rowgroup modes.

**Proved by flipping the hold off**, not argued. `scheduler.rs`'s `satisfy` kept
`satisfied[node] = true` and lost only the `limit_holds[held] += 1; refresh(held)` loop, so
`report.satisfied` and the goldens' `early_exit=` stay populated — the reviewer's own
construction. Under that, the full `--lib` was 611 passed / 21 failed / 2 ignored. The 21 are
twenty in `executor::driver::scheduler::tests` and `executor::driver::tests::{budget, counts,
limit, render}` — the mock tier — plus exactly one outside it: the new case. The other three
`end_to_end::limits` tests stayed green, which is the gap the review named. The hold was then
restored and all four are green.

#### The reviewer's suggested query is refused at the tp4 modes, and was before this branch

`SELECT * FROM lineitem WHERE l_quantity > 0 LIMIT 10` — the review's own wording — plans at the
tp1 modes (offered/pulled 1/1 and 49/1, the unload carrying the interval) and is **refused** at
tp4-single, tp4-rowgroup and tp4-sized:

    invalid plan: GpuLimit: a limit feeding only the sink is not a node

Above one partition DataFusion plans `GlobalLimitExec(0,10) → CoalescePartitionsExec →
CoalesceBatchesExec(fetch=10) → FilterExec → ParquetExec`. `unload_input` descends a
`CoalesceBatchesExec` only when it has **no** fetch, so the inner one becomes a `GpuLimit` whose
only parent is the sink, and `plan/validate.rs` refuses it. Pre-existing: with `translate`'s body
reverted to the base's (`node(self, &input)` instead of `unload_input`) the same three modes
refuse with the same message, because base's `node()` reached the same `mid_plan_limit` arm.
Measured, not reasoned.

The fix is one arm — let `unload_input` pass a `CoalesceBatchesExec` *with* a fetch and compose
the two intervals with `RowInterval::over`, which is on this branch already and is the identity
here (outer `{0,10}` over inner `{0,10}`). It is a refusal of a legal query, so it is production
behaviour and wants a ticket; it is outside this task's Restriction ("the scan's limit and the
limit's count") and outside its Scope table, so it was not fixed here. **No ticket filed** — this
developer does not write the ticket files; the coordinator was asked to file it.

This also corrects the review's second "checked and found right" bullet, which ends "Anything else
becomes a node and the limit lands legally beneath it." A fetch-carrying `CoalesceBatchesExec`
becomes a node and the limit lands *illegally* beneath it. The descent set is the erasing set, as
the bullet says; what does not follow is that everything outside it is safe.

The new case puts its limit under an aggregate for this reason: there the `GpuLimit` is a node
with a legal parent, it plans at all five modes, and both rowgroup modes show 49 offered for 1
pulled rather than tp1-rowgroup alone.

#### A stale code comment the measurement contradicted

`translator/common.rs`'s `limit_interval` said its `CoalesceBatchesExec` arm was "Not reachable
from today's planner". It is reachable, and it is what makes the query above plan at tp1: with one
target partition DataFusion leaves no `GlobalLimitExec` above the coalesce it parked the fetch in,
so `CoalesceBatchesExec(fetch=10)` is the root. Comment corrected in place, under the rule that a
stale sentence is fixed in the commit that found it. `common.rs` is outside the Scope table; no
code there changed.

#### N3, N4, N8

- **N3.** `settle_limit` (`driver/partitioned.rs`) matches `ExecutorCategory::BatchAccumulator`
  explicitly and returns `RunError::Protocol` naming the node for any other category carrying an
  interval. `settle_limit` and `seed` now return `Result<(), StepError>`; `drive` and
  `driver/tests/mod.rs`'s `driver_with` propagate. The new arm is unreachable today and cannot be
  tested without inventing a third `row_interval()` carrier — which is the condition it guards.
- **N4.** Four lines on `covering_prefix` saying `can_be_null` stays ORed over the untrimmed
  survivors, that `planner/nulls.rs` spends a false positive on a refusal and a false negative on
  a wrong answer, and that the untrimmed claim is also what the scan made before the trim existed.
  So the branch made no refusal more likely than the base does.
- **N8.** `a_mid_plan_limit_is_satisfied_by_fetch_rows_emitted_whatever_its_skip` →
  `a_mid_plan_limit_is_satisfied_by_fetch_alone_and_not_by_skip_plus_fetch`, and its comment now
  says a real limit executor would need three pulls where the mock needs one.

#### Round 2 evidence

**Per-target counts, measured at the final tree.** rust-only `--lib` 632 passed, 2 ignored
(#182), **634 cases** — one more than round 1's 633, the new end-to-end case;
`test_cpu_corpus` 569; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 17;
`test_golden_format` 26; `test_ci_coverage` 9; `cargo test -p cost-report` 36. All rc=0, no
failures anywhere. `build-test.md` moved by exactly one: End to end 36 → 37, `--lib` 633 → 634,
cpu block 1231 → 1232, Rust 1902 → 1903, grand total 2377 → 2378, each header re-summed against
its own rows rather than deltaed. The row's prose enumeration said "seventeen cases no query list
can carry" where the non-query cases numbered nineteen; it is now twenty and adds up (17 queries +
20 = 37).

**Nothing else moved.** `git diff --name-only` over round 2 names only `.rs` and `llm-wiki/`
files: no golden, no `testdata/`, no `corpus_cases.inc`, no `cost-registry.csv`, no `cpp/` and no
`flatbuffers/`. So the deletion audit by `git diff --numstat` has nothing to reconcile outside the
files this round edited, and the cost gate's rows stand exactly as the `## Cost gate` section
records them — the gate reads committed golden sections and none changed.

**The device-only code.** No device ran. The gpu rung was rebuilt with
`CUDF_ROOT=…/rapids-cuda-12.2 scripts/cargo-cudf.sh test -p peacockdb-core --features gpu
--no-run` and the same with `--test test_gpu_corpus`, both rc=0, because `settle_limit`'s
signature changed and the round-1 lesson was that a rust-only `--lib` cannot see a gpu-only
caller. The staged lib binary's `--list` reports **536** `gpu_tests::` cases, unchanged.
`the_registry_matches_the_gpu_corpus_in_both_directions` was run from the built
`test_gpu_corpus` binary and passes, so both registry directions are covered this round.

**C++.** No file under `cpp/` or `flatbuffers/` changed in round 2, so the 25.02 build was not
redone from scratch; the ffi build script reused round 1's `libpeacock_gpu.so` during the gpu rung
build above. `ctest -L cpu` re-run: 1/1 passed.

**One pre-existing warning, unchanged.** The gpu rung still reports the unused `AsArray` import at
`tests/gpu_tests/aggregate_dimension_cases.rs:9` — one warning, the same one round 1 recorded, and
the only one either build emits.

### 2026-10-09 — round 1 findings closed, state completing

Committed as `7d4f92d8`. The important finding is closed with a measured proof rather than an
argument: the hold's refresh loop was removed, leaving `satisfied[node] = true` so
`report.satisfied` and the goldens' `early_exit=` lines stayed populated, and `--lib` went 611
passed / 21 failed — twenty of them the mock tier and the twenty-first the new end-to-end case.
The other three `end_to_end::limits` cases stayed green, which is the gap the finding named.

Counts after round 2: `--lib` 632 passed + 2 ignored = 634, `test_cpu_corpus` 569,
`test_corpus_goldens` 26, `test_cost_model` 3, `test_module_layout` 17, `test_golden_format` 26,
`test_ci_coverage` 9, `cost-report` 36, `ctest -L cpu` 1/1, the gpu rung built and listed at 536
cases. `build-test.md` moved by one — End to end 36 → 37, `--lib` 634, cpu 1232, Rust 1903, grand
total 2378 — and the developer also corrected prose that was already wrong there, "seventeen
cases no query list can carry" where they were nineteen. No golden moved and no `testdata/`,
`cpp/` or `flatbuffers/` file changed in this round, so `## Cost gate`'s rows still stand.

#### [#284](../tickets/complete-coverage.md#t284) filed — a limit over a filter is refused above one lane

The developer sanity-checked the reviewer's suggested query instead of taking it, and found half
of it refused: `select * from lineitem where l_quantity > 0 limit 10;` is `invalid plan: GpuLimit:
a limit feeding only the sink is not a node` at the three tp4 modes. Above one partition
DataFusion parks a **fetch-carrying** `CoalesceBatchesExec` between the root limit and the
filter, and `unload_input` descends that node only where it has no fetch, so the inner fetch
becomes a limit whose only parent is the sink.

It predates this branch, and that was measured, not reasoned: reverting `translate` to the base's
`node()` call gives the same refusal at the same three modes. A refusal of a legal query is
production behaviour, so it is filed rather than noted — #284, in complete-coverage.md, since no
corpus query has the shape. The fix is one arm and the composition it needs is already written
and already tested.

This also corrects the review's own closing sentence, that anything outside the descent set
"becomes a node and the limit lands legally beneath it". The descent set is the erasing set, as
the review says; what does not follow is that everything outside it is safe. A comment in
`limit_interval` claiming its `CoalesceBatchesExec` arm is "not reachable from today's planner"
went with it — it is reachable, and at one target partition it is what makes that query plan.

### 2026-10-09 — completeness pass: 0 blocking, 7 important across the two readings

Neither reading found a defect in the code. Both found prose the branch falsified, and they did
not overlap — the reviewer on the one sentence that states a *count*, the analyst on three other
sections and two ticket bodies. That convergence on a class rather than an item is the thing worth
remembering: this branch's code was right twice over and its paper trail was wrong in seven places.

**Reviewer's one (what is wrong).** `architecture.md`'s *Intervals nest* said two intervals on one
root-to-leaf path and named `tpch/nested-limits` as the example two sentences later — and the
branch makes that query carry three, from its own committed golden: the unload's 3..+20 over a
`GpuLimit` 5..+23 over the part scan's 0..+28. No production consequence — nothing in the code
caps the depth — but "at most two per path" is a wrong premise for a validator rule or an
estimator assumption, which is what that page is read for. Rewritten to say three, to name the
path, and to say explicitly that no rule caps the count. The same stale claim in
`tests/end_to_end.rs`'s `nested-limits` comment went with it; the sibling comment in
`end_to_end/limits.rs` had already been updated for exactly this and this one had not.

**Analyst's six (what is missing).**

1. `tickets/memory.md` #182 and #179 both pivot on nested-limits' `part` loader mapping two row
   groups in one lane at tp4-rowgroup, and the trim made it one. #182's "fix proposed" repointed a
   test pair at that query; #179's only positive example was that query, and its "1.63x" is
   literally 1,600,062/983,071, the two-groups-to-one ratio. Both corrected to say the corpus now
   has **no** candidate for the rebatcher half, so it wants a query written for it rather than one
   repointed at, and #182's two `#[ignore]`d cases stay ignored until there is one. This is the
   finding that would have cost a later agent the most: they would have implemented a fix that
   cannot work.
2. `tickets/df-upgrade.md` #166 said `limit_interval`'s root-coalesce arm is unreachable on
   DataFusion 45 and "deliberately defensive". Round 2 measured the opposite, and corrected the
   code comment but not the ticket or `hacks-audit.md` §4. Both corrected; #166's open half is now
   "live and still untested", its proposed test is the real query rather than a hand-built node,
   and it cross-references #284, which is the same arm from above one lane.
3. `architecture.md`'s limit lowering rule described the trim and not its complement: where
   DataFusion could not push the cut, there is nothing to trim and the driver's hold is the whole
   bound. Added, with what the round-2 test measured — 49 groups offered and 1 pulled at the
   row-group modes, and at the single-batch modes a lane's one batch already being the whole
   mapping, so the table is read before the limit can be satisfied.
4. `architecture.md`'s "`CudfLimit` (a limit is a row range on the export)" is now true only
   root-adjacent; nested-limits alone has three limits driven by `slice_handle`. The parenthetical
   states the real reason instead: a limit's bounds are runtime values.
5. `plan/mod.rs`'s `can_be_null` doc said "the surviving row groups", which now means something
   different from the `survivors` field three lines above it. The invariant was written, four good
   lines, but a module away in `covering_prefix`. A clause on the field closes it.
6. The new invariant nobody wrote down: a scan-pushed root-adjacent `LIMIT` returns the same rows
   at every mode, by construction — one lane, the covering prefix in index order. *Determinism
   rules* names an unordered `LIMIT` as its example of what may differ across plans, so it gains
   the exception. The coverage consequence (scan-limit could carry a shared result golden instead
   of a per-mode `live_cpu` run) is out of Scope and went into the signoff rather than the branch.

Plus one observation the reviewer declined to file: the injector's `emitted` counter now guards a
state that cannot happen, since `Empties::Sometimes` fires every first call. That is the
legitimate way to retire a guard — make the property structural — but the comment still described
the old reachable state. Corrected in place.

#### Checked and found right, so nobody re-checks it

- **`RowInterval::over` is correct for any pair, and unreachable with a non-identity inner.** The
  reviewer traced DF45's `LimitPushdown`: the scan's limit is always `outer.skip + outer.fetch`
  from the same requirements that produced the kept node, `CoalescePartitionsExec` has no `fetch`
  field at all, and both other descent arms clear the pushdown state, so the inner is always
  `{0, skip+fetch}` and the fold is the identity. One unit test is adequate cover for what is
  reachable.
- **The prefix trim is safe for a firmer reason than round 1 gave.** Not "`pushdown_filters` is
  never enabled" but: a scan carrying a pushed limit can have no predicate at all, because the
  only thing that gives `ParquetExec` one is a filter DataFusion kept above it, and a limit is
  never pushed through a filter. With `with_row_groups` and no `RowSelection`, every row of an
  opened group is returned, so `covering_prefix` cannot come up short.
- **The `--- memory ---` sections moved correctly**, recomputed figure by figure rather than
  accepted. The rowgroup modes' estimate *rose* (142 → 229) while the single modes' did not: the
  old 137 was `983071 × 224/1,600,000`, an artifact of dividing the real batch by the untrimmed
  row count. All five modes now agree at 229/224/5, which is the signature of a correct move.
- **The estimator's second pass still addresses the same sources in the same order.**
  `batching_for_source` is called once per `loader()`, no path reaches both callers for one
  `ParquetExec`, and the trim depends only on `config.limit` and the parquet metadata — not on the
  batch size — so it is byte-identical across the two passes.
- **The cost gate's +228 is exactly accounted**: `cuda_limit_bytes` 187 → 415, where
  415 = 187 (the 23-row offset limit) + 228 (the new 28-row cut over part) + 0 (region, zero
  columns). No third regression, five improvements.
- The estimator does not treat the new node as an accumulator (`NodeRef::Limit` falls to
  `_ => None`), so it neither truncates the amplification walk nor comes off the budget.

### 2026-10-09 — done

CI run [37898744572](https://github.com/asymptote-tech/peacockdb/actions/runs/37898744572) on
`34bd7f4c`. Green: changes, both dataset-matrix legs, cpp-build-2502, s3-datasets;
deploy-pages skipped as a master-push job.

Two jobs red, both of them the cases the board says not to wait on.

**cost-report** — the gate, and it reports exactly what the human pre-accepted and nothing more:
*5 improvements, 2 regressions*, the two being `tpch.sf1/nested-limits` at tp1-rowgroup-mini and
tp4-rowgroup-mini, 976.44 KB → 976.66 KB each. That is the +228 bytes `## Cost gate` predicted
from the local run, at the same two sections, with no third. The five improvements are
nested-limits at the other three modes (−38.51%) and scan-limit at tp4-single and tp4-sized
(−97.95%).

**GPU Tests** — `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection timed out`, at
the rsync step. The host is unreachable, so no pool was built: not
[#178](../tickets/testinfra.md#t178), not this branch, and the same failure both earlier chain K
tasks saw today.

Everything the chain can test is green, so the task is `done` and the human merges. `#281` holds
the device half, and `#284` holds the refusal round 2 uncovered.
