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
turns a root `CoalesceBatchesExec` carrying a fetch into the unload's interval, and says DataFusion
45 never produces that shape; `nodes.rs` handles the same node mid-plan, which is live. The two
comments are about different positions, so they do not contradict. The arm is right if it is ever
reached — a coalesce's `fetch` is always a row limit, the batch size being the separate
`target_batch_size` — so it stays as deliberate defence. What it lacks is a test that reaches it.

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

**Resolved by join-backend.** The drop is unconditional: `feeds_owing_build` and its exception
are gone, and a join answers a lane with no build batch through `set_build(None)`. #199's half,
a keyless aggregate's identity row over a lane that received nothing, is still open.

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

Every join finding from the passes above, gathered here. **All are resolved by join-backend**,
which put both engines' joins on the session. Numbers are the original ones, since tickets cite
them: plain numbers from the shape problems, "Propping-up" ones from the second pass.

- **5.** The Rust copy of the C++'s hardcoded `EQUAL` list is gone with `planner/nulls.rs`. Every
  join type honours `null_equals_null` on both engines.
- **11.** `finish_without_keys` is gone. A finish over no probe batch is the session's ordinary
  finish.
- **12 (join bullet).** `wire/join.rs` reads no capability, so the `expect` is gone.
- **Propping-up 4.** `without_build` and the cpu's `Calls` are gone. A lane with no build batch
  calls `set_build(None)` on both engines (#212).
- **Propping-up 8.** The mock's `empty_build_owes_its_probe` is gone. The mock reads its type.
- **Propping-up 9.** `copy_of` and `build_copy` are gone. A session keeps its build table, so no
  handle is copied (#152).
- **Propping-up 10.** The recipe walk has no copy inputs left to conflate, no one-probe-batch
  assert and no join refusals. It still plans at `OneBatchPerLane`, for the reason its header
  gives.
- **Propping-up 13.** No golden spells `build copy` or `batch copy`, and exec_model's
  `copy_handle` is gone with `recipe.py`.
