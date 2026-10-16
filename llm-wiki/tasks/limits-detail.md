# limits — working record

## Standing facts

- Third of chain K, branch `ENS-limits` off `ENS-distinct-companions`, PR will target that
  branch. Closes [#186](../archive/archived-tickets.md#t186) and
  [#234](../archive/archived-tickets.md#t234).
- **The GPU half is done, on nebius-gpu** (`dmitry@89.169.109.150`, `~/peacockdb-K`, cuDF 25.02),
  under chain K's board note; the last section of this file is its record. CPU builds and runs
  stay local — verda does not resolve from this host. `done` is CI green except the `gpu-tests`
  job, which runs on the unreachable shad-gpu. The no-GPU rule the rounds below were worked under
  is superseded; #281 no longer holds anything of this task's.
- **The cost gate passes on its own now.** The human pre-accepted the two `nested-limits`
  rowgroup rises (+228 bytes, 2026-10-08, on the board), but master `bc9b6e2f` moved the gate to
  fail only past +10% and they are +0.02%, so the job is green and the acceptance is no longer
  load-bearing. The local `--cost-diff` against the current merge-base is in the last section.
- The chain's base is master `bc9b6e2f` after the second rebase (`31c56bea` before it); the
  merge-base with `ENS-distinct-companions` is `8a831978`, which is what `--cost-diff` compares
  against. `guard-checks-detail.md` records what the first rebase brought across. Nothing in this
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

### 2026-10-09 — done (pre-rebase; superseded)

**This section is the cpu half's `done`, before the second rebase.** `34bd7f4c` is no longer an
ancestor of this branch — the rebase onto master `bc9b6e2f` replaced it — so the run below says
nothing about the current head and `done` needs a fresh one.

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

## The GPU half — reopened 2026-10-09

The human reopened this task from `done` to `building`. The cpu work is closed and #186 and #234
are archived; what is left is the half the chain could not run while it had no GPU, filed as
[#281](../tickets/corpus-coverage.md#t281): **this task's device changes are built and have run on
no device.** `scan.cpp` no longer calls `set_num_rows`, since a `GpuLimit` above the scan now
makes the cut, and the four `bug_` cases in `gpu_tests/source_cases.rs` that the change turned into
agreement cases — one of the scan, three of a `GpuLimit` over the scan's batches — have never
executed on a card.

### Second rebase, onto master bc9b6e2f through ENS-distinct-companions

Nine commits replayed. Every conflict was bookkeeping, and two were resolved by ownership rather
than by side:

- `tasks.md`, once — this branch's own state progression, so the replayed side stands.
- `testdata/cost-registry.csv`, once, on a hunk holding three rows. `rollup_distinct` and
  `distinct_functions` are distinct-companions' and took the base's side (that task's GPU half
  enabled five device cells and moved `262`); `scan_limit` is this task's and took the replayed
  side. Taking either side wholesale would have reverted one task's work.
- `build-test.md`'s grand total and its cpu block header, twice each. The replayed side is an
  absolute computed off the old base, so each was resolved by applying the replayed commit's own
  delta to the base's numbers. Checked afterwards the way the page says: the rows sum to Rust 1919,
  C++ 94, Python 381, grand total **2394**, and the cpu block's 1241 is 640 + 569 + 29 + 3.

No code, golden or test conflicted. Nine commits above `ENS-distinct-companions`, this task's own
count, so the PR diff is still the task.

### What the GPU run must do

`tpch/scan-limit` and `tpch/nested-limits` each have all five device cells off on #281 alone
(`cost-registry.csv` fields 14-18). Run them; each cell that passes is enabled, each that fails
gets the ticket it fails on, and #281 narrows rather than closing — empty-sorts owns the rest of
it. Then the device tier, which is where the four converted `source_cases.rs` cases live.

**One outcome to expect rather than be surprised by.** `nested-limits` carries a cross join
(`features` says `limit_offset cross_join`), and a `GpuCrossJoin` consumes its build side on the
first call — [#152](../tickets/joins.md#t152). At `tp1-single` the probe is one batch and that is
fine; at the row-group and sized modes it may not be, in which case those cells are #152's and not
this task's. Measure it rather than assuming either way.

`scan-limit`'s device oracle is `live_cpu`, not a golden: its result section is skipped, so the
device answer is compared against a fresh cpu run at the same mode. `nested-limits` is
`golden_exact`.

### 2026-10-09 — the GPU half ran on nebius-gpu

**The tree it ran on.** The working tree that became `a47aff90`. Two commits landed after it:
`04547750`, comments and wiki only, and `dbfc4079`, which edits `flatbuffers/gpu_plan.fbs` and so
regenerates both generated files and relinks `libpeacock_gpu.so`. That edit is comment-only and
the regeneration was proved identical with comment lines stripped — md5s in the round below — so
this run stands for the final tree. Nothing else compiled has moved.

Everything #281 held for this task is now measured. The device tier is green at 537 + 38 + 1 + 8
+ 4 + 56 cases, `tpch/scan-limit`'s five device cells are **enabled**, `tpch/nested-limits`' five
are **off on [#285](../tickets/corpus-coverage.md#t285)** — a defect the run found that is not
about a limit — and #281 narrows to empty-sorts' half alone.

**Host and commands.** nebius-gpu, `dmitry@89.169.109.150`, `~/peacockdb-K`, cuDF 25.02, card
free the whole time (`nvidia-smi` 0 MiB; chain J never contended). `df -h /` 21 GB free before
each of four builds and unchanged after, so chain J's cleanup rule was never reached and nothing
was deleted. Each build: `rsync -a --delete-after --exclude=.git --filter=':- .gitignore' ./
dmitry@89.169.109.150:peacockdb-K/`, then `find … \( -name '*.rs' -o -name '*.inc' \) | xargs
touch` on the host, then `./scripts/build-test-shadgpu.sh --build` detached, never `--run`. Runs
are the staged binaries directly under `LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib`,
`PEACOCK_TESTDATA_DIR=$PWD/testdata`, `--test-threads=1`. Logs in `~/K-logs/limits-build[1-4].log`
and `limits-tier{,2}.log` on the host.

**The rsync-mtime antipattern was checked, not assumed.** After the first sync a second
`rsync -ain` with the same filters listed nothing, so the host's content equalled the worktree's;
the sources were `touch`ed before every build anyway. Each build was then confirmed by a literal
the test reads rather than by its exit code: `nm -C cpp/install/lib/libpeacock_gpu.so | grep
set_num_rows` went from `U cudf::io::parquet_reader_options::set_num_rows(int)` (the base's call
site, still staged from distinct-companions' build) to **nothing**, which is the proof that the
staged C++ is this branch's `scan.cpp`; and `--list` on the staged Rust binaries named the four
converted cases and then the new `bug_` one.

#### Per-binary counts, final run (`~/K-logs/limits-tier2.log`)

| binary | ran | passed | filtered |
|---|--:|--:|--:|
| `peacockdb_core_gpu_lib gpu_tests::` | 537 | 537 | 644 |
| `test_gpu_corpus` | 38 | 38 | 0 |
| `test_node_timing` | 1 | 1 | 0 |
| `peacock_gpu_benchmarks --skip bench_` | 8 | 8 | 3 |
| `cpp/install/bin/peacock_gpu_tests` | 4 | 4 | — |
| `cpp/install/bin/peacock_plan_tests` | 56 | 56 | — |

Every one of them executed tests; none is a zero-test pass. The 644 filtered out of the lib
binary are its cpu and ffi rungs, which `gpu_tests::` excludes by design; the 3 filtered out of
the benchmark binary are its `bench_` cases, which `--skip bench_` is there to drop.

#### scan-limit: five cells enabled, and its oracle was wrong

All five pass, and they pass against the **result golden**, not a live cpu run. The committed
declaration said `live_cpu`, and that is refused the moment a device cell runs:

    tpch/scan-limit at tp1-single on a device: gpu_oracle is live_cpu and `.result.txt` holds
    this query's rows — a device-side cpu run for a comparison the committed section already makes

`mini.result.txt` has held a real `== scan-limit` section at `mode=tp4-sized` all along — the
brief for this round, and `corpus_gpu.rs`'s own doc, both say that section is skipped, and it is
not. So `live_cpu` → `golden_exact`, which is strictly more coverage: the device's answer at each
of the five modes is now compared to the cpu-authored tp4-sized rows, and all five passing is the
first measurement of the completeness pass's analyst finding 6 — that #186 made a scan-pushed
root-adjacent LIMIT return the same rows at every mode, one lane over the covering prefix in
index order. Nothing on the cpu tests that: `authoritative_mode` asserts the result section at
the **last enabled mode only**, so before this the four other modes' answers were compared to
nothing but DataFusion's subset.

That edit then reddened a cpu guard, `each_declarations_two_oracles_suit_each_other`:

    tpch/scan-limit: gpu_oracle is golden_exact where no committed section can serve it
    (cpu_oracle is data_fusion_subset, so the modes need not agree), so it fails on correct behaviour

**The guard conflated two claims, and `build-test.md` was already right about which one matters.**
Its `needs_live` was `over_cap || cpu_oracle == "data_fusion_subset"`. But `data_fusion_subset`
says *DataFusion's pick and ours need not be the same rows* — `build-test.md:35` says exactly
that — which is not the same as *our own five modes differing*. scan-limit is both: undetermined
against DataFusion, identical across the modes since #186. `build-test.md:737` already states the
device-side rule keyed on the right thing ("live_cpu … for a query whose section is skipped"), and
`corpus_gpu.rs`'s check keys on it too; only the cpu guard used the proxy. So the term is gone and
both guards now ask the one question they are both about — whether a committed section serves.
Code-vs-wiki drift with the wiki in the right, so it was fixed rather than filed.

**The guard still goes red both ways, constructed rather than argued.** scan-limit
`golden_exact` → `live_cpu` with its frozen section reddens the second branch; `filter-project`
`live_cpu` → `golden_exact` with its over-cap marker reddens the first. Both reverted, and the
guard is green on the final tree. It has no unit cases of its own, which is why this was done by
construction. `data_fusion_subset` is on exactly one line (scan-limit), and the other four
`live_cpu` lines — filter-project, semi-join, anti-join, q16 — all rest on `over_cap` alone and
keep the same verdict, checked against their sections.

#### nested-limits: five cells off, and the cause is not a limit

Red at all five modes, every one at the same place — the cpu golden's section, not the answer:

    tp1-single-mini.cpu.txt: `nested-limits` moved — line 1, column 14 —
    expected `early_exit=GpuUnload@8,GpuLimit@5,GpuLimit@4`
    actual   `early_exit=GpuLimit@5,GpuLimit@4` (+10 more lines)

`line_difference` shows one line, so the device's whole rendering was dumped under a temporary
`PCK_DUMP_RUN` hook in `corpus_gpu.rs` (added, used, reverted; the final tree has no diff there).
It says everything:

    GpuUnload: skip=3, fetch=20, output_rows=0 … in_rows=[[0]] batch_rows=[[]]
      GpuCrossJoin: in_rows=[[0],[23]] batch_rows=[[]]
        GpuCoalesceAllBatches: output_rows=0, batch_rows=[[]]
          GpuLimit: skip=0, fetch=23, output_rows=0, in_rows=[[0]] batch_rows=[[]]
            GpuLoadParquet: table=region, projections=[], output_rows=0, batch_rows=[[0]]
        GpuProject … GpuLimit 5..+23 in_rows=[[28]] → 23 … GpuLimit 0..+28 in_rows=[[122880]] → 28

**The part side is perfect** — 122,880 → 28 → 23, both of this branch's new limits over a scan
doing exactly what the cpu golden says. **The region scan reads 0 rows where the cpu reads 5**,
and everything after follows: the `GpuLimit` drops the zero-row batch (#214, which this task's
Restriction left alone), the coalesce has nothing, the cross join's build side is empty, the
unload never reaches `skip + fetch = 23` and so never satisfies, and the query answers nothing
instead of twenty rows. **The expected #152 outcome did not happen**: the cross join's build-side
copy is not in play at any mode, because there is nothing to copy. Nothing refused and nothing
warned.

The root cause is **#285**, filed: `CudfScan.file_schema` is the node's *declared* schema and
`projection` is left empty (`wire/node_writer.rs::scan`), which `scan.cpp` reads as every column
of the schema it was given — and region's declared schema is `schema=[]`, so it selects none, and
a cuDF table of no columns reports `num_rows() == 0`. Pinned at the operator level by
`bug_a_scan_declaring_no_column_reads_no_rows_on_the_device` (`gpu_tests/source_cases.rs`), which
has no limit in it at all: 64 rows on the cpu against 0 on the device, over one parquet. That pin
is what makes the diagnosis independent of this query — and its red step was watched first, as
an agreement case, before it was turned into the pin.

**It predates this branch**, and the branch only changed the symptom. `region`'s declared schema
is `schema=[]` at the base too (`git show 8a831978:…/tp1-single.plans.txt`), and the
`set_num_rows(23)` the branch dropped was on the same zero-column read, where it could not have
made cuDF answer five rows. What the branch did change is that the new `GpuLimit` above the scan
now swallows the zero-row batch, so nothing reaches `cudf::cross_join` — which is why the symptom
is a silent empty answer rather than the "Left table is empty" refusal #63 predicted for this arm.
The base's device behaviour for this query is **not measured**: its cells were off.

**#285 is the report's unfiled ticket, not a duplicate.** `reports/corpus-fixes.md`'s "Tickets to
file" item 1 is this defect, predicted at `188c23ce` and never filed; the same report's coverage
table says "nested_limits' first device run after this fails at the `region` scan — ticket 1, not
a new one", and `join-rewrite-cell-estimate.md`'s R4 names the zero-column scan too. #63 owns the
same root cause at a *project* and names this arm only inside its fix paragraph, with tpcds/q9 as
its query and no corpus query or pin for the scan. So #285 was filed with the measurement, the
corpus query and the pin, cross-referenced to #63 both ways, and #63's closing sentence corrected
— measured, the scan arm answers empty rather than refusing. The two share one fix.

#### What the four converted cases proved about dropping `set_num_rows`

All five of the file's agreement cases ran on the card and passed — the four this task
converted from `bug_` pins, and the one that was already an agreement case:
`both_backends_read_the_same_batches_per_row_group`,
`a_scan_of_one_row_group_reads_it_whole_on_both`,
`a_limit_over_a_scan_of_one_row_group_keeps_its_first_rows_on_both`,
`a_limit_inside_the_first_of_four_scanned_row_groups_drops_the_rest_on_both`,
`a_limit_across_a_scanned_row_group_boundary_slices_the_second_group_on_both`
(the first two of the scan, the last three of a `GpuLimit` over the batches a scan emitted).

Together with scan-limit's five corpus cells that closes #186's device half: a scan with its
limit removed reads its row groups whole on a device, byte for byte as the cpu does, and the cut
is made above it — by a `GpuLimit` in the harness cases, and by the unload's interval in
scan-limit, whose section matches the cpu's `early_exit=GpuUnload@1` and `in_rows=[[122880]]` at
all five modes. Where it is *not* proved: a zero-column scan, which never worked and is #285.
Note that the harness cases could not have caught #285 before — every one of them declares the
file's own schema, so none selects no column.

#### CPU, re-proved locally on master `bc9b6e2f`

| target | result |
|---|---|
| rust-only `--lib` | 638 passed, 2 ignored (#182) = 640 cases |
| `--test test_cpu_corpus` | 569 |
| `--test test_corpus_goldens` | 29 |
| `--test test_cost_model` | 3 |
| `--test test_ci_coverage` | 9 |
| `--test test_module_layout` | 17 |
| `--test test_golden_format` | 26 |
| `cargo test -p cost-report` | 38 |
| C++ `scripts/build.sh --build` (cuDF 25.02, g++-12) | rc=0, 20 ninja steps, no warning |
| `ctest -L cpu` | 1/1 |

All rc=0. The registry tests both ways are inside these:
`the_registry_matches_the_cpu_corpus_in_both_directions` in `test_cpu_corpus` and
`the_registry_matches_the_gpu_corpus_in_both_directions` in `test_gpu_corpus` on the device —
both green, which is what holds the ten `cost-registry.csv` gpu cells to the `corpus_query!`
lines in both directions.

**Warnings.** Neither build adds one. The device build logs zero. The local cpu side logs exactly
one, `function sha_links is never used` at `cost-report/src/main.rs:1538` — master's, present
verbatim in `bc9b6e2f` and in no file this branch touches. Cosmetic, so no ticket. The gpu rung's
old unused-`AsArray` warning at `aggregate_dimension_cases.rs:9` is **gone**: master's rebase
carried the fix.

#### The cost gate is green now, not red

`cargo run -q -p cost-report -- --cost-diff --base 8a831978` (the new merge-base after the second
rebase): **563 compared, 7 changed, 2 regressions, 0 over 10%, rc=0.** The two are exactly the
pre-accepted ones — `tpch.sf1/nested-limits` at tp1-rowgroup-mini and tp4-rowgroup-mini,
976.44 KB → 976.66 KB, +0.02% each, the +228 bytes `## Cost gate` predicted — with no third, and
five improvements (nested-limits at the other three modes −38.51%, scan-limit at tp4-single and
tp4-sized −97.95%). **The job will pass**: master `bc9b6e2f` moved the gate to fail only past
+10%, so the human's pre-acceptance is no longer load-bearing for `done`. Nothing this round
touched a golden, a `.cpu.txt` or a `.cost.txt`, so these rows are the committed sections'.

#### Counts moved in `build-test.md`

gpu block 581 → **587**: `--lib -- gpu_tests::` 536 → 537 (the #285 pin), `test_gpu_corpus`
33 → 38 (scan-limit's five cells). Rows re-summed rather than deltaed — the block's ten rows sum
to 587, "Corpus, device" 32 → 37 and "Operator harness" 335 → 336 — and the page's three top
figures moved by the same six: Rust 1919 → **1925**, grand total 2394 → **2400**, with all 77
table rows on the page summing to 2400 and C++ and Python untouched. The "Corpus, device" prose
now says thirty-six cells, names scan-limit at every mode, and sends nested-limits to #285.

#### Deferred, and nothing else

As the board's host override requires: `peacock_tpch_tests` and `peacock_tpchv_tests` (sf40),
`--run-benchmarks`, and any Nsight capture — not run, not attempted. `peacock_cpu_tests` stayed
local. Nothing else was skipped, and no test was disabled.

#### For whoever is next

- **`#285` is the one thing standing between `tpch/nested-limits` and five device cells**, and it
  is a two-line C++ or wire change away. When it closes, those five cells go on in the same two
  coordinated edits (`corpus_cases.inc`'s gpu modes and the registry's `gpu_*` columns) and #63's
  q9 cells come with it. `#214` is the second node in that chain but not a blocker: with a scan
  that reads its rows, the limit has a non-empty batch and never reaches its zero-row arm.
- **#281 now means empty-sorts only.** Its title, body and index entry were rewritten; it has no
  registry row on this branch any more, and `tpcds/q17`'s row (on `ENS-empty-sorts`) is its last.
- **A device cell can be red on its declaration rather than on the engine**, and both of the ways
  that happened here were invisible until a card ran: `corpus_gpu`'s oracle check and the cpu
  guard's pairing rule disagreed about scan-limit for as long as its cells were off. Anyone
  turning on a first device cell for a query should read both before building.

### 2026-10-09 — review round 1 on the device half: the fbs contract

One finding was mine, `flatbuffers/gpu_plan.fbs:331`. Both lines fixed:

    /// The node's PROJECTED schema, not the file's: the writer puts the declared columns
    /// here, by name, because a plan node does not carry the file's own column list.
    file_schema: Schema;
    /// Indices into `file_schema`. No writer fills it; C++ reads empty as every column of
    /// `file_schema`, not of the file — so a node declaring none selects none (#285).
    projection: [uint32];

They replace "Schema of the files (before projection)" and "Column indices to project (empty =
all columns)", which together stated the model #285 is made of. "No writer fills it" was checked
rather than assumed: `CudfScanArgs` is constructed exactly once (`node_writer.rs:90`) and sets
`file_paths` and `file_schema` only — the `projection` hits elsewhere in that file belong to
`CudfFilter`. Four comment lines, two per field, against `coding-style.md`'s ten-above-a-
declaration cap, counted; and reflowed to 88 bytes because the first draft's 94 was the only
comment line in the whole schema over 90 (the file's own maximum is 89, measured).

#### `.fbs` doc comments DO reach generated code — the byte-identical check fails, by design

flatc 25.12.19 copies a field's `///` into the generated accessor in **both** languages. The
premise that a comment-only `.fbs` edit leaves codegen untouched is false, and this is what moved:

    gpu_plan_generated.rs:4021  and  gpu_plan_generated.h:2284   (file_schema's doc)
    gpu_plan_generated.rs:4029  and  gpu_plan_generated.h:2288   (projection's doc)

and **nothing else**. Measured three ways rather than read off the diff:

- Both files regenerated from the base revision's schema and from this one, with the two vendored
  `flatc` binaries directly (`flatc-fork`'s for `--rust`, `cpp/build/_deps/flatbuffers-build/flatc`
  for `--cpp`, both 25.12.19). `diff` is exactly those four lines → four lines, twice.
- **Comment lines stripped, the two outputs are byte-identical**: Rust
  `6863f2f05718bc2b633720054cf07672` before and after, C++ `d03da32914cbf0829546bf770579da42`
  before and after. Non-comment line counts unchanged at 6520 and 4101; totals +2 each.
- `CudfScan`'s vtable slots are unchanged and still carry the branch's deprecation gap:
  `VT_FILE_PATHS=4, VT_FILE_SCHEMA=6, VT_PROJECTION=8, VT_BATCH_SIZE=10, VT_ROW_GROUPS=14,
  VT_BATCHES=16` — slot 12 absent, which is `limit (deprecated)`.

So no accessor signature, builder, verifier, `VT_` constant or offset moved; the wire contract is
untouched and only the prose flatc copies changed. The live artifacts were then confirmed to be
that same output: `cpp/build/generated/gpu_plan_generated.h` (cmake regenerated it —
`[1/20] Generating gpu_plan_generated.h from FlatBuffers schema`, then 20 targets rebuilt) and
cargo's `OUT_DIR/gpu_plan_generated.rs` are both byte-identical to the independent regeneration,
`ad1d566a…` and `fc1c66f0…`.

**`testdata/goldens/recipe-payloads.txt` is untouched**, `dd51d58a4baf985d597792d211a46eb3` before
and after, and `git diff --name-only testdata/` is empty — nothing under `testdata/` moved, so no
digest was regenerated to silence anything.

#### One code comment the edit falsified

`peacockdb-core/src/wire/generated.rs:1` said flatc's output is "7,252 lines"; it is **7,254** now
and says so. `limits-impl.md:1638` records a round already having to correct this number once, so
it is a standing trap: **any `.fbs` edit that adds or removes a line moves it**, comments included.

#### Counts after the round

`--lib` 638 passed + 2 ignored = 640, unchanged. The payload goldens were run by their real path
and **ran**: `planner::tests::plan_goldens::the_payload_golden_carries_what_each_call_hands_the_executor`
and `…_covers_every_kind_and_call_shape_the_modes_produce`, 2 ran / 2 passed / 638 filtered. The
first attempt at them, `--lib -- the_payload_golden_carries_… --exact`, reported
**0 passed, 640 filtered out, rc=0** — the zero-test pass, caught by reading the counts rather
than the exit code; the name needs its `planner::tests::plan_goldens::` prefix. C++
`scripts/build.sh --build` rc=0 with the regenerated header, 20 targets, no warning, and
`ctest -L cpu` 1/1. No device run: nothing here reaches one.

### Deferred from the completeness pass

`flatbuffers/gpu_plan.fbs`'s new `projection` comment says "No writer fills it", which is true of
the recipe writer and not of `cpp/tests/gpu/test_plan_executor.cpp`, where four hand-built plans
do. The neighbouring `row_groups` comment protects itself with "reachable only from a hand-built
plan" and this one should say the same. Left alone deliberately: a `.fbs` comment is a codegen
input, so the qualifier costs a regeneration round and a fresh proof that nothing but the doc line
moved, and a nit does not buy that at a completeness pass. For whoever next edits that file.

### 2026-10-09 — done, on the GPU half

CI run [37989280042](https://github.com/asymptote-tech/peacockdb/actions/runs/37989280042) on
`dbfc4079`, **the last code-carrying commit**. Every job green — changes, both dataset-matrix
legs, cpp-build-2502, cost-report and s3-datasets; deploy-pages skipped as a master-push job.

**The cost-report job is green, where it was red before the rebase.** Master's `bc9b6e2f` moved
the gate to fail only past `REGRESSION_FAIL_PCT` (10%), and this branch's two accepted sections
are +0.02%. They are still reported red in the PR comment, as intended. So the human's
pre-acceptance in chain K's board note is no longer load-bearing for this task.

GPU Tests failed at rsync with `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection
timed out`, exit 255 — shad-gpu unreachable, so no pool was built: not
[#178](../tickets/testinfra.md#t178) and not this branch. Chain K's rule exempts that job and asks
instead for a recorded nebius-gpu pass, which is the section above.

The four commits after `dbfc4079` are documentation-only and skipped every job behind the
`changes` gate, so their green asserts nothing. Read the SHA before the conclusion.

### 2026-10-16 — third rebase, onto ENS-distinct-companions on master ca4dedeb

master brought chain J's eight tasks, gpu-host, and the fold of chain L into chain K.
guard-checks and distinct-companions are `done` again on it. This branch was replayed with
`git rebase --onto ENS-distinct-companions 8a831978`. One developer resolved every code stop.
In short:

- **5f70fe64, the main commit, against master's `rows_only`.** master's `__rowmarker__` scans
  (`CudfScan.rows_only`) read the scan's limit, both to cap the count and in the #186 refusal.
  The decision: a rows-only scan carries no limit, like any other scan. The `GpuLimit` above it,
  or the unload's interval, cuts the marker rows, and the covering-prefix trim still applies.
  `scan.cpp`'s `rows_of(paths, rgs)` takes no limit. flatc writes no accessor for a deprecated
  field, so master's `scan->limit()` could not have compiled. Two of master's #186 `bug_`
  rows-only cases became one green case,
  `a_limit_over_a_rows_only_scan_keeps_its_first_marker_rows_on_both`.
  `ScanRowsOnlyAnswersTheRowCount` lost its limit assertion. `generated.rs`'s line count was
  recounted with the vendored flatc; master's figure was already stale.
- **a47aff90, the device half: #285 is not filed on this branch.** On the old base, nested-limits'
  region scan declared no column and the device read zero rows. master's `rows_only` builds such
  a scan's rows from the footer, which is #285's own proposed fix. So the pin was dropped as a
  duplicate of master's `a_rows_only_scan_agrees_on_both_backends`. nested-limits' five device
  cells are on, beside scan-limit's five, **to be proven** in the GPU cycle, which also records
  their `gpu-result.txt` sections. Until then 11 rust-only `test_cpu_corpus` cases are red by
  design.
- **A design call on master's test, recorded for review.**
  `an_undetermined_lines_rows_are_compared_only_against_committed_files` refused any device cell
  on a `data_fusion_subset` line. scan-limit is such a line. The rule is now that such a cell must
  be `golden_exact`, which holds the recorded rows to the committed section at every mode. The
  `order_hazard.rs` comment that pointed at the old rule was reworded.
- **dbfc4079.** The schema's `projection` comment ended on #285. It now says that a scan of no
  column sets `rows_only` (#63).

Markdown, resolved by the coordinator. `tasks.md` by ownership; this task's state on the branch
board is `rebase needed(done)`. #186 and #234 are archived in master's format, with the index at
104 open. #284 keeps its number: master skipped 284 and 285, and its next free number is 287.
`architecture.md` was reconciled against the task's own final text, without #285, and with
`CudfScan`'s `limit` gone from both of its rows. `build-test.md` waits for the measured counts.

Owed before `done`:
- regenerate the 15 tpch plan and mini goldens and `recipe-payloads.txt`;
- the cpu tiers, the cost-report Python suites and the cost diff;
- a GPU cycle on nebius-gpu that proves or reverts the ten cells and records their sections;
- a dispatched CI.

#### Re-proved on master ca4dedeb

Branch head `5745609c`, against `ENS-distinct-companions` `5dd0abf7`. CPU work local (verda down);
the device on nebius-gpu in `~/peacockdb-K`. Everything green, and all ten new device cells pass.

**Disk.** `df -h /` on the VM read 42 GB free before the build (55 GB used of 96), and 41 GB
after. Above the 20 GB line, so nothing was removed.

| step | command | result |
|---|---|---|
| plan goldens | `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 cargo test --features rust-only -p peacockdb-core --lib -- planner::tests::plan_goldens --test-threads=2` | 26 passed, first pass |
| cpu regen | `UPDATE_CANONICAL=1 … --test test_cpu_corpus -- --test-threads=2` | 1994 passed, 11 failed: the 10 `duckdb_gpu_*` cells of scan-limit and nested-limits and the coverage check, each "holds no section for this cell" |
| `--lib` | `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2` | 805 passed, 0 failed, 2 ignored (#182): 807 |
| `test_cpu_corpus`, after the device record | same, `--test test_cpu_corpus` | 2005 passed, 0 failed |
| `test_corpus_goldens` | | 28 passed (again after the record: 28) |
| `test_cost_model` | | 3 passed |
| `test_golden_format` | | 43 passed (again after the record: 43) |
| `test_ci_coverage` | | 11 passed |
| `test_module_layout` | | 18 passed |
| device-gated targets under rust-only | `test_gpu_corpus`, `test_node_timing`, `peacock_gpu_benchmarks` | each `running 0 tests` |
| ffi rung | `CUDF_ROOT=… LD_LIBRARY_PATH=<ffi out/lib>:<cuDF lib> scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::`; `… -p peacockdb-ffi --test test_ffi` | 4 passed; 3 passed |
| cost-report | `cargo test -p cost-report` | 41 passed |
| warnings | `cargo build --features rust-only -p peacockdb-core` after `touch lib.rs`, then `--all-targets` | 0 and 0. The gpu-feature build's one warning, an unused `AsArray` in `aggregate_dimension_cases.rs:9`, is master's |
| C++ 25.02 | `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build` | rc 0, 28 steps incremental (`scan.cpp` and `test_plan_executor.cpp` among them), 0 warnings; `peacock_cpu_tests` 12 passed; `ctest -L cpu` 1/1 |
| Python, as the cost-report job runs them | `testdata/test_*.py`, `scripts/exec_model/tests/test_*.py` (less the three the job names as run elsewhere), `scripts/calibration/tests/test_*.py`, `scripts/gpu-host/tests/test_*.py`, and the job's `shellcheck` line | all rc 0: duckdb_cost 41, duckdb_result 20; exec-model 212 over ten files; calibration 12 (calls 5, hbm 4, plot 3); gpu-host 104 (check 19, check_actor 7, ci_ship 8, gpu_host 29, gpu_host_test 11, idle_watch 30); shellcheck clean |
| cost gate | `cargo run -q -p cost-report -- --cost-diff --base ENS-distinct-companions` | `868 compared, 7 changed, 2 regression(s), 0 over 10%`; rc 0 |
| device build | `build-test-shadgpu.sh --build` on nebius-gpu | rc 0, no warning in the log |
| device run | `PCK_WRITE_GPU_RESULT=1 gpu-host-test.sh run ~/peacockdb-K` | `ALL rc=0`, no segfault in any log |

This host had neither `matplotlib` nor `shellcheck`, which the job self-heals with `pip`.
`test_plot.py` failed its import before they were installed. Both went into a scratch
`pip install --target` directory outside the tree and were put on `PYTHONPATH` for that run
only: no dependency of the repo changed.

**Device results, per binary.** `peacock_cpu_tests` 12, `peacock_gpu_tests` 2,
`peacock_join_session_tests` 53, `peacock_plan_tests` 81, `peacock_gpu_benchmarks` 8 (3
filtered), `peacockdb_core_gpu_lib` 708, `test_gpu_corpus` 785, `test_node_timing` 1. All passed.
`test_gpu_corpus` is master's 775 plus the ten cells. The gpu lib is one fewer than master's
709: `source_cases` has 11 cases where master had 12. The task's four converted cases,
master's rows-only case and the new `a_limit_over_a_rows_only_scan_keeps_its_first_marker_rows_on_both`
all passed. `ScanRowsOnlyAnswersTheRowCount` passed in `peacock_plan_tests`.

**The ten cells, all kept.** scan-limit: tp1-single, tp1-rowgroup, tp4-single, tp4-rowgroup and
tp4-sized all ok. Its five recorded sections are byte-identical to each other, `lineitem`'s first
ten rows in file order, which is the claim `golden_exact` makes on a `data_fusion_subset` line.
nested-limits: all five ok, 20 rows at every mode, the five sections identical. So the
`__rowmarker__` region scan under its `GpuLimit 0..+23` answers on the device.

**Regenerated, and why each moved.** A section-by-section comparison found nothing outside this
task. Nothing moved out of order: the stop-1 resolution took master's side wholesale.

- `recipe-payloads.txt`: `tpch nested-limits` only. A `GpuLimit` over each scan, the two
  `limit:` lines gone, and a new digest.
- the five `tpch.sf1/*.plans.txt`: scan-limit and nested-limits only. scan-limit is
  byte-identical to the pre-rebase tip `3e306482` at every mode. nested-limits differs from it
  only by master: the `__rowmarker__` schema, `projection=[k@1]`, `CudfJoin`'s recipe, and the
  memory figures that follow.
- the ten `tpch.sf1/*-mini.{cpu,cost}.txt`: the same two queries. tp4-rowgroup's cost file has
  nested-limits only, since scan-limit's cost there does not change. scan-limit's `.cpu.txt`
  sections equal the pre-rebase tip; its `.cost.txt` differs only in master's
  `# GpuLoadParquet, GpuEmpty` comment.
- `tpch.sf1/mini.result.txt`, `duckdb-result.txt`, and every tpcds and pbench golden did not move.
- `gpu-result.txt`, home over `gpu-host.sh rsh` and rsync:
  - tpch gains the ten sections;
  - shuffle-stddev moves in the last digits at tp1-single, tp4-single and tp4-sized (about 1e-15
    relative). That is the device-reduction drift build-test.md describes, as at
    distinct-companions' re-prove;
  - tpcds and pbench are byte-identical;
  - all three files keep `cudf=25.02`.

**The cost delta.**

| section | base | branch | Δ |
|---|--:|--:|--:|
| nested-limits, tp1-single | 1,626,484 | 1,000,118 | −626,366 |
| nested-limits, tp1-rowgroup | 999,884 | 1,000,118 | **+234** |
| nested-limits, tp4-single | 1,626,484 | 1,000,118 | −626,366 |
| nested-limits, tp4-rowgroup | 999,884 | 1,000,118 | **+234** |
| nested-limits, tp4-sized | 1,626,484 | 1,000,118 | −626,366 |
| scan-limit, tp4-single | 1,048,882,882 | 21,476,312 | −1,027,406,570 |
| scan-limit, tp4-sized | 1,048,882,882 | 21,476,312 | −1,027,406,570 |

scan-limit's tp1 sections are new against a `skipped` base and the gate omits them; both read
21,476,312. Its tp4-rowgroup section is unchanged.

**The two regressions are +234 bytes, not the pre-accepted +228.** Both are in the two sections
the chain note names. The 228 is the `GpuLimit 0..+28` over the part scan, 28 Int64 rows, as
before. The other 6 bytes are the new `GpuLimit 0..+23` over the region scan. On master that scan
emits five `__rowmarker__` rows of 6 bytes, where on the old base it declared no column and
emitted 0, so its limit's output now counts. The gate's own rule is 10%, and this is +0.02%.

**Counts, measured, for build-test.md's rows this branch changes.**

| row | master | now |
|---|--:|--:|
| cpu block header | 2819 | 2843 |
| `--lib` | 797 | 807 |
| `test_cpu_corpus` (header) | 1991 | 2005 |
| Corpus, cpu (row: the binary less its registry test) | 1990 | 2004 |
| End to end (`tests::end_to_end`, with its four children) | 37 | 40 |
| Drivers over a mock backend (`executor::driver::tests`) | 110 | 114 |
| Translator, one rule at a time (`planner::translator::tests`) | 47 | 50 |
| gpu block header | 1496 | 1505 |
| `--lib -- gpu_tests::` | 709 | 708 |
| `test_gpu_corpus` (header) | 775 | 785 |
| Corpus, device (row) | 774 | 784 |
| Operator harness (the non-schema `tests::gpu_tests` modules) | 457 | 456 |
| Rust total | 4435 | 4468 |
| grand total | 5002 | 5035 |

The corpus: 180 lines (tpch 41, tpcds 82, pbench 57), unchanged; cpu cells 868 → 870 (tpch
205); device cells 773 → 783 (tpch 176 → 186; tpcds 348 and pbench 249 unchanged); the DuckDB
tier 956 → 966 (180 lines + 783 device cells + 3 checks); the order tier 159 → 161 (tpch 38).
C++ (170) and Python (397) are unchanged by the branch. The rows whose files this branch only
touched without moving a case (null analysis, plan types, recipes per join type, order hazard,
plan-executor 81, and CPU backend executors 80, which with its contract row's 1 is the 81
`executor::cpu_backend::tests` lists) keep their counts.

The VM was soft-stopped after the pull: STOPPED. Deferred, as the chain note says: sf40 and the
benchmarks.

#### Coordinator, after the re-prove

- **All ten device cells stay on**: scan-limit and nested-limits at five modes. #285 is not needed
  on master, so #281's limits half is closed outright.
- **The cost rise is +234 bytes, not the pre-accepted +228.** Both are nested-limits at
  tp1-rowgroup and tp4-rowgroup, the sections the chain note names. The 228 is the `GpuLimit`
  over the part scan, as before. The other 6 bytes are master's: the region scan now emits five
  `__rowmarker__` rows of 6 bytes through its own new `GpuLimit`, where on the old base it
  emitted nothing. Under the 10% gate, so the job is green. The sections are the accepted
  ones, and the extra bytes come from master.
- **build-test.md reconciled** with the measured counts: cpu block 2843 (`--lib` 807,
  `test_cpu_corpus` 2005), gpu block 1505 (`gpu_tests::` 708, `test_gpu_corpus` 785), grand
  total 5035. The device prose has 783 cells and drops scan-limit and nested-limits from
  tpch's exceptions.
