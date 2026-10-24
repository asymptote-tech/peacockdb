# Hacks audit: planning and execution

One reading of `planner/`, `plan/`, `wire/`, `executor/`, `cpp/src/node_session.cpp`,
`cpp/src/expr.cpp`, `cpp/src/operators/` and their tests, at 4f2f3665. It looks for
bandaids, duplicated rules, and tests that assert the wrong thing.

Already-known work is excluded: #175, #173, #183/#187, #198's literal arms, the recipe
walk's refusal sites, `exports.rs`, and the rollout's tickets of the time — #181, #182,
#184, #185, #186, #188, #189, #190, #191, #192, #180. Several of those were rediscovered
here; the scan-limit divergence in particular is #186 and is not repeated.

Ordered worst first, by what it costs a reader who has to trust the code.

**Pruned 2026-09-28, 2026-10-08, 2026-10-09 and 2026-10-23.** Findings fixed since, or made obsolete by later
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
rule still holds.

### 4. An untested arm for a coalesce that carries a fetch at the root

Folded into [#166](../tickets/df-upgrade.md#t166). `limit_interval` (`planner/translator/common.rs`)
turns a root `CoalesceBatchesExec` carrying a fetch into the unload's interval. **Half fixed
2026-10-09**: the arm said DataFusion 45 never produces that shape, and limits (chain K) measured
that it does — at one target partition `select * from lineitem where l_quantity > 0 limit 10;`
has that coalesce as its root, and the comment is corrected. The open half is the same as before
and worse for being live: nothing reaches the arm, because above one partition that query is
refused ([#284](../tickets/complete-coverage.md#t284)) and at one partition no test asks for it.

### 12. Small ones, no fix needed beyond a line

- `driver/scheduler.rs:26` keeps `lane_count` solely for a `debug_assert!`; in a release build
  a bad index writes into the next node's flags. Either the check is worth an `assert!` or the
  field should go. (The doc now says debug builds only.)
- `driver/single_partition.rs:183` has the same shape: `select`'s precondition is a
  `debug_assert!`, and in release a lane that cannot step is handed a call anyway.
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
## The tests

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
fixtures and goldens that grew around them. Every finding it made is resolved: join-backend
removed the join ones, and keyless-identity closed #199, the open half of the first. What it did
not read stays below.

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
