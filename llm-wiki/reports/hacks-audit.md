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
work, are cut; a finding half fixed keeps only its open half. Production bug 1 is folded into
[#229](../tickets/memory.md#t229) and shape problem 4 into [#166](../tickets/df-upgrade.md#t166). The numbers are the original
ones, since tickets cite them. Line numbers are the audit's and have drifted; search by
symbol.

## Shape problems

Nothing below behaves wrongly today. Each one costs a reader who has to decide whether a
rule still holds.

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
