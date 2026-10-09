# join-backend — run record

Chain J task 8. Branch `ENS-join-backend`, forked from `ENS-join-session-cpp` at `3d9a122e`.
PR targets `ENS-join-session-cpp`. Spec [`join-backend.md`](join-backend.md) (frozen); plan
[`join-backend-impl.md`](join-backend-impl.md) (16 tasks, run order 1, 2, 3, 5, 6, 7, 8, 4, 9,
9b, 9c, 10–16); design [`join-rewrite-design.md`](join-rewrite-design.md) §1.3, §3.5, §4, §5.7.

## State

| | |
|---|---|
| state | `building` since 2026-10-09 |
| developer rounds | 1 dispatched 2026-10-09 |
| review rounds | none |
| tickets filed | none. Next free is **267**; master reserved 264–279 for this chain |

## Hosts, measured at dispatch (2026-10-09)

- **nebius-gpu `dmitry@89.169.109.150`** — up, L40S, 46068 MiB VRAM with 0 MiB in use, sf1
  tpch/tpcds/pbench present in `~/peacockdb-J/testdata`. **Disk is the risk: 21 GB free of 96 GB.**
  `~/peacockdb-J` is 23 G, `~/miniforge3` 17 G, `~/peacockdb-K` 16 G (chain K's worktree — never
  delete it), `~/peacockdb` 991 M (already cleaned). A device build that runs out of disk reads as
  a compiler error; check `df -h ~` first and clean only stale build/target dirs inside
  `~/peacockdb-J`, per the board's host override.
- **shad-gpu** — down since 2026-10-08. The `gpu-tests` CI job is exempt by the override.
- **verda** — unlocatable: `VERDA_CLIENT_ID` is unset, so `scripts/list_verda_instances.sh`
  exits on it and `ssh verda` resolves nowhere. **Every CPU build and run is local.**
- Local `cpp/build` is an empty **root-owned** directory and cmake dies at configure blaming
  itself. `/tmp/dkb-cppbuild` is already configured against this worktree and is the 30-second
  local compile check before paying for a device cycle. Clearing `cpp/build` needs `sudo` and is
  the human's.

## What task 8 inherits from join-session-cpp, each already paid for once

- **`wire/fb_text.rs`'s `payload_text` ends in `_ => {}`**, so a `CudfJoin` renders no fields into
  `recipe-payloads.txt`. Task 7 left the arm empty deliberately: with no writer and no golden,
  which of the nine fields to print was a guess. Task 8's writer makes the arm reachable and will
  meet an empty payload golden.
- **An fbs change is a Rust change.** `peacockdb-core/build.rs` regenerates the bindings every
  build, so the rust-only tier belongs in re-proving any `gpu_plan.fbs` edit whatever the spec's
  restriction says about Rust behaviour. `cargo check -p peacockdb-ffi` compiles and runs no test;
  task 7 reported green off it and `wire::tests::the_child_walk_names_every_node_kind` was red.
- **`chunk_bytes` is priced at `pairs × (8 + the residual's per-row bytes)`**, not §4.3's flat 8
  bytes a pair. The fbs comment carries the real formula. §4.3 is this task's to reconcile; a
  planner converting its scratch budget per §4.3 under-prices by the residual's width.
- **`peacock::JoinRefusal` is the "session stands" marker**, in `plan_executor.h`, thrown at
  exactly four sites and caught in `join_call`. Do not widen that catch: `cudf::data_type_error`
  derives from `std::invalid_argument` on 25.02 and 26.02 both, and `join_probe` consumes the
  handle before the session runs, so a shared type lets a failure mid-work read as a refusal that
  started nothing — a wrong answer, not a failed query.
- **An absent build's output schema comes from `build_schema`'s field names and order**, pinned by
  `AnAbsentBuildEmitsTheSchemaABatchWouldHave` against a rows-present run. The driver concatenates
  lane outputs, so a disagreement shows up as an Arrow schema mismatch on empty lanes alone.

## Rules this chain paid for, and the one that costs a whole round

- **A green measured against a mutated tree is worth nothing.** Where a round proves a bound or a
  fold red by deliberately breaking the code, the break must be reverted before the commit. Twice
  in task 7 a mutation was still in the tree at commit time, and it is invisible in a report that
  says "proved red". Before each commit, read `git diff <base> -- <file>` hunk by hunk.
- **`git-clang-format --diff` cannot confirm a reflow was reverted** — the reflowed form is the
  output it prefers, so a clean verdict is what you get either way.
- **Restrict formatting to the lines the task changed.** Both join `.cpp` files were already
  non-conformant on the parent; a tree-wide sweep rides in a commit of its own.
- **`coding-style.md` caps a commit message at 10 lines including the subject.** With the mandated
  attribution line that leaves six body lines.
- **Derive counts, never add deltas.** `build-test.md`'s header is the sum of its two tables' N
  columns; 3085 and C++ 169 reconcile as of task 7.
- **The CI `changes` job classifies the push it reacts to, not the PR's diff.** A documentation
  push skips every job and reports success. Before writing `done`: find the last commit touching
  anything but `**.md` and `llm-wiki/**`, and confirm a run built it or later.

## Deferred by the board's host override, not blocking

- the sf40 binaries `peacock_tpch_tests` and `peacock_tpchv_tests`;
- `--run-benchmarks`, Nsight captures, any H200 timing;
- cuDF 26.02 verification — task 9 `verify-26.02` owns it.

## Cost gate

The board accepts in advance any cost regression this task brings, and the spec's risk 1 asks for
the record. **Plan task 3 is where it came from and its numbers are under that round below**: 228
regressions, 173 past the 10% gate, driven by #137's `IS NOT NULL` filters. Re-run
`cargo run -q -p cost-report -- --cost-diff --base <base sha>` once before the PR against the
branch point, so the PR quotes one figure for the whole task rather than one per round.

## Round log

