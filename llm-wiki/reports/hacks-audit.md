# Hacks audit: planning and execution

One reading of `planner/`, `plan/`, `wire/`, `executor/`, `cpp/src/node_session.cpp`,
`cpp/src/expr.cpp`, `cpp/src/operators/` and their tests, at 4f2f3665. It looks for
bandaids, duplicated rules, and tests that assert the wrong thing.

Already-known work is excluded: #175, #173, #183/#187, #198's literal arms, the recipe
walk's refusal sites, `exports.rs`, and everything in `tasks/active-tickets.md` —
#181, #182, #184, #185, #186, #188, #189, #190, #191, #192, #180. Several of those were
rediscovered here; the scan-limit divergence in particular is #186 and #188 and is not
repeated.

Ordered worst first, by what it costs a reader who has to trust the code.

## Production bugs

Only two things here behave wrongly, and both are narrow.

### 1. A sliced batch on the device is priced with no string bytes

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

### 2. `Some(0)` and `None` are the same scan limit on the wire

`wire/node_writer.rs:93` writes `limit: node.limit.unwrap_or(0)`, and `cpp/src/operators/
scan.cpp` reads `if (scan->limit() > 0)`. A pushed-down `LIMIT 0` and no limit at all are
one value.

This is inside #186/#188's shape and not a separate ticket, but neither ticket says it, and
whoever fixes those two will touch this line. The field wants to be nullable on the wire,
or the C++ wants a separate "has a limit" bit.

## Shape problems

Nothing below behaves wrongly today. Each one costs a reader who has to decide whether a
rule still holds.

### 3. The Rust child-walk is a whitelist where the C++ twin is total

`wire/read.rs:34` and `cpp/src/node_session.cpp:128` walk the same flat buffer in the same
order, and a seq means the same node only if they agree. The C++ names all fifteen kinds
and ends with

    default: throw std::runtime_error("node_children: unsupported PlanNodeKind: " ...)

The Rust ends with `_ => Vec::new()`, which makes an unlisted kind a leaf. Today the Rust
arm lists fourteen and `CudfScan` is genuinely a leaf, so the two agree by coincidence of
completeness rather than by construction.

Antipattern: a whitelist where a total function belongs.

Cost: add a node kind to `gpu_plan.fbs` with an `input` and forget this file, and the Rust
side renumbers every seq above it. `check_seq_kinds` reads the same walk, so it cannot
catch that; it would compare a wrong node against a wrong claim and often agree. The next
thing a reader sees is a query answered by the wrong kernel.

Fix: match every `PlanNodeKind` arm explicitly, `CudfScan` included, and make the fallthrough
a `PlanError` naming the kind — the same shape the C++ already has.

### 4. Two answers for a coalesce that carries a fetch, and one says it is unreachable

`planner/translator/nodes.rs:85` handles `CoalesceBatchesExec` with a fetch and says:

    // A fetch does not: DataFusion's limit pushdown parks a limit here, and
    // dropping the node with it would answer a count over three rows with the
    // count of all of them.

`planner/translator/common.rs:137`, inside `limit_interval`, handles the same DataFusion
node and says the opposite:

    // Not reachable from today's planner: where a limit is root-adjacent DataFusion leaves
    // a GlobalLimitExec there and uses a coalesce's fetch only as the bound pushed below
    // it. The arm is here so both paths agree if that ever changes ...

Both build `RowInterval { skip: 0, fetch }` from `coalesce.fetch()`. One arm is live and one
is dead, and the two comments cannot both be true of the same DataFusion version.

Antipatterns: a rule duplicated in two places with nothing enforcing it, plus a branch whose
only stated reason is a hypothetical — which `coding-style.md` forbids as defensive code for
an impossible scenario.

Cost is not the duplication, it is the landmine. `limit_interval` is called on the root
(`translator/mod.rs:85`), so if a `CoalesceBatchesExec` with a fetch ever does reach the
root, the arm silently drops the node and hands its fetch to the unload. A coalesce's fetch
that is a batch-size hint rather than a limit would become a `LIMIT`.

Fix: delete the arm in `limit_interval`, and let `node()`'s arm be the one statement of the
rule. If it genuinely has to stay, it wants a test that reaches it.

### 5. Two models of what the C++ join does, both written in Rust

`planner/nulls.rs:49`:

    fn hardcodes_null_equality(join_type: JoinType) -> bool {
        matches!(join_type, JoinType::LeftAnti | JoinType::RightAnti | JoinType::LeftMark)
    }

