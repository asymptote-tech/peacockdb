# Hacks audit: planning and execution

One reading of `planner/`, `plan/`, `wire/`, `executor/`, `cpp/src/node_session.cpp`,
`cpp/src/expr.cpp`, `cpp/src/operators/` and their tests, at 4f2f3665. It looks for
bandaids, duplicated rules, and tests that assert the wrong thing.

Already-known work is excluded: #175, #173, #183/#187, #198's literal arms, the recipe
walk's refusal sites, `exports.rs`, and the rollout's tickets of the time — #181, #182,
#184, #185, #186, #188, #189, #190, #191, #192, #180. Several of those were rediscovered
here; the scan-limit divergence in particular is #186 and is not repeated.

Ordered worst first, by what it costs a reader who has to trust the code.

**Pruned 2026-09-28, 2026-10-08 and 2026-10-09.** Findings fixed since, or made obsolete by later
work, are cut; a finding half fixed keeps only its open half. The numbers are the original
ones, since tickets cite them. Line numbers are the audit's and have drifted; search by
symbol.

## Production bugs

One thing here behaves wrongly, and it is narrow.

### 1. A sliced batch on the device is priced with no string bytes

Ticketed as [#229](../tickets/memory.md#t229).

`executor/gpu_backend/accumulate.rs:455`. The mid-plan limit slices a straddling batch and
prices the result with

    logical_size_from_schema(&self.schema, rows.length as usize, 0)

The third argument is the var-length content, and it is zero. Every other GPU batch is
priced by `produced()` (`gpu_backend/mod.rs:194`), which passes
`stats.varlen_content_bytes` from the ABI. So one batch in the tree is accounted at its
fixed-width size alone. For a schema with string columns that understates it by the whole
payload.

The comment says the batch "is priced from the rows asked for" and does not say that the
strings are priced at nothing.

What it costs: the accountant's resident total is low for that batch, so a budgeted run
can pass a boundary it should trip on, and `peak_bytes` in a golden is wrong. Holds and
releases still reconcile, because `Held::of` reads the figure once — which is why nothing
notices.

Not reachable in the corpus today: the one mid-plan limit, `tpch/nested-limits`, sits over
`part(p_partkey)`, which is Int64.

Honest fix: read the slice's size the way every other batch is read. The ABI has
`peacock_result_from_handle` for the export but no stats call for a slice, so either
`slice_handle` reports `NodeStats` like `execute_node` does, or the slice is priced by
scaling the input batch's measured var-length bytes by the row ratio, with the
approximation named at the site.

## Shape problems

Nothing below behaves wrongly today. Each one costs a reader who has to decide whether a
rule still holds. §5's join half, §11 and one §12 bullet are under Joins.

### 4. An untested arm for a coalesce that carries a fetch at the root

Folded into [#166](../tickets/df-upgrade.md#t166). `limit_interval` (`planner/translator/common.rs`)
turns a root `CoalesceBatchesExec` carrying a fetch into the unload's interval. **Half fixed
2026-10-09**: the arm said DataFusion 45 never produces that shape, and limits (chain K) measured
that it does — at one target partition `select * from lineitem where l_quantity > 0 limit 10;`
has that coalesce as its root, and the comment is corrected. The open half is the same as before
and worse for being live: nothing reaches the arm, because above one partition that query is
refused ([#284](../tickets/complete-coverage.md#t284)) and at one partition no test asks for it.

### 5 (aggregate half). The executor derives ddof from the aggregate's name

Folded into [#225](../tickets/corpus-coverage.md#t225), whose fix makes the wire change it needs.

`cpp/src/operators/aggregate.cpp:42`: `stddev_ddof` derives the
ddof from the aggregate's *name*, while the Rust side already computed it as data
(`plan/aggregates.rs:17`, `AggSpec.ddof`, carried into `AggStateColumns.ddof`). The wire has
no ddof field, so the executor re-derives from a string what the planner already knows.

Antipattern: a model of what another component does where the answer could be read directly.

Fix: send the ddof on the wire for `execute_aggregate` to read rather than parsing the name.

### 10. Aggregate name decoding in C++ is wider than the writer, and not exhaustive

`make_agg` (`aggregate.cpp:76`) and `make_reduce_agg` (`:118`) accept two casings of every name
and four spellings of mean, and `is_stddev_name`/`is_var_name` (`:27`, `:35`) accept six and
five. The Rust writer emits ten lowercase spellings and no others (`plan/aggregate.rs:99` and
`:115`). Every uppercase arm is dead. Worse, `is_stddev_name` returns `false` rather than throwing
on a spelling it does not know, so a name drift routes a stddev down the plain-aggregate path
instead of failing — the one decode in the file that is not exhaustive.

Fix: narrow the name sets to what the writer emits, and make an unmatched name throw everywhere
rather than in two of four functions.

### 12. Small ones, no fix needed beyond a line

- `driver/scheduler.rs:26` keeps `lane_count` solely for a `debug_assert!`; in a release build
  a bad index writes into the next node's flags. Either the check is worth an `assert!` or the
  field should go. (The doc now says debug builds only.)
- `driver/single_partition.rs:169` has the same shape: `select`'s precondition is a
  `debug_assert!`, and in release a lane that cannot step gets `LaneCall::NoBuild`.
- `wire/writer.rs:117` — `reduce` returns `Result` and never returns `Err`.
- `plan/validate.rs:179` — `declared_width` returns `Ok(())` for any node the registry does
  not know, "a hand-built one under test". Production validation with a hole shaped like a
  test.
- `planner/translator/aggregate.rs:59` reads hash-key ordinals against DataFusion's
  partial-aggregate schema and `nodes.rs:198` applies them to the engine's own intermediate
  schema. The two agree only because a shuffle is always keyed on group keys, which lead both
  schemas; the state columns after them are in different orders by design
  (`decompose`'s comment at `:143` says so). Nothing checks that the keys are below
  `key_columns`.
- `planner/memory_estimation.rs:275` — `key_width` uses `filter_map(... .get(ordinal))`, so
  an out-of-range key ordinal silently contributes zero width instead of failing.

## The tests

### Tests that would not catch the bug they exist for

### Assertions looser than the claim above them

- `memory_estimation.rs:456` — `assert!(model.sources[0].target_batch_bytes > 0)`. The real
  claim is that a source under an estimated-constant refusal is still given
  `MIN_TARGET_BATCH_BYTES` capped by its own size. `> 0` passes on almost any arithmetic bug.
- `memory_estimation.rs:428` — `assert!(model.accumulator_bytes > 0)`. Same shape; the line
  after it does the real work.
- `memory_estimation.rs:524` — `a_target_lands_on_the_coarse_grid` asserts
  `target.is_power_of_two()` and `target >= MIN_TARGET_BATCH_BYTES`. `coarse` floors at
  `MIN_TARGET_BATCH_BYTES`, which is itself a power of two, so both assertions pass if the
  grid rounding is deleted and the floor kept. The first half asserts
  `target_batch_bytes < share_per_source` where its own comment states the value — "each
  target is the source's own size".
- `memory_estimation.rs:513` — `two_sources_get_equal_shares_rather_than_proportional_ones`
  asserts each target is `<= share_per_source`. The claim is `coarse(share / amplification)`
  capped by the source's bytes.
- `memory_estimation.rs:575` — `a_loader_is_priced_by_the_batches_its_mapping_makes` runs two
  batching forms. The budgeted form is the one whose price could come from the target rather
  than the mapping, and it is the one not run.
- `driver/tests/memory.rs:172` — `assert!(report.peak_bytes >= 4096)` where 4096 is exactly
  the script's `build_residency`. The claim is that the build side is resident *at the same
  time as* a probe batch, so the number to assert is `4096 + 80`.
- `driver/tests/memory.rs:125` — `assert!(four.peak_bytes >= one.peak_bytes)`. Four lanes
  hold four batches; `>=` passes on a driver that ran the lanes one at a time.
- `driver/tests/flow.rs:222` — `assert!(count(&report, CallKind::ReleaseUnwanted) > 0)`. The
  probe side produced two batches, so the number is 2.
- `driver/tests/failure.rs:162` and `budget.rs:27` — `holds > 0` and `peak_bytes > 0` are
  guards against a vacuous test rather than claims, and they say so in their messages. Those
  are fine as written.

## What I did not read

Named so the next reader knows the edges.

- `cpp/src/operators/join.cpp` beyond the NULL-equality region (lines 1-80 and 300-536),
  `aggregate.cpp` beyond `execute_aggregate`'s first 200 lines and the name helpers, and
  `expr.cpp`'s column path (`:470-945`) except `build_scalar` and `infer_expr_type`.
  `window.cpp`, `sort.cpp`, `project.cpp`, `filter.cpp`, `union.cpp`, `limit.cpp` not read.
- `cpp/tests/` was grepped, not read. The gtest suites are a large uncovered surface.
- `wire/expr_writer.rs`, `wire/fb_text.rs`, `wire/node_writer.rs` past the scan and filter
  arms, `wire/serialize.rs`, and `wire/tests.rs` (894 lines).
- `plan/mod.rs` past the join and interval sections; `plan/aggregate.rs` and
  `plan/accumulators.rs` sampled only; `plan/tests/` not read.
- `planner/translator/scan_mapping/` — `partition.rs` sampled, `parquet_meta.rs` and
  `rowgroup_prune.rs` not read. The mapping is the one policy every golden depends on and it
  deserves its own pass.
- `cpu_backend/join.rs` past line 140, `cpu_backend/expr_physical.rs`, `merge_m2.rs`,
  `single_node.rs`, `spark_partitioning.rs`.
- The integration tests were grepped for assertion shapes, not read: `test_corpus_goldens.rs`,
  `test_plan_goldens.rs` past `call_shapes`, `test_golden_format.rs`, `test_ci_coverage.rs`,
  `test_cpu_end_to_end.rs` past the limit section, `test_gpu_recipe_walk.rs` past the walk
  itself, and both executor test crates. `test_ci_coverage.rs` in particular is full of
  first-match readers over shell and YAML, which is the antipattern that file is most exposed
  to, and I did not check them.

## What the known bugs are propping up

A second reading of the same tree at 5fed63f2, looking only for code that exists because of a
known defect. Not the defects — #175, #173, #183, #187, #198, #152 — but the branches, fields,
fixtures and goldens that grew around them. Two were given as the pattern: the CPU accumulator's
empty arm at `cpu_backend/accumulate.rs:139` and the device collapse's at
`gpu_backend/accumulate.rs:210`. Neither is repeated below except where it leads somewhere.

Ordered by what a fix has to undo, worst first. Items 4, 8, 9, 10 and 13 are under Joins.

### 1. The driver still drops an empty scatter output under a join that owes nothing

`driver/partitioned.rs` drops a zero-row scatter output unless the node above feeds a join that
owes rows over an empty build (`feeds_owing_build`), which is what closed #175. For every other
consumer the drop stands, for a cost reason (hash skew), and it is one route to a lane with no
batch at all: the arrival #199 (a keyless aggregate answers nothing), #212 and #214 turn on.

It is not the only route — a source whose mapping gives a lane no row groups, or a limit that
releases every batch, leaves a lane with nothing too — so every consumer has to answer that case
anyway, and the drop adds none. What it saves is the calls a zero-row batch would cost down the
chain under skew. So the drop stays, and becomes unconditional: `feeds_owing_build` goes with the
join rewrite (see Joins), once joins answer a missing build side themselves, and #199's fix gives
a keyless aggregate its identity row over a lane that received nothing.

### What I did not read

- `cpp/` beyond `expr.cpp`'s `cudf_ast_can_evaluate`, `build_scalar` and `build_column`, and
  `node_session.cpp` around the collapse. No operator file was read for this pass.
- `driver/accounting.rs`, `driver/scheduler.rs`, `driver/index.rs` and the whole of
  `driver/tests/` except the two files the findings name. An empty lane's effect on the
  accounting is unexamined, and finding 1 turns partly on it.
- `planner/memory_estimation.rs`, `planner/translator/scan_mapping/`, `plan/validate.rs` and
  `plan/tests/`. I checked `validate.rs` for empty-lane rules and found none; I did not read it
  through.
- `tests/test_corpus_goldens.rs`, `test_golden_format.rs`, `test_plan_goldens.rs`,
  `test_ci_coverage.rs`, `test_inc2_conformance.rs`, and the gtest suites under `cpp/tests/`.
- `scripts/exec_model/` past `recipe.py`'s session and `recipe_join.py`'s header. The Python model
  duplicates the capability rules and deserves a pass of its own against `plan/join.rs`.
- The goldens were counted, not read. Whether any golden records a *result* that is a workaround
  rather than an answer is a question this pass did not ask, and it is the one I would ask next.

## Joins

Every join finding from the passes above, gathered here. **All of them are expected to be fixed
by a complete rewrite of the GPU backend's join nodes, with accompanying minor fixes in the CPU
backend. The tickets in [`tickets/joins.md`](../tickets/joins.md) are expected to be fixed in bulk
with that rewrite**, not one at a time. Numbers are the original ones: plain numbers from the
shape problems, "Propping-up" ones from the second pass.

### 5. A Rust model of which joins the C++ hardcodes NULL equality for

`planner/nulls.rs:49`:

    fn hardcodes_null_equality(join_type: JoinType) -> bool {
        matches!(join_type, JoinType::LeftAnti | JoinType::RightAnti | JoinType::LeftMark)
    }

This is a copy of a decision that lives in `cpp/src/operators/join.cpp:135-245`, where
LeftAnti, RightAnti and LeftMark pass `cudf::null_equality::EQUAL` literally while every
other type passes `join_nulls`. The list is right today. Nothing compares them, and the
whole refusal — a user-visible plan-time refusal — rests on the list being right.

Antipattern: a model of what another component does where the answer could be read directly.

Fix: make the flat buffer carry a per-join `null_equality` the C++ reads and the planner
writes, so the refusal becomes unnecessary.

### 11. A finish that answers with nothing where it means to answer with the build side

`gpu_backend/join.rs:207`:

    Some(JoinType::LeftAnti) => {
        return Ok((self.build.take().into_iter().collect(), CallStats::default()));
    }

The doc above says an anti join with no probe keys "hands its build side up, which is the
answer". If `self.build` is `None` — consumed by an earlier call — this returns an empty
vector and calls it the answer. It is unreachable today, because `accumulated.is_empty()`
implies no probe batch ran and so nothing consumed the build. That is a two-step argument
about two other functions, and it is not written down.

Fix: `self.build.take().ok_or_else(...)` with a message saying the build side was already
consumed. One line, and the invariant stops being an argument. Part of
[#173](../tickets/joins.md#t173)'s fix, which also applies LeftAnti's projection here.

### 12 (join bullet). A panic where the function already returns an error

- `wire/join.rs:34` — `node.capability().expect(...)` where `?` would do, in a function that
  already returns `Result<_, PlanError>`.

### Propping-up 4. The CPU backend refuses a join DataFusion can answer

`cpu_backend/join.rs:184`:

    pub fn without_build(self) -> Result<(), BackendError> {
        if self.calls.empty_build_answers_nothing {
            return Ok(());
        }
        Err(BackendError::new(
            "this lane's build side is empty, and what this join owes is its probe side — \
             which takes a call over a build table that does not exist (#212)",
        ))
    }

Now [#212](../tickets/joins.md#t212)'s. Nothing on this backend cannot make that answer. The per-call path is a `HashJoinExec`, and a
Right outer over an empty build side is a shape DataFusion answers every day. The refusal is here
so the oracle does not answer a query the device refuses — which the ticket states plainly and
which is a real reason. It is still scaffolding, and it is the largest piece: the `Calls` struct
carries a `empty_build_answers_nothing` field (`:43`) set at three construction sites (`:79`,
`:99`, `:163`) for no other purpose.

At close: the field, its three initializers, the refusal arm and the message go, and
`without_build` becomes `Ok(())` or disappears. The device half at `gpu_backend/join.rs:103`
follows the same shape and goes with it. `empty_build_answers_nothing` in `plan/join.rs:452`
survives — for the six types that owe nothing it is a genuine shortcut, not a workaround.

### Propping-up 8. A mock knob that exists to fake a refusal

`driver/mock.rs:90`, `JoinRule.empty_build_owes_its_probe`, read once at `:521` to return an error
whose text is a paraphrase of the production one. The mock join has no join type, so the script
supplies the verdict directly.

The field, its default at `:149` and the arm at `:521` are now [#212](../tickets/joins.md#t212)'s.
They go when it closes.

### Propping-up 9. Two functions in the device join whose whole body is a refusal

`gpu_backend/join.rs:293` and `:304`:

    fn copy_of(&self, _batch: &mut Option<GpuBatch>) -> Result<u64, BackendError> {
        Err(BackendError::new(
            "this join's recipe copies its probe batch — ... the ABI has no copy ... (#152)",
        ))
    }

`copy_of` takes a `&mut Option<GpuBatch>` it never touches — the underscore is the tell — and
exists only so the `Input::BatchCopy` arm at `:251` has somewhere to go. `build_copy` is worse
than a refusal: it hands the first probe batch the original handle and refuses the second, so a
single-batch probe silently succeeds at a call that asked for a copy. That is why the build-side
semi family runs on a device at all today.

Both are #152. At close, `copy_of` becomes one refcount call and `build_copy` becomes the same
call with no special case for the first batch — and the "first one gets the original" trick has
to be deleted deliberately, because leaving it is a handle released once and used twice.

### Propping-up 10. The recipe walk models an unavailable copy as the same handle

`wire/gpu_tests/mod.rs` (`resolve`; the walk moved from `tests/test_gpu_recipe_walk.rs`):

    match input {
        Input::Batch | Input::BatchCopy => held(at.batch),
        Input::BuildSide | Input::BuildSideCopy => held(at.build),

A copy and the thing copied resolve to one handle. The doc at `:206` says why: every shape the
walk drives plans one probe batch, so the handle is used once. Two more pieces hold that up — an
assert at `:441` that each lane's probe is exactly one batch, and a refusal table at `:770` where
`CrossJoin` and `NestedLoopJoin` are `Refused("both copy their build side, and the ABI has no
copy (#152)")`.

The walk is the only thing in the tree that drives a whole plan against a real device. Its
coverage is bounded by the bug, and the bound is written in three places rather than one.

When #152 closes: `resolve` must stop conflating the two inputs or it will double-consume a
handle and report a working recipe for a plan that cannot run. The one-probe-batch assert and two
arms of the refusal table go. This is the one item here where leaving the scaffolding in place is
actively unsafe rather than merely confusing.

Less certain, stated both ways: the walk plans everything at `OneBatchPerLane`
(the walk's header). The header gives a good reason — no driver, so the recipe's call
order has to be the schedule. But it is also exactly the sizing at which #152 never fires, and
the two reasons are indistinguishable from the file. If the first reason is the real one, the
knob stays after the fix; if the second is, the walk should gain a multi-batch mode with it.

### Propping-up 13. Goldens and a cost model that encode the missing copy

Two records of #152 that a fix has to decide about rather than discover:

- The plan goldens spell the input names. `build copy` appears 3,639 times and `batch copy` 69
  across `testdata/goldens/*/*.plans.txt` and `recipe-payloads.txt`. Whether those move depends on
  the fix: if refcounted handles keep `BuildSide` and `BuildSideCopy` as distinct recipe inputs —
  and they probably should, since the last probe batch can still hand over — nothing moves. If the
  two collapse into one, every one of those lines does. Worth knowing before the branch starts,
  not after.
- `scripts/exec_model/operators/recipe.py:392`, `copy_handle`, whose docstring opens "**Not on the
  frozen surface.**" It tallies `copies` and `copied_bytes` per reason (`:363`), and
  `tests/test_join_capability.py:439` asserts the counts. That apparatus exists to quantify the
  ticket. It is the evidence for #152 rather than a workaround of it, which is a good reason to
  keep it until the ticket closes and none to keep it after.