Each impl-plan task is one commit on `ENS-join-backend`, in plan run order, so `git log
--oneline ENS-join-session-cpp..` is the task-by-task record and no SHA needs copying here.

One developer is resumed across the whole task rather than redispatched per plan task: a
`SendMessage` keeps its context and costs a fraction of a fresh dispatch. It returns after each
impl-plan task with its verification evidence, the coordinator commits that task, and resumes it
on the next. That cadence is what makes "every commit green" checkable — a developer that ran
four plan tasks before reporting leaves one commit nobody can bisect.

### Round 1 — dispatched 2026-10-09

Impl-plan tasks 1, 2, 3, 5 — the additive planner pieces, each green on today's executors —
reporting after each for its own commit. Scoped before any executor change so the first review
round can read the planner alone; tasks 6, 7 and 8 move both backends at once.

#### Round 1, plan task 1 — one footer reader, and nullability on both trees

Rust only; no device cycle and no fbs change, so the dispatch's fbs trap did not apply.

**What shipped.** `planner/nulls.rs` → `planner/nullability.rs` (`mv`, unstaged — the whole
rest of the file is byte-identical to the parent, so the only diff is the header, the imports
and the new block). New `planner/parquet_nulls.rs`: `nulls_possible(Option<&Statistics>)`,
the one "no statistic is not a promise of no nulls" rule, and `column_may_hold_null(path,
column)` over every row group. `parquet_meta.rs` now calls `nulls_possible` instead of
spelling the rule a second time. New `logical_can_be_null` + `join_can_be_null` +
`scan_column_may_hold_null` in `nullability.rs`. New `planner/tests/logical_nullability.rs`,
five cases. `url = "2"` in `peacockdb-core/Cargo.toml` (already pinned in `Cargo.lock`, so
the lock moves by one line).

**The plan's Join arm was wrong and its own test says so.** The sketch at
`join-backend-impl.md` step 4 matched `LogicalPlan::Join(j) if j.join_type == JoinType::Inner`
and let everything else fall to `_ => true`. Its own
`an_outer_join_pads_and_so_nullable` asserts `!logical_can_be_null(c_custkey)` over a LEFT
JOIN — the *preserved* side — which `_ => true` denies. Proved: with the arm restricted to
Inner (`JoinType::Left => false` in the preserved match) that one assertion is the only
failure. Shipped as `join_can_be_null`: the padded side is nullable whatever it holds, the
preserved side answers from its own input, and a column name present on **both** sides
returns `true` rather than picking the left one — an unqualified `Column` matches by name
only (`DFSchema::index_of_column_by_name`), so guessing there is the false-negative shape the
module's doc calls a wrong answer. The plan's sketch is corrected in place.

**A fifth test, not in the plan.** `a_key_whose_row_groups_hold_a_null_is_nullable`, over
tpcds `store_sales.ss_sold_date_sk` (129,850 NULLs at sf1 — the design §4.1 figure, measured
and confirmed). The plan's four all assert over tpch, whose keys hold no NULLs, so a
`nulls_possible` hardwired to `false` passes all four: without this case the footer reader's
true direction is unproven. Proved red by that exact mutation — it was the only failure.

**Red-green evidence, each mutation reverted and the revert read off `git diff`.**
- Red before: `cargo test -p peacockdb-core --lib --features rust-only logical_nullability`
  → `unresolved import crate::planner::nullability`.
- Mutation A, `table_url.prefix()` in place of `Url::to_file_path` (the trap the plan names):
  3 failed, 2 passed — the file never opens, `unwrap_or(true)` answers nullable.
- Mutation B, `nulls_possible` → `false`: 1 failed (the tpcds case), 4 passed.
- Mutation C, Left preserves nothing: 1 failed (`an_outer_join_pads_and_so_nullable`), 4 passed.
- Green after: 5 passed.

**Two things found, neither blocking.**
- `parquet_meta/tests.rs`'s `a_column_can_be_null_only_where_a_surviving_row_group_holds_one`
  passes with `nulls_possible` hardwired to `false`: tpch.minimal holds no NULLs anywhere, as
  its own comment says, so it can only ever go red in one direction. Pre-existing and
  deliberate; the other direction is now covered by the tpcds case above.
- The installed rustfmt disagrees with the committed `nulls.rs` at three pre-existing sites
  (the `meeting` closure, `NestedLoopJoinType::Left`, `Expr::Case`). Left alone per the
  chain's "restrict formatting to the lines the task changed" rule, so
  `rustfmt --check nullability.rs` still reports three hunks; none is on a line this round
  touched. Only the two hunks on new lines were applied, by hand.

**`#[cfg_attr(not(test), allow(dead_code))]` on `logical_can_be_null`.** Task 1 is additive
and its production caller is task 2's `NOT IN` rule, so the lib build warns four times
without it (the attribute covers the two private helpers and `column_may_hold_null` too,
since they are reachable from it). The sanctioned form per `coding-style.md`; it leaves with
that rule.