This is a copy of a decision that lives in `cpp/src/operators/join.cpp:135-245`, where
LeftAnti, RightAnti and LeftMark pass `cudf::null_equality::EQUAL` literally while every
other type passes `join_nulls`. The list is right today. Nothing compares them, and the
whole refusal — a user-visible plan-time refusal — rests on the list being right.

The same shape, smaller, at `cpp/src/operators/aggregate.cpp:42`: `stddev_ddof` derives the
ddof from the aggregate's *name*, while the Rust side already computed it as data
(`plan/aggregates.rs:17`, `AggSpec.ddof`, carried into `AggStateColumns.ddof`). The wire
carries the number and the executor re-derives it from a string.

Antipattern: a model of what another component does where the answer could be read directly.

Fix, in order of cost: make the flat buffer carry a per-join `null_equality` the C++ reads
and the planner writes, so the refusal becomes unnecessary; and have `execute_aggregate`
read the ddof the plan sent rather than parsing the name.

### 6. The validator claims a check it does not make

`plan/validate.rs:214`:

    // The rest emit their input's columns, which `types_across_the_edge` compares
    // field for field — a stronger statement than a count.
    _ => return Ok(()),

`types_across_the_edge` (`:255`) zips the two field lists:

    ours.fields.fields().iter().zip(theirs.fields.fields().iter()).find(...)

`zip` stops at the shorter list. It is therefore *weaker* than a count in exactly the
dimension the comment names: a node declaring three columns over a five-column input passes
both checks. The kinds affected are the seven `declared_width` hands off — Sort,
CoalesceAllBatches, AccumulateBatchesAndSort, Limit, MergePartitions, EmitPartitions,
MergeSortedPartitions.

Unreachable from the constructors, because those nodes derive their schema from their input.
But `validate` is documented as existing for trees a test rewrites into shapes no planner
emits, and that is the caller the gap is open to.

Fix: compare the two lengths in `types_across_the_edge` before zipping, and correct the
comment. One test: a hand-built sort declaring one column over a two-column source.

### 7. The mock keeps its own copy of the clamp the limit tests are about

`executor/driver/mock.rs:558`:

    let start = (rows.offset as usize).min(batch.rows);
    let taken = if rows.length == u64::MAX { batch.rows - start }
                else { (rows.length as usize).min(batch.rows - start) };

That is `RowRange::clamp` (`executor/row_range.rs:12`) written a third time — #174 already
records two, in Rust and C++, and this is the one the driver's limit tests run against.
Every row count in `driver/tests/limit.rs` is a fact about this private clamp, not about the
clamp the engine ships.

Antipattern: a test asserting mock behaviour, and a rule duplicated with nothing enforcing
it.

Fix: `let (offset, length) = rows.clamp(batch.rows as u64);`. It is a one-line change and it
makes those tests cover the production rule. Worth doing inside #174 rather than separately.

### 8. A dead accessor whose doc says the driver reads it

`cpu_backend/accumulate.rs:398` and `gpu_backend/accumulate.rs:410`:

    /// How many rows have gone past, which is what a driver reads to know the interval is
    /// satisfied and the pulls can stop.
    pub fn seen(&self) -> u64 { self.seen }

Nothing calls either one. The driver keeps its own count in
`driver/partitioned.rs:50` (`rows_seen`) and settles the limit from that
(`settle_limit`, `:559`). So a mid-plan limit's stream is counted twice — once by the driver
to decide the early exit, once by `LimitStream` to decide the slice — and the two must agree
with nothing comparing them.

The doc is false about which of the two is read, which is the worst version of this: a
reader repairing the counting would fix the one that is not used.

Fix: delete both accessors and the doc, or make the driver read them and delete `rows_seen`
for limit nodes. Either ends the double count; leaving the accessor is what makes the
duplication invisible.

### 9. A parameter kept alive by a discard