**build-test.md needs a row (human's file).** `planner::tests::logical_nullability` is a new
component-tier block of 5 cases, and `--lib` moves 696 → 701. That makes the cpu block
1708 → 1713, the Rust total 2515 → 2520 and the grand total 3085 → 3090. Suggested row, to
sit after "Null analysis rules":

    | Logical-plan nullability | [a_key_whose_row_groups_hold_no_null_is_not_nullable](../peacockdb-core/src/planner/tests/logical_nullability.rs) | 5 |

    whether a column of a DataFusion logical plan can hold a NULL, traced to the parquet
    footers — what the `NOT IN` rewrite and #137's key filters ask before a physical plan
    exists. Against real files, since every corpus column is declared nullable; the tpcds
    case is the only one whose column really holds NULLs

#### Round 1, plan task 2 — the `NOT IN` rule (#80)

Rust only; no device cycle. Every expected row was re-measured against DuckDB 1.5.4 here
rather than taken from the plan, and **all twelve of the plan's expectations match** — the
scratch-probe numbers (correlated 3, uncorrelated 2) included.

**What shipped.** New `planner/not_in.rs`: `impl OptimizerRule for NullAwareNotIn`, plus
`negation_normal_form`, `spine`, `rewrite_not_in`, `not_exists`, `count_of`, `to_outer_refs`.
`planner/mod.rs` declares `mod not_in;` and the `NullAwareNotIn` unit struct (the component
declares its own type; the implementation module implements the trait for it).
`planner/nullability.rs` gains `in_subquery_may_meet_null` and
`refuse_nullable_in_off_the_spine`, and **loses `logical_can_be_null`'s
`#[cfg_attr(not(test), allow(dead_code))]`** — the rule is the production caller task 1's
comment promised, so the attribute left with it, as it said it would. `lib.rs`'s
`build_session_state` inserts the rule immediately before `decorrelate_predicate_subquery` in
`Optimizer::new().rules`. New `planner/tests/not_in.rs`, sixteen cases.

**Goldens moved and were regenerated in this round.** Five pbench `*.plans.txt`, three
sections each, every one a refusal line:
- `in-is-null`: was `refused: unsupported: plan node EmptyExec` (#155 via #257's fold), now the
  #250 refusal from our rule. **This is #250 becoming demonstrable.** Its ticket says it
  "cannot be demonstrated until #257 is fixed" — wrong now, and in the good direction: our rule
  refuses in the first optimizer pass, before the mark join exists, so the `IsNull`-over-a-
  non-nullable-mark fold never happens.
- `not-in-correlated`: was the nulls.rs physical refusal (#59/#80), now
  `RightAnti join with a residual filter` (#159) — the next refusal down, lifted in task 9.
- `not-in-uncorrelated`: was the same, now `nested-loop join type Right` (#160) — likewise.
- `not-in-under-or`, `not-exists-null-keys`, `anti-null-preserved-condition`,
  `exists-or-mark`, `mark-cross-residual`, `not-not-in`, `not-or-not-in` did **not** move.
  `not-not-in` and `not-or-not-in` already planned as a `RightSemi` hash join before the rule
  and still do: a positive `IN` on the spine is left alone, which is what §3.5 asks for.

`testdata/cost-registry.csv`'s `in_is_null` row: plan columns `disabled` → `na` ×5,
`plan_status` `ok` → `fail`, tickets `155 250 257` → `250`. Forced by
`the_registry_matches_the_goldens_in_both_directions`, which classifies a refusal whose text
starts `refused by datafusion` as `na`. No `.cpu.txt`, `.cost.txt`, `.result.txt` or
`recipe-payloads.txt` moved — `test_cpu_corpus` and `test_cost_model` are unchanged.

**Two things the plan got wrong.**
- **Step 5 claimed `plan_goldens` would be unchanged.** It is not: three pbench sections move
  at all five modes. Corrected in the plan.
- **The nested-EXISTS case cannot witness what it is for.**
  `a_not_in_nested_in_an_exists_is_rewritten_too`'s six rows are DuckDB's *and* DataFusion's
  before the rule, because the one `s` row the rewrite changes (`z=2, y=NULL`) is covered by a
  sibling row (`z=2, y=7`) that keeps the same `o` rows. So the plan's own witness for "the
  rule walks subqueries" is blind. Added
  `a_nested_not_in_over_a_set_holding_a_null_empties_its_exists`: the inner set is
  `{x : o2.w = 1} = {1, 2, NULL}`, so SQL's `s.y NOT IN (…)` is never true and the EXISTS is
  empty (DuckDB: 0 rows), where a two-valued answer keeps six. It is red both before the rule
  and under the `transform_down` mutation, and it is the only case either reddens.

**Two things worth knowing for later rounds.**
- `a_not_in_under_not_folds_to_a_positive_in` and `de_morgan_reaches_a_not_in_under_a_negated_or`
  are green *before* the rule exists and go red if the rule ships without the NNF pass —
  because without NNF a `Not(InSubquery)` is an off-spine leaf and the #250 refusal fires on
  it. So they are guards on the rule, not on the rewrite, and that is their value: they are
  what stops the refusal swallowing a form the fold handles.
- The rule reports `Transformed::yes` whenever the NNF pass folded anything, so on a predicate
  with a `NOT` the optimizer re-runs it. It terminates: the rewritten form holds no
  `InSubquery`, and a second NNF pass over a NOT-free predicate reports `no`.

**Red-green evidence. Seven mutations, every one reverted and the revert read off `git diff`.**
- Red before the rule: **9 failed, 7 passed** of 16. The three answer cases that passed before
  are the two fold cases and the plan's nested case, discussed above; the counts were
  DataFusion's 7 rows for the correlated form and 5 for the uncorrelated, exactly §3.5's
  measurement.
- `transform_down_with_subqueries` → `transform_down`: 1 failed (the added nested case).
- `count(lit(1i32))` → `count(lit(1i64))`: 1 failed, and the failure text contains
  `PlaceholderRowExec` — #158's trap is real and the guard sees it.
- NNF pass replaced by `Transformed::no(predicate)`: 4 failed.
- `refuse_nullable_in_off_the_spine` short-circuited to `Ok(())`: 1 failed (the #250 pin).
- `in_subquery_may_meet_null` → `true`: 1 failed (`not_in_over_keys_that_hold_no_null_is_left_alone`).
- correlated form without `NOT EXISTS (S AND x IS NULL)`: 3 failed.
- uncorrelated form without `(x IS NOT NULL OR count(S) = 0)`: 4 failed.
- Green after: 16 passed.

**Deferred, named: the #246 pin of step 1b.** `nested_cases.rs` lives in
`src/tests/gpu_tests/`, so the pin needs a device to run — and a cudf-feature cargo build even
to compile, which `build-test.md` forbids in `./target` ("it would evict the rust-only
artifacts"). Writing it unrun would be a test in a commit nobody has proved. It costs nothing
extra in task 7, which pays for a device cycle anyway, and nothing in task 2 touches the
behaviour it pins (`expr.cpp:873-877` refusing a column `LIKE` pattern).

**build-test.md needs a second row (human's file).** `planner::tests::not_in` is a new
component-tier block of 16 cases, and `--lib` moves 701 → 717. Suggested row, after
"Logical-plan nullability":

    | `NOT IN` as SQL means it | [correlated_not_in_answers_as_sql](../peacockdb-core/src/planner/tests/not_in.rs) | 16 |

    the `NOT IN` rewrite, over two in-memory tables whose columns the footer reader cannot
    reach, so every operand counts as possibly-NULL and every form rewrites: the four scratch
    probe shapes, the three negation-normal-form folds, the nested `NOT IN` inside an `EXISTS`,
    the uncorrelated count that must not become `count(*)`, the tpch keys that hold no NULL and
    so plan untouched, and the two refusals this chain leaves — #250's off-spine `IN` and
    #247's seven DataFusion limits. Every expected row is DuckDB 1.5.4's over the same tables

**Wiki consequences for the human to write.** #250's "Corpus queries" paragraph says
`in-is-null` "does not reach it" and "cannot be demonstrated until #257 is fixed": both are now
false — it reaches #250 at all five modes. #257's "Corpus queries" line claims `in-is-null`'s
cells are disabled on `155`; they are `na` on `250` now, and #257 loses its only corpus row
while staying a real DataFusion bug.

#### Round 1, plan task 3 — #137, no NULL key crosses a shuffle it cannot match through

Rust only; no device cycle. **This is the round that moved the cost goldens**, as spec risk 1
warned.

**What shipped.** `plan/join.rs` gains `null_key_droppable(JoinType) -> (bool, bool)`, with a
`pub(crate)` delegate in `plan/mod.rs`. `plan/mod.rs` also gains `GpuEmitPartitions::into_parts`
and the `IntoAnyBox` trait, a blanket impl that turns `Box<dyn GpuNode>` into `Box<dyn Any>` so
a node `as_any` has identified can be taken apart rather than only read; `GpuNode` takes it as a
supertrait. `planner/translator/nodes.rs` gains `drop_null_keys_below_shuffle` and calls it from
`hash_join`, after the co-partition check and before the build's `GpuCoalesceAllBatches`. New
`planner/tests/null_key_filters.rs` (7 cases) and one case in `plan/tests/joins.rs`.

**`IntoAnyBox` is a new row in `SURFACE`, with its receipt.** A supertrait is as public as its
trait, so `pub(crate) trait IntoAnyBox` under `pub trait GpuNode` compiles with
`warning: trait IntoAnyBox is more private than the item GpuNode` (`private_bounds`, measured
on both `-p peacockdb-core` and `-p peacockdb`). That is the second receipt form the SURFACE doc
names, on `GpuNode`, a row already listed — and a warning is not acceptable here, so `pub` plus
the row is the only shape. 46 bare `pub` items becomes 47.

**Three things the plan and the design got wrong.**
- **tpch's plans do move.** Both §4.1 and the plan say tpch's keys hold no NULL so its plans
  stay. True of its scan keys; false of two queries that join on an aggregate output — q2 on
  `min(partsupp.ps_supplycost)` and q15 on `total_revenue` — where `can_be_null` says an
  aggregate's output can be NULL, because a sum over no rows is. +3 filter lines per tp4 mode,
  2 of 39 sections. The rule is exactly §4.1's ("only where `can_be_null` says the key can be
  NULL"); the prediction was narrower than the rule. The filters are correct: a NULL from an
  aggregate matches nothing under SQL equality on an Inner side. Cost: q2 +0.72%,
  q15 +1.43…1.62%.
- **`recipe-payloads.txt` moves and the plan does not mention it**, 12 of 20 queries. Any
  inserted node renumbers every seq above it, so the recipe bytes move for every query that
  gained a filter. Its `sha256=` lines are deliberately guarded: `UPDATE_CANONICAL=1` alone
  VERIFIES them and goes red, by design, so the regen needs `PEACOCK_REWRITE_RECIPE_BYTES=1`
  alongside. The `/tmp/peacock-plan-bytes-root` symlink build-test.md mentions is created by the
  test itself; no manual step.
- **`UPDATE_CANONICAL=1 … --test test_cpu_corpus -- tp4` is the wrong variable for a filtered
  run.** `UPDATE_CANONICAL` is the whole-file contract and prunes sections no declaration
  accounts for; `<tier>.result.txt` is keyed by query and written by its author mode, which a
  tp4-only run may not have visited. Ran the whole tier unfiltered instead.

**Which golden kinds moved, by section.**

| kind | files | sections | new `IS NOT NULL` lines |
|---|---|---|---|
| `*.plans.txt` tpcds tp4-{single,rowgroup,sized} | 3 | 74 of 99 each | +416 each |
| `*.plans.txt` pbench tp4-{single,rowgroup,sized} | 3 | 9 of 60 each | +13 each |
| `*.plans.txt` tpch tp4-{single,rowgroup,sized} | 3 | 2 of 39 each | +3 each |
| `*.plans.txt` every tp1 mode | 0 | — | 0 — one lane, no shuffle |
| `recipe-payloads.txt` | 1 | 12 of 20 | — (seqs and digests) |
| `*-mini.cpu.txt` tp4 × 3 datasets | 9 | — | GpuFilter +3 / +363 / +12 |
| `*-mini.cost.txt` tp4 × 3 datasets | 9 | — | derived |
| `*.result.txt` | **0** | — | the answers did not move |

The `.cpu.txt` review the coordinator asked for, done arithmetically rather than by eye: node
kinds counted before and after in each file, and **`GpuFilter` is the only kind whose count
changed** — tpch 42→45, tpcds 317→680, pbench 2→14. Nothing else moved by a single line.
(tpcds +363 in the cpu golden against +416 in the plans golden: the cpu golden carries only the
cells enabled at that mode, the plans golden carries every query including the refused ones.)

**Cost gate, `cargo run -q -p cost-report -- --cost-diff --base ee0367cd`:**
**6 improvements, 228 regressions, 173 over the 10% gate — red, and pre-accepted by the board.**
- By dataset: tpcds 198 rows (66 distinct queries), pbench 24, tpch 6.
- tpch, in full: q2 +0.72% at all three tp4 modes (374.74 → 377.46 MB); q15 +1.59 / +1.43 /
  +1.62% (56.60 → 57.50 MB).
- The worst: tpcds q33 +30.74%, q56 +30.58%, q61 +30.53% (1.74 → 2.27 GB), q31 +30.39%,
  q60 +30.30%. Distribution over the 198 tpcds rows: 21 under +5%, 16 to +10%, 21 to +15%,
  22 to +20%, 14 to +25%, 90 to +30%, 14 above.
- pbench's worst: `not-not-in` +26.89%, `uint-key-join` +25.95%, `exists-null-keys` +25.62%.
- **Not purely additive: tpcds q93 improves by 11.19%** (963.44 → 855.63 MB), and q76 by 0.01%.
  Dropping the NULL keys before the shuffle cuts what every node above it carries, so where the
  key is NULL-heavy the rule pays for itself. The metric is Σ bytes out of every node, and a new
  filter's output is one more copy of its input — which is why the typical row is +25% and why
  q93, whose join loses rows, goes the other way.
- `cost_diff.html` is written by that command; the per-query table is reproducible from it.

**Red-green evidence. Four mutations, every one reverted and the revert read off `git diff`.**
- Red before: 2 of 4 original cases (inner wanted 2 filters and saw 0, left wanted 1 and saw 0).
- `null_key_droppable`'s `Right` → `(false, true)`: 1 failed, the nine-way table case.
- the `!join.null_equals_null()` guard → `if true`: 5 failed —
  `set_semantics_keeps_the_null_keys_a_shuffle_would_skew`, tpcds's three tp4 plan goldens and
  the payload golden. tpcds carries 11 `null_equals_null=true` RightSemi joins (q14 ×6, q38 ×2,
  q8, q87 ×2), so the goldens do guard it; the targeted case is what names the rule.
- the filter put ABOVE the emit instead of below: 11 failed, including
  `the_filter_goes_below_the_shuffle_and_not_above_it` and every tp4 plan golden.
- `can_be_null` ignored (`nullable[k] || true`): 12 failed, including
  `a_tpch_join_gains_no_filter` and `translator::tests::an_equi_join_is_co_partitioned_by_a_scatter_on_each_side`
  — the footer reader is load-bearing, which is the question the dispatch asked.
- Green after: 723 passed, 2 ignored, 0 failed.

**One defect of mine, caught by reading the diff rather than by a test.** Both insertions into
`plan/mod.rs` took the doc block of the declaration below them — `GpuNode` lost "What a plan
node offers the driver and the validator." to `IntoAnyBox`, and `empty_build_answers_nothing`
lost its three-sentence block to `null_key_droppable`. This is `coding-style.md`'s "a doc
comment reassigned by an insertion" exactly, and `test_golden_format`'s
`no_declaration_carries_a_block_left_behind_by_a_split` cannot see it: that guard looks for a
block orphaned by a BLANK LINE, and an insertion is contiguous. Both fixed; the final
`plan/mod.rs` diff is pure addition with no doc block moved. Worth knowing that reading each
block's first sentence in the diff is the only check there is.

**The #257 projection form: measured, and it does not fold.** The coordinator's note says
`SELECT (f_k IN (SELECT …)) IS NULL FROM fact` still folds. It does not — measured on the
`not_in.rs` fixture: it is refused at physical planning with "Physical plan does not support
logical expression InSubquery". The fold needs the mark join, the mark join needs decorrelation,
and DataFusion 45 decorrelates only inside a filter (#247's second row). So with this chain's
rule in place #257's wrong answer is unreachable from every shape: on a WHERE spine it is
rewritten; off the spine with nullable operands it is refused on #250; off the spine with
provably non-null operands `IS NULL` folds to false, which is the right answer; and in a
projection the planner refuses it. My view on the coordinator's question: **no new ticket.** A
form that never reaches the defect has nothing to track that #247 does not already track, and
`bug_datafusion_45_refuses_six_subquery_shapes` already pins the projection refusal — adding
`IS NULL` around it is the same refusal from the same site.

**build-test.md needs a third row (human's file).** `planner::tests::null_key_filters` is a new
component-tier block of 7 cases; `plan::tests::joins` goes 23 → 24; `--lib` moves 717 → 725.
Suggested row, after "`NOT IN` as SQL means it":

    | #137's NULL-key filters | [an_inner_join_drops_null_keys_below_both_shuffles_where_the_data_holds_them](../peacockdb-core/src/planner/tests/null_key_filters.rs) | 7 |

    which side of a join drops its NULL keys before the shuffle, read off the plan text over
    real footers — pbench's `f_k` holds 1,034 NULLs and `d_k` 50, where every tpch scan key
    holds none, so the tpch case is what proves the footer reader is consulted at all. Three
    answers a query can show (Inner both sides, Left its probe, Full neither), one lane showing
    that no shuffle means no filter, and two over the hand-built fixture for what no corpus SQL
    reaches: the `null_equals_null` flag, and whether the filter landed below the shuffle

The nine-way table itself is `plan::tests::joins::every_join_type_says_which_side_may_drop_a_null_key`,
so that row's count moves from 23 to 24.

#### Round 1, plan task 5 — a predicate-free nested loop is a cross join only when Inner

Rust only; no device cycle. Wider in the tree than in the plan: a field type on a plan node
changed, so every consumer moved with it.

**What shipped.** `GpuNestedLoopJoin.join_type` is now DataFusion's `JoinType`, and
`NestedLoopJoinType` is **deleted here rather than in task 15** — see below.
`planner/translator/nodes.rs`'s `nested_loop_join` keeps the node for every type but Inner,
joining over `Expr::Literal(Boolean(true))` with no filter columns, and the Inner arm now
merges its probe. Three new cases in `planner/translator/tests.rs`, one in
`planner/tests/null_analysis.rs`.

**Every site the type change touched, and the one left alone.**

| site | what it became |
|---|---|
| `plan/mod.rs` | the field, the constructor parameter, and `NestedLoopJoinType` gone |
| `plan/join.rs` validation | `check_projection(self.join_type, …)` direct; the Left single-batch rule on `JoinType::Left` |
| `wire/join.rs` | the build-handle rule keeps Inner/Left and returns a `PlanError` naming #160 for the rest; the fb mapping now calls the existing `wire_join_type`, which was already total over the nine |
| `executor/cpu_backend/join.rs` | `&node.join_type` straight into `NestedLoopJoinExec::try_new` — the conversion match is gone |
| `planner/nullability.rs` | the arm now shares the hash join's rule, below |
| `plan_text/node_text.rs` | **not touched**: it renders `{:?}`, and both enums print `Inner`/`Left` identically, so no golden moved from the rename |
| `executor/gpu_backend/backend.rs` | **not touched**: it routes on the node kind and never reads the type |
| five rust-only test files | mechanical; `wire/tests.rs`'s two-arm match gained a panicking fallback |
| `tests/gpu_tests/nested_cases.rs` | mechanical, **and not compiled** — see the risk note |

**`NestedLoopJoinType` is deleted now, not in task 15.** After the switch nothing referenced
it, so the alternatives were a dead-code warning or an `#[allow(dead_code)]` whose only reason
is "a later task removes it" — process history in a comment, which `coding-style.md` forbids.
Task 15's cleanup list has one fewer item. This is a smaller change than the plan expects, not
a larger one.

**The nullability arm became a shared rule, after my own tooling tripped over the duplicate.**
Converting the arm to nine types made its body byte-identical to the hash join's arm —
`joined_can_be_null(type, build, probe)` then narrow by projection. A mutation I aimed at the
nested-loop arm landed on the hash join's instead, because `str.replace(…, 1)` takes the first
match and the two blocks were the same text; I noticed only because the query that went red
(tpch q15) has no nested loop in it. That is `coding-style.md`'s "a reader that stops at the
first match", in a throwaway script, and the fix is not a better script: both arms now call one
`join_output_can_be_null`, so there is one copy of the rule and nothing ambiguous to patch.

That episode also found a real gap: with the arm bypassed entirely, **nothing in the suite went
red** — no corpus query has a nested-loop join whose type or projection changes the answer. Closed
with `null_analysis.rs`'s `a_nested_loop_join_pads_the_side_its_type_drops_and_then_projects`, a
hand-built projected Left nested loop over two not-nullable sources, which asserts `[true, false]`
and returns the 4-wide unpadded vector under the bypass. The old arm also ignored the projection;
`NodeRef::CrossJoin` still does, which is harmless today — a wrongly-read flag can only skip a
#137 filter (skew, not a wrong answer) or add one that removes nothing — and is not this task's.

**Which golden kinds moved.**

| kind | files | sections | change |
|---|---|---|---|
| `*.plans.txt` tpcds, all five modes | 5 | q9 | 15 joins × 3 sections |
| `*.plans.txt` pbench, all five modes | 5 | scalar-subquery-cross | 4 joins × 3 sections |
| `*.plans.txt` tpch | 0 | — | no predicate-free nested loop |
| `*-mini.cpu.txt` tpcds tp1/tp4 ×5 | 5 | q9 | `GpuCrossJoin` 27→12, `GpuNestedLoopJoin` 3→18 |
| `*-mini.cpu.txt` pbench tp1 ×2 | 2 | scalar-subquery-cross | `GpuCrossJoin` 5→1, nested loop 0→4 |
| `*-mini.cost.txt` | **0** | — | cost is bytes, and no byte moved |
| `*.result.txt` | **0** | — | the answers did not move |
| `recipe-payloads.txt` | **0** | — | neither query is in the payload subset |

Every changed line in every `.plans.txt` is a `GpuCrossJoin` → `GpuNestedLoopJoin` line, in the
tree, the recipe (`CudfCrossJoin, build copy` → `CudfNestedLoopJoin, build`) and the memory
section. Checked by grepping the whole plan-golden diff for anything that is not one of those two
node names: nothing. No seq renumbered, no figure changed, no `IS NOT NULL` moved.

**Two things the plan and the dispatch got wrong.**
- **pbench's `scalar-subquery-cross` moves too**, which the dispatch asked to hear about before
  the commit. It is the same shape as q9 — DataFusion plans a scalar subquery as a predicate-free
  `Left` nested loop, and the query is four of them chained. Its name says cross because that is
  what the engine used to make of it. `every_pbench_join_plans_as_its_spec_says` asserted
  `GpuCrossJoin` for it; corrected, with the reason, in the same shape as the `not-not-in` row's
  existing correction. The query name is now mildly misleading, and renaming it would move its
  golden sections, its registry row, its corpus declaration and `gen.sql` — out of scope here.
- **The cost goldens do not move.** The dispatch expected them to; `--cost-diff --base 4ef0e714`
  reports 703 compared, 0 changed, 0 regressions. The change is a node name and a build-handle
  rule, and the cost model prices bytes out per node.

**One `.cpu.txt` change that is not just a node name, as asked.** q9's and
`scalar-subquery-cross`'s `batch_rows` go from `[[1]]` to `[[1,0]]`: the cpu's Left
`NestedLoopJoinExec` emits the matched rows and then the unmatched-build pass, two batches per
call, where `CrossJoinExec` emitted one. Not new behaviour — the `nested-loop-left-join` golden
has carried `batch_rows=[[50,1]]` all along — and not an answer change, since `.result.txt` did
not move and the extra batch holds no rows. It is #220's shape (the cpu answers several batches
per call) arriving at two more queries, and task 8's interim adapter folds it back into one.
The device's `CudfNestedLoopJoin{Left}` is one call and one batch, so the two engines differ in
batch structure here; q9's and `scalar-subquery-cross`'s gpu cells are all disabled today, so
nothing compares them yet.

**Red-green evidence. Seven mutations, every one reverted and the revert read off `git diff`.**
- Red before: the Left case planned as `GpuCrossJoin`; the Inner case left its probe unmerged.
- predicate-free Left treated as Inner: 11 failed —
  `a_predicate_free_left_nested_loop_is_not_a_cross_join` and every pbench and tpcds plan golden.
- the Inner arm not merging its probe: **1 failed**, the targeted case alone. No golden moves,
  because no corpus query has a multi-lane probe under a predicate-free Inner nested loop — so
  that case is the only guard the merge has.
- the #160 refusal removed: 9 failed, including the `bug_` pin, `join_capability`'s and
  `join_refusals`' existing refusal cases and pbench's five goldens.
- the Left build handle copied instead of handed over: 17 failed, including
  `wire::tests::a_nested_loop_join_copies_its_build_side_only_where_the_probe_streams`, every
  plan golden and the payload golden.
- the nullability arm ignoring the join type: 1 failed,
  `join_capability::an_outer_join_makes_its_padded_side_nullable_for_the_join_above_it`.
- the shared rule dropping its projection narrowing: 9 failed, tpch q15 among them — the hash
  join's half of that rule is load-bearing on today's corpus.
- the nested-loop arm bypassing the shared rule: **nothing red** before the new
  `null_analysis.rs` case, 1 failed after it.
- Green after: 727 passed, 2 ignored, 0 failed.

**One risk I cannot retire: `tests/gpu_tests/nested_cases.rs` is edited and not compiled.**
`--features gpu` makes `peacockdb-ffi` build the C++ side through cmake and needs `CUDF_ROOT`,
which exists only on nebius-gpu, so there is no local way to typecheck the gpu rung — not even
with a separate `CARGO_TARGET_DIR`. The edit is mechanical (an import, eleven enum paths, a
signature, and one match that gained a panicking fallback) and the diff is in the round's record
to be read, but the first thing task 7's device build does is prove or disprove it.

**Pre-existing, not mine, and worth somebody's attention:**
`planner/tests/plan_goldens.rs` is 1,113 lines, over `coding-style.md`'s 1,000-line cap. It was
1,110 at task 3's commit and over the cap before this chain began — the coordinator's three-line
comment in task 2 is the only thing either of us added. Splitting it is a refactor this task's
restriction does not cover; task 10 is the round that touches the test files.

**build-test.md counts (coordinator's file).** `--lib` moves 725 → 729 cases:
`planner::translator`'s module unit block gains 3 and "Null analysis rules" goes 8 → 9. No new
row.



#### Round 1, plan task 6 — the join traits, the driver, the mock and the harness

Rust only; no device cycle. The first round that moves the executor traits, and the first where
the spec knew something the plan's step list did not.

**The traits.** `executor/mod.rs`: `JoinExecutor::set_build(Option<B::Batch>)` replaces
`set_build(Batch)` and `without_build`; `ProbingJoin` returns `Option<B::Batch>` from both calls
and gains `owes_nothing()`. `CallKind::NoBuild` is gone.

**The driver.** `LaneCall::NoBuild` is gone; `select` answers `SetBuild` whether the build slot
has a batch or not, and the `SetBuild` arm routes to `LaneState::Draining` when
`probing.owes_nothing()` and to `Probe` otherwise. `partitioned.rs`'s `lane_left_build` trigger
is one call instead of two. One piece the plan does not name was needed: **`LaneCall::needs_its_batch`**.
`consumes()` says `SetBuild` reads slot 0, and `take_input` refused an empty queue — so a
`SetBuild` with no batch failed with "the schedule offered a batch that is not there". The new
predicate separates "which slot this call reads" from "the slot has to hold something", and
`take_input` returns `Option<Held>`.

**The adapters, and the line each later task deletes.**
- `cpu_backend/backend.rs`'s `one_batch`: 0 chunks → `None`, 1 → it, several →
  `concat_batches`. **Task 8 deletes it**, with `CpuJoin::without_build_allowed`,
  `CpuProbingJoin::build: Option<RecordBatch>` and its `build()` accessor: one stream per lane
  answers an absent build and one batch per call by construction.
- `gpu_backend/backend.rs`'s `one_batch`: 0 → `None`, 1 → it, several → a `BackendError`, since
  a device call answers at most one handle and concatenating a case that cannot occur would hide
  it. **Task 7 deletes it**, with `GpuJoin::without_build_allowed`.
- `owes_nothing` is a field on both probing joins, set at `set_build` from the join type's own
  answer. One fact, one field, the same name in both backends — and it stays correct after tasks
  7 and 8, when an absent build under Right, Full or RightAnti produces a probing join that owes
  its probe rather than a refusal.

**The plan's step 3 asks for one removal that cannot land here, and I measured it rather than
arguing.** With `index.rs`'s `feeds_owing_build` and `partitioned.rs`'s scatter-drop exception
removed, a Right, Full or RightAnti lane whose build side is empty reaches `set_build(None)` —
and the adapters answer that with today's refusal, because the engines that can answer it are
tasks 7 and 8. **Six tests red over four queries**, every one with #212's message:
`tests::end_to_end::{tpcds_q93, tpcds_q97, tpch_anti_join}`,
`dimensions::{a_right_join_with_an_empty_build_pads_every_probe_row,
a_right_anti_join_with_an_empty_build_returns_every_probe_row}` and
`flow::holds_and_releases_balance_over_a_plan_with_empty_lanes`. So `feeds_owing_build`, the
exception, and `dimensions.rs`'s use of both stay for now and leave with the engine that
replaces them — the device in task 7, the cpu in task 8. Noted in the plan at the step.

**The spec's risk 2 was right that the goldens move and low on how many.** It says "about 82"
rows; measured, **497 (query, mode) cells over 116 distinct queries** — tpcds 75, tpch 26,
pbench 15. If "rows" meant registry rows, the estimate was 82 against 116, low by about 40%; if
it meant cells, it was low by six times. Either way the direction and the kinds were right. One
line for the completeness record.

**Which golden kinds moved, and the one that did not.**

| kind | files | what moved |
|---|---|---|
| `*-mini.cpu.txt` | 15 | `batch_rows`/`batch_bytes` per call, and the `output_rows` of nodes above a join |
| `*-mini.cost.txt` | 14 | derived, downward |
| `*.result.txt` | **0** | the answers did not move |
| `*.plans.txt`, `recipe-payloads.txt`, `cost-registry.csv` | **0** | the planner did not change |

Two arithmetic checks rather than an eyeball. Node-kind counts in each `.cpu.txt` are
**identical** before and after — no node appeared or vanished. And every `GpuUnload:
output_rows` is identical in all three datasets (35 + 74 + 27 sections), which is the answer.

**An `output_rows` change that is not a batch count, and why it is right.** tpch q3's
`GpuAggregate` goes from 21,242 rows to 11,620. The join below it used to emit four batches
(8192, 8192, 8192, 5943), so the partial aggregate ran four times and emitted four group sets;
with one batch of 30,519 rows it runs once and emits 11,620 groups. Fewer duplicate partial
groups, same final answer — `GpuUnload: output_rows=10` either way. That is also where most of
the cost improvement comes from: the saving is not the batch headers, it is the partial
aggregation above the join.

**Cost gate, `cargo run -q -p cost-report -- --cost-diff --base 4db7e82f`:**
**445 improvements, 0 regressions.** The opposite direction from task 3. tpcds 318 rows, tpch
111, pbench 16. Largest: tpcds q22 −17.54% at tp1-single (692.96 → 571.38 MB), tpch q13 −11.75%
(173.91 → 153.47 MB), tpcds q22 −11.01% at tp1-rowgroup, tpcds q65 −8.60%, tpcds q22 −8.64% and
−8.63% at the tp4 modes.

**Tests, and one gap closed.** `flow.rs`'s join cases are the plan's:
`a_lane_with_no_build_batch_calls_set_build_none` (one `SetBuild`, no `Probe`, one drop per probe
batch, no `Finish`), `a_lane_that_owes_its_probe_side_probes_it` (the old
`…_is_refused` case, which no longer refuses because the mock answers the absence),
and the two scattered-build cases now counting `SetBuild` and `Probe`.
`single_partition/tests.rs`'s `a_build_side_that_never_produced_still_chooses_set_build`
replaces the `NoBuild` one, and its decision sweep filters on `needs_its_batch`.
`dimensions.rs`'s "no owing lane reached NoBuild" assertion became "every lane of an owing join
reached `set_build`", counted per node against its lane count — the old form is structurally
true once the kind is gone, and that is a guard that cannot go red.
**The gap:** nothing in the rust-only tier covered the #212 refusal — it was pinned only in
`gpu_tests/join_cases.rs`, which needs a device. Closed with
`cpu_backend::tests::join::a_build_side_that_produced_nothing_is_refused_for_the_types_that_owe_their_probe`:
the three owing types refuse `set_build(None)` naming #212, and the six others answer
`owes_nothing()`.

**Red-green evidence. Six mutations, every one reverted and the revert read off `git diff`.**
- Red before: a compile error on the traits, then 15 of the lib tier — five driver and flow
  cases, `dimensions::a_right_join_with_an_empty_build_pads_every_probe_row`, and nine
  `tests::end_to_end` queries failing with "the schedule offered a batch that is not there",
  which is what made `needs_its_batch` necessary.
- the mock's `owes_nothing()` → `true`: 11 failed.
- → `false`: 2 failed (`a_lane_with_no_build_batch_calls_set_build_none` and the
  owes-nothing scattered-build case).
- `needs_its_batch()` → `true`: 15 failed, the end-to-end set among them.
- the cpu skipping `without_build_allowed` in `set_build(None)`: 1 failed, the new #212 case.
- the cpu `one_batch` keeping only the first chunk: 16 lib failures and 4 of 5 `cpu_tpch_q3`
  cells — the concatenation is load-bearing for the answer, not just the batch count.
- the scatter-drop exception removed (the plan's step 3): 6 failed, above.
- Green after: 728 passed, 2 ignored, 0 failed; 983 corpus cells.

**The identical-text trap again, in the other direction.** Last round a mutation aimed at one of
two byte-identical blocks landed on the other. This round an un-asserted
`str.replace(…, 1)` for `let out = hold_all(out, acct, Some(batch.bytes))?;` hit the
**Accumulate** arm instead of **Probe**, because the two arms spell that line identically — and
then a later asserted replace caught the real ones, so all four changed and two of them had no
business changing. It compiled and passed (a `Vec` collected into a `Vec`), and it showed up only
in the hunk-by-hunk diff read. Both reverted. The rule I am taking forward: assert the match
count before **every** replace, not just the ones that look risky.

**Still unproven: the gpu rung.** `tests/gpu_tests/script.rs`'s join arm,
`gpu_backend/gpu_tests/join.rs`'s five `set_build` sites and task 5's `nested_cases.rs` are all
edited and not compiled — `--features gpu` makes `peacockdb-ffi` run cmake and needs
`CUDF_ROOT`, which exists only on nebius-gpu. Task 7's device build is the first thing that
proves any of it.

**build-test.md counts (human's file).** `--lib` moves 729 → 730: `cpu_backend`'s join block
gains one case, and `executor::driver`'s flow block keeps its count (one case replaced, one
renamed). No new row.