`planner/memory_estimation.rs:311`:

    fn batch_bytes(&self, targets: &HashMap<usize, u64>) -> Vec<u64> {
        let _ = targets;

The doc above says "a source emits its target". It does not: the source arm reads
`load.largest_batch_bytes()`. The `HashMap` the caller builds at `:70` exists only to be
passed here and thrown away.

This is #134's shape exactly — real, cosmetic, nothing behaves wrongly — so no ticket. But
`let _ =` is a bandaid for a warning, and it is what makes a dead parameter survive review.
Delete the parameter and the map, and correct the doc's first clause.

### 10. A refusal in C++ that no writer can provoke, with a comment explaining the wrong reason

`cpp/src/operators/aggregate.cpp:148` throws when an `AggregateFuncNode` carries
`distinct()`, and the comment says it is "Unreachable today" because DataFusion rewrites a
standalone `count(DISTINCT x)`.

Two layers above it make it unreachable more simply, and neither is named:
`planner/translator/aggregate.rs:119` refuses `aggregate.is_distinct()` at plan time, and
`wire/aggregate_writer.rs:177` writes `distinct: false` unconditionally. No test in
`cpp/tests/` sets the flag either. The field cannot be true on this wire at all.

Beside it, `make_agg` (`:76`) and `make_reduce_agg` (`:118`) accept two casings of every
name and four spellings of mean, and `is_stddev_name`/`is_var_name` (`:27`, `:35`) accept
six and five. The Rust writer emits ten lowercase spellings and no others
(`plan/aggregate.rs:99` and `:115`). Every uppercase arm is dead. Worse, `is_stddev_name`
returns `false` rather than throwing on a spelling it does not know, so a name drift routes
a stddev down the plain-aggregate path instead of failing — the one decode in the file that
is not exhaustive.

Fix: drop the `distinct` guard with the field, or keep the guard and correct the comment to
name the two Rust sites. Narrow the name sets to what the writer emits, and make an
unmatched name throw everywhere rather than in two of four functions.

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
consumed. One line, and the invariant stops being an argument.

### 12. Small ones, no fix needed beyond a line

- `driver/scheduler.rs:26` keeps `lane_count` solely for the `debug_assert!` at `:91`, whose
  doc says a bad index is "caught here rather than silently writing into the next node's
  flags". In a release build it is not caught and it does write there. Either the check is
  worth an `assert!` or the field and the doc should go.
- `driver/single_partition.rs:169` has the same shape: `select`'s precondition is a
  `debug_assert!`, and in release a lane that cannot step gets `LaneCall::NoBuild`.
- `driver/partitioned.rs:50` documents `rows_seen` as "rows of its input stream seen so far"
  for every node. It only advances for nodes that carry an interval; for every other node it
  is permanently zero.
- `wire/writer.rs:117` — `reduce` returns `Result` and never returns `Err`.
- `wire/join.rs:34` — `node.capability().expect(...)` where `?` would do, in a function that
  already returns `Result<_, PlanError>`.
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

**`driver/tests/flow.rs:229` — `a_join_that_owes_its_probe_side_without_a_build_side_is_refused`.**
The doc says "The three types that preserve unmatched probe rows owe their probe side". No
join type appears in the test. It sets `JoinRule { empty_build_owes_its_probe: true }` and
asserts the mock's own `without_build` error comes back as `RunError::CallFailed`. Change
`empty_build_answers_nothing`'s table in `plan/join.rs:447` to any wrong answer, or change
either backend's `without_build`, and this test still passes. The only production change
that reddens it is one in the driver that swallows a `NoBuild` failure. The name should say
that, or the test should drive a real backend.

**`driver/tests/limit.rs` as a whole.** Every row count in it is a fact about
`MockUnload::unload`'s private clamp (finding 7). Replace `RowRange::clamp` with a wrong
implementation and nothing in this file moves.

**The mid-plan limit's trimming is uncovered by the driver.**
`a_satisfied_limit_reports_done_so_the_node_above_it_can_finish` (`limit.rs:174`) asserts 20
rows where the interval asks for 12, and says so: "the mock's limit forwards whole batches".
That is honest, but it means no driver test sees a mid-plan interval applied at all. The
trimming is covered only in the two backends' own accumulate tests.

**`GpuLoadParquet.limit` is covered on neither backend.**
`planner/translator/tests.rs:603` proves the plan carries `limit: Some(3)` and one lane.
`CpuSource` (`cpu_backend/source.rs`) never reads the field and `cpp/src/operators/scan.cpp`
applies it per call. Both halves are #186 and #188; the test gap is that the only assertion
anywhere is about the plan text. A test that runs `SELECT * FROM lineitem LIMIT 10` on the
CPU backend and counts rows is what should have caught it, and it is what should land with
the fix.

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
- `memory_estimation.rs:575` — `a_loader_is_priced_by_the_batches_its_mapping_makes` says
  "Three batching forms, one budget" and compares two. The budgeted form is the one whose
  price could come from the target rather than the mapping, and it is the one not run.
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
