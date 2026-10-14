# join-backend — run record

Chain J task 8. Branch `ENS-join-backend`, forked from `ENS-join-session-cpp` at `3d9a122e`.
PR targets `ENS-join-session-cpp`. Spec [`join-backend.md`](join-backend.md) (frozen); plan
[`join-backend-impl.md`](join-backend-impl.md) (16 tasks, run order 1, 2, 3, 5, 6, 7, 8, 4, 9,
9b, 9c, 10–16); design [`join-rewrite-design.md`](join-rewrite-design.md) §1.3, §3.5, §4, §5.7.

## State

| | |
|---|---|
| state | `completing` since 2026-10-14, PR #179 against `ENS-join-session-cpp` |
| developer rounds | 1 dispatched 2026-10-09, stopped by the human mid plan task 7; 2 dispatched 2026-10-13 |
| review rounds | none |
| tickets filed | #267 (plan task 9). Next free is **268**; master reserved 264–279 for this chain |

## Hosts, and the device recipe now proven (2026-10-09)

- **nebius-gpu, reached as the `~/.ssh/config` alias `nebius-gpu`, never by IP** (the board's
  override; the IP once quoted here is gone), `computeinstance-e00tnrse7ayntnzcyt` — up, L40S,
  46,068 MiB VRAM with 0 MiB in use, cuDF 25.02 from `~/data/miniforge3/envs/rapids-cuda-12.2`,
  sf1 tpch/tpcds/pbench present in `~/peacockdb-J/testdata`.
- **Disk is the standing risk and it is getting tighter: 14 GB free of 96** after task 7's first
  device build (21 GB before it, 15 GB after). Nothing was deleted to get there — the 17 G
  `target-cudf-rapids-cuda-12.2` is reused incrementally, which is why the second build took 23
  seconds. One more full rebuild fits; a `cargo clean` is not recoverable inside 14 GB. A build
  that runs out of disk reads as a compiler error, so check `df -h ~` first, and clean only stale
  build and target dirs inside `~/peacockdb-J`. `~/peacockdb-K` is chain K's worktree: never touch
  it, nor `~/miniforge3`.
- **The recipe works and `--build` is safe with shad-gpu down.**
  `./scripts/build-test-shadgpu.sh --build` exits 0 and neither pushes nor ssh-es anywhere; each
  staged binary is then run directly with `--test-threads=1`. Never `--run`, which ssh-es to
  shad-gpu.
- **shad-gpu** — down since 2026-10-08. The `gpu-tests` CI job is exempt by the override.
- **verda** — unlocatable: `VERDA_CLIENT_ID` is unset, so `scripts/list_verda_instances.sh`
  exits on it and `ssh verda` resolves nowhere. **Every CPU build and run is local.**

### The device baseline, at task 6 plus its harness fix

816 cases over seven binaries, every one green, each binary confirmed to have run tests:
`peacockdb_core_gpu_lib gpu_tests::` 580, `test_gpu_corpus` 95, `test_node_timing` 1,
`peacock_gpu_benchmarks --skip bench_` 8, `peacock_gpu_tests` 2, `peacock_plan_tests` 77,
`peacock_join_session_tests` 53. Not run, per the board: `peacock_cpu_tests` (local), the sf40
pair, the `bench_` cases. This is the figure a later device round is read against.

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

## Four workstation traps, each already paid for once

Carried from the chain's previous run, and the first one cost this task a round: the coordinator
had them and passed on only the inherited-code list.

- **`pgrep -f build-test-shadgpu` matches its own ssh command line** and so reports RUNNING
  forever. `build-test.md`'s antipattern section names the shape: a wait loop whose pattern
  appears in the waiter's own argv waits for itself and ends only at the timeout. Poll the log's
  mtime, a terminal line in it, or an `rc` marker file written by a detached `nohup` — never a
  process pattern. Task 7's first round polled a build that had finished for about 38 minutes.
- **Local `cpp/build` is an empty root-owned directory** and cmake dies at configure blaming
  itself. `/tmp/dkb-cppbuild` is configured against this worktree and is the 30-second local
  compile check. Clearing `cpp/build` needs `sudo` and is the human's.
- **`matplotlib` is absent** while `scripts/calibration/tests/test_plot.py` imports it at module
  scope. `pip3 install --target` outside the tree plus `PYTHONPATH` works.
- **A stray `/tmp/struct.py` shadows the stdlib** for any python run whose cwd is `/tmp`.

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
**Taken in round 3, against `3d9a122e`: 703 compared, 278 improvements, 251 regressions, 173 past
the 10% gate** — by dataset and worst rows under "Round 3 — review round 1's findings".

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

#### Round 1, plan task 7 — the device cycle, the inherited compile verdict, and a task 6 defect

This round bought the device cycle and spent it on the three things that had to come before any
task 7 code: whether the blind edits compile, whether the device tier is green at task 6's
commit, and what `chunk_bytes` is really priced at. **Task 7's steps 1–6 are not started** — no
wire writer, no session executor, no pricing, no pin flips, no goldens.

**Run identification.** Host `dmitry@89.169.109.150` (nebius-gpu,
`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S 46,068 MiB with 0 MiB in use. cuDF **25.02**
(`CUDF_VERSION_MAJOR 25`, `MINOR 2`, from
`~/data/miniforge3/envs/rapids-cuda-12.2/include/cudf/version_config.hpp`). Tree synced
uncommitted with the board's `rsync -a --delete-after` line; built in `~/peacockdb-J` with
`./scripts/build-test-shadgpu.sh --build`, which exits 0 and stages four rust binaries and six
C++ ones. Every binary run directly, `--test-threads=1`, with
`LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib` and
`PEACOCK_TESTDATA_DIR=$PWD/testdata`. `--build` does **not** push or ssh anywhere, so it is safe
with shad-gpu down.

**Disk, measured rather than inherited.** 21 GB free of 96 before the first build — not the 26
the dispatch carried, so that reading was already stale. 15 GB after the first build, 14 GB after
the second. **I removed nothing**: `~/peacockdb-J/target-cudf-rapids-cuda-12.2` is 17 G and was
reused incrementally, which is what made the second build 23 seconds. `~/peacockdb-K` (17 G) and
`~/miniforge3` (17 G) untouched. A third full rebuild would fit; a `cargo clean` would not be
recoverable inside 14 GB and must not be done casually.

**The first finding is the good one: tasks 5 and 6's blind edits compile.**
`cpp/install/rust-tests/peacockdb_core_gpu_lib` is the binary that carries every `gpu_tests`
module, and it staged clean. That covers task 5's `tests/gpu_tests/nested_cases.rs` (the
`NestedLoopJoinType` → `JoinType` conversion, eleven paths, a signature and a panicking match
arm) and task 6's `tests/gpu_tests/script.rs` and `gpu_backend/gpu_tests/join.rs` (five
`set_build(Some(…))` sites).

**The second finding is a defect in task 6, which is committed: `script.rs` called
`finish_and_fetch` on a drained lane.** Four cases failed —
`join_cases::{left, left_semi, left_anti, left_mark}_with_no_build_batch_is_never_probed` — with
`cpu refused: a join whose build side is absent was probed rather than drained`. The cause is
mine and the plan's together: task 6 step 5's sketch puts the finish call *outside* the
`if !probing.owes_nothing()` block, and I copied it. The driver does not do that —
`flow::a_lane_with_no_build_batch_calls_set_build_none` asserts `Finish == 0` for exactly this
lane — so the harness, which stands in for the driver, was violating the protocol the driver
keeps. The guard that caught it is the `build()` accessor task 6 added, doing its job.
Fixed by moving the finish inside the block, which also restores the pre-task-6 contract that a
no-build lane pushes no slots at all. Rust-only stayed green throughout (728), because
`script.rs` is `#[cfg(all(test, feature = "gpu"))]` and the cpu tier never compiles it: **this
class of defect is only ever found by a device build.** Re-synced, rebuilt (23 s incremental),
re-run: 580 passed, 0 failed.

**The device baseline at task 6's commit plus that fix — 816 cases over seven binaries, all
green.** Nobody had measured the device tier since shad-gpu went down, so this is also the
precondition task 7's work will be read against.

| binary | result |
|---|---|
| `rust-tests/peacockdb_core_gpu_lib gpu_tests::` | **580 passed**, 0 failed, 734 filtered out |
| `rust-tests/test_gpu_corpus` | **95 passed**, 0 failed |
| `rust-tests/test_node_timing` | **1 passed** |
| `rust-tests/peacock_gpu_benchmarks --skip bench_` | **8 passed**, 3 filtered out |
| `bin/peacock_gpu_tests` (gtest) | **2 passed**, 2 suites |
| `bin/peacock_plan_tests` (gtest) | **77 passed**, 16 suites |
| `bin/peacock_join_session_tests` (gtest) | **53 passed**, 1 suite |

Not run, per the board's override: `peacock_cpu_tests` (local), the sf40 pair
(`peacock_tpch_tests`, `peacock_tpchv_tests`), and `bench_` cases.

**`chunk_bytes`: the fbs is right and the plan and §4.3 are wrong, confirmed by reading.**
`flatbuffers/gpu_plan.fbs:479-483` says: "The executor bounds `pairs x (8 + the residual's
per-row bytes)` by it, the second term being the sum over the filter's columns of their fixed
width, 16 for a variable-width one — **not 8 bytes a pair, which is the index maps alone**." The
plan's step 5 ("8 B per key-match pair") and design §4.3 both state the flat form. **Task 7's
pricing must follow the fbs**, or the accountant and the session disagree about the same call,
which is what step 5b exists to prevent. §4.3 is this task's to reconcile and the sentence to
change is the flat-8-bytes one.

**The four FFI symbols are declared as the plan's step 2 says**, in `peacockdb-ffi/src/lib.rs:202-237`:
`peacock_join_build(exec, seq, build, *mut join, *mut stats) -> i32`,
`peacock_join_probe(exec, join, probe, *mut handle, *mut stats) -> i32`,
`peacock_join_finish(exec, join, *mut handle, *mut stats) -> i32`,
`peacock_join_release(exec, join)`. Their doc comments carry the contract task 7 implements
against: `build = 0` for no build batch; probe answers exactly one handle, possibly zero rows, or
0 for the build-side semi family; finish answers one handle for Left/Full/LeftSemi/LeftAnti/
LeftMark and 0 for the rest; release is idempotent.

**A mistake of mine worth the line, because this file already warns about it.** My wait loop was
`until ! ssh … "pgrep -f build-test-shadgpu >/dev/null"`, and `pgrep -f` matched **its own
command line** on the remote, so it reported RUNNING forever. The build had finished at 23:03
and I polled a finished build for about 38 minutes before checking `ps` for an actual compiler
and finding load average 0.08. `build-test.md` names this exact trap ("a wait loop that matches
itself") and I walked into it anyway. What would have caught it in one step: wait on the **log's
mtime or a terminal line in the log**, not on a process pattern that includes the pattern.

**What task 7 still needs, so the next round does not re-survey it.** Steps 1–6 untouched:
`wire/join.rs`'s one leaf writer for the three nodes, `FbKind::Join` and
`CallPattern::{JoinBuild, PerProbeBatch}` (neither exists yet — `FbKind` has `HashJoin`,
`CrossJoin`, `NestedLoopJoin` and `CallPattern` has `PerProbeBatch` but no `JoinBuild`),
`serialize_join_schema` as `pub(super)`, `Writer::with_join_scratch` plus the budget through
`planner/pipeline.rs`, `gpu_backend/join.rs` on the four symbols with `Drop` releasing,
the §4.3 pricing in `memory_estimation.rs`, `fb_text.rs`'s `payload_text` arm (still `_ => {}`,
so a `CudfJoin` renders nothing and the payload golden will be empty until the writer chooses
which of the nine fields to print), and step 6's pin flips. The #246 pin deferred from task 2
also belongs here.

##### Round 1, plan task 7 continued — step 5's planner half, and where the task stands

Stopped on the human's `stop` order at a green line. **Nothing was reverted**: the round's one
change is complete and every tier is green, so this is option 1 — a green partial.

**What landed: step 5's planner half, the §4.3 pricing in `memory_estimation.rs`.** The `Join`
arm's resident figure was `build bytes + probe rows × key width` — the frozen surface's
accumulated probe keys (#136). It is now `build bytes + 16 × build rows + (build rows where the
type answers unmatched build rows at the finish)`: the batch, the hash table its keys go into,
and one byte a row saying which matched. Every term is a function of the **build** side now,
where the old one grew with the probe. `HASH_TABLE_BYTES_PER_ROW = 16` is a named constant with
its reason. `key_width` and the two arrow imports it needed are deleted — nothing else used them,
and keeping a helper for a term that is gone is the dead code task 15 would have had to find.

**Which formula the code follows, since the plan and §4.3 disagreed.** This step is the
*planner's* resident model and the fbs/§4.3 disagreement is about the *executor's* per-call
`scratch_bytes` — so this half does not depend on it. The pricing that does is step 5's executor
half (`gpu_backend/join.rs`), not yet written; **it must follow the fbs**, `pairs × (8 + the
residual's per-row bytes)`, and bound the same number `chunk_bytes` carries.

**The test now separates the two models, where it did not before.**
`a_build_preserving_join_is_charged_for_the_keys_it_accumulates` becomes
`a_build_preserving_join_is_charged_for_its_matched_column`, and its assertion changed from
`left > inner` to `left - inner == 10_000` — supplier's exact row count, one byte each. The
inequality passes under **both** models (the old term was also positive), so it was a guard that
could not go red; the equality fails against the old model with 750,000 (150,000 probe rows × 5)
where it wants 10,000. Red measured before the change, green after.

**Goldens: 15 `*.plans.txt`, regenerated and consistent with the code as committed.** Checked
rather than asserted: with the `estimated_max_resident_size` and `budget=…` lines filtered out,
every plan golden is **byte-identical** to `24ea9c03` — the trees and the `--- recipes ---`
sections did not move, only the `--- memory ---` figures. No `.cpu.txt`, `.cost.txt`,
`.result.txt`, `recipe-payloads.txt` or `cost-registry.csv` moved, and
`cargo run -q -p cost-report -- --cost-diff --base 24ea9c03` reports **703 compared, 0 changed**.
The accumulator totals move in **both** directions (one tpch section 104,000 → 136,000 as the
hash table arrives, another 156,000 → 90,000 as the probe-key term leaves), which is what a model
change looks like rather than a uniform inflation.

**Where task 7 stands, step by step.**

| step | state |
|---|---|
| 1 — the failing tests (`wire/tests.rs`, `gpu_tests/join.rs`) | **not started** |
| 2 — run red | **not started** |
| 3 — the writer (`wire/join.rs`, one leaf for three nodes) | **not started** |
| 4 — the executor on the four session symbols | **not started** |
| 5 — pricing | **half done**: the planner's resident model landed; the executor's `resident_bytes`/`scratch_bytes` not started |
| 5a — `serialize_join_schema` | **not started** |
| 5b — the chunk budget (`join_scratch_bytes`, `Writer::with_join_scratch`, the pipeline) | **not started** |
| 6 — the pin flips | **not started** |
| 7 — the device run and the recipe-line/payload goldens | **not started** |
| the #246 pin deferred from task 2 | **not started** |

**What the next round should do first, and why in that order.**
1. **The wire switch and the executor rewrite have to land together.** `FbKind::Join` is what
   `attach.rs` routes to and what `gpu_backend/join.rs` reads; changing one without the other
   leaves the device build broken, and the device build is the only thing that compiles it. So
   steps 3 and 4 are one unit of work, not two.
2. **Nothing smaller is independently landable.** `serialize_join_schema` (5a) and
   `join_scratch_bytes` (5b) are both dead code until `cudf_join` calls them, and an
   `#[allow(dead_code)]` whose only reason is "a later step calls it" is the process-history
   comment this chain has refused twice. Step 5's planner half was the one piece that stood alone,
   which is why it is what landed.
3. **The remote host is now a 23-second incremental compile check** (`./scripts/build-test-shadgpu.sh
   --build` after the `rsync` line), so the gpu-tier files are no longer blind. Use it as the
   loop for steps 3–4 rather than writing them unverified.

**Additive enum variants the next round needs, surveyed so it does not re-derive them.** `FbKind`
has `HashJoin{join_type}`, `CrossJoin`, `NestedLoopJoin` and needs `Join` →
`fb::PlanNodeKind::CudfJoin`. `AbiSymbol` has four variants and needs `JoinBuild`, `JoinProbe`,
`JoinFinish` (names `join_build`, `join_probe`, `join_finish`). `CallPattern` has `PerProbeBatch`
and `AtDone` but no `JoinBuild`. The target line step 3 wants —
`join_build(#4), per probe batch: join_probe, at done: join_finish` — needs two renderer rules in
`recipes.rs`'s `impl Display for Recipe`: a pattern whose `text()` is empty prints no `": "`
prefix, and a call whose target repeats the previous call's prints no `#seq kind`. Both are
general rules rather than join special cases.

**`fb_text.rs`'s `payload_text` arm is still undecided and still `_ => {}`.** Nothing forced the
choice this round because no `CudfJoin` is written yet. The question to answer when it is: which
of the nine fields a reader of `recipe-payloads.txt` needs in order to see a *wrong* plan. My
reading, for the next round to accept or reject: `join_type`, `keys`, `null_equals_null`,
`projection` and `chunk_bytes` are the five a wrong plan shows up in, and `build_schema` /
`probe_schema` are the two a pad-type bug shows up in — seven of nine. `filter` and
`filter_columns` are already rendered for the nested loop today and should stay, which makes it
nine; the argument for fewer is that the two schemas are long and repeat the plan line's schema.

### Round 2 — dispatched 2026-10-13

Fresh developer: round 1's died with the run the human stopped, so there is nobody to resume.
Starts at plan task 7 steps 3–4 together (the wire switch and the session executor), then the
rest of task 7, then task 8, reporting after each plan task for its own commit. Hosts at
dispatch: nebius-gpu up, L40S idle, **14 GB free of 96** (unchanged since 2026-10-09); verda
unlocatable (`VERDA_CLIENT_ID` unset), so every CPU run is local.

#### Round 2, plan task 7 — the device on the session, and `CudfJoin` on the wire

Steps 3 and 4 landed together, with 5's executor half, 5a, 5b, the step-6 flips, the #246 pin
and step 7's goldens. No C++ and no fbs change: `CudfJoin` and the four symbols were
join-session-cpp's. The old three fbs tables, their `execute_*_join`, their `fb_text.rs` arms and
`read.rs`'s child-walk arms stay for task 15, since nothing writes them now but the C++ and the
walk still read them.

**What shipped.**
- `wire/`: `FbKind::Join` (one kind, no field — the type is the node's own now),
  `AbiSymbol::{JoinBuild, JoinProbe, JoinFinish}`, `CallPattern::JoinBuild`, `Call::join`.
  `wire/join.rs` is one writer over `JoinNode::{Hash, NestedLoop, Cross}` (a cross is Inner
  with no key and no condition). `CudfJoin` is a leaf (`writer.node(0, …)`): the two inputs
  stay in the pool and the walk's own `reduce` gathers them under a structural `CudfUnion`, so
  seqs stay the C++'s post-order with no change to `writer.rs`'s rules. Gone as dead with the
  writers: `FbKind::{HashJoin, CrossJoin, NestedLoopJoin}`, `ProjectRole::{ProbeKeys, NullPad,
  Narrow}`, `Input::{BatchCopy, BuildSide, BuildSideCopy, AccumulatedKeys}`,
  `Input::is_build_side`, `attach.rs`'s `cross_join`, `node_writer::null_literal`.
- The recipe line is `join_build(#4 CudfJoin); per probe batch: join_probe; at done:
  join_finish`, from three general renderer rules in `recipes.rs`: a call addressing the
  previous call's seq names no target, a call with nothing to pass has no parentheses, a pattern
  whose text is empty has no prefix. The plan's `join_build(#4), …` dropped the kind and used
  `, ` between phases; every other line in the goldens keeps `#seq kind` and `; `, so this one
  does too. The payload section prints a session's payload once, not once per call.
- `fb_text.rs`'s `CudfJoin` arm: **all nine fields** — `join_type`, `keys` (when any),
  `filter`, `filter_columns`, `null_equals_null` (when true), `build_schema`, `probe_schema`,
  `projection`, `chunk_bytes` (when not 0). Round 1's reading accepted. The two schemas decided
  it: they are what an absent side and every pad are typed from, so a wrong pad type is visible
  in this golden and in no other text; that they repeat the children's plan lines is the price.
- `gpu_backend/join.rs` on the four symbols: `set_build` always makes `join_build` (handle 0 for
  no batch), `probe_and_fetch`/`finish_and_fetch` answer `Option` (handle 0 is `None`), `Drop`
  releases. Every call journals one `AbiCall` on the join's seq, target `Node(FbKind::Join)`.
  `GpuJoin::new` refuses a recipe that is not the session line for the node's type. The
  `one_batch` adapter, `without_build_allowed`, `JoinCall`, `make`, `copy_of`, `build_copy`,
  `finish_without_keys`, `key_schema` are gone.
- Pricing (step 5's executor half), **the fbs formula**: resident = build bytes + 16 B a build
  row (keyed only) + 1 B a row of `matched` (finishing types); probe scratch = `n_bytes` + the
  output priced at `n_rows` + `pairs × (8 + residual bytes a row)`, capped at `chunk_bytes`
  (0 → 1 GiB); pairs = `n_rows` keyed (one match a row — an estimate), build rows × `n_rows`
  keyless; residual bytes are cuDF's fixed width per filter column, 1 for a bool, 16 for a
  variable-width one (`filter_row_bytes`). Finish scratch = the output priced at build rows.
  Device runs carry no budget (`corpus_gpu.rs` passes `None`), so nothing in the corpus can
  trip on it; `a_join_calls_pair_scratch_is_the_sessions_chunked_formula` and
  `a_session_holds_its_build_its_hash_table_and_its_matched_column` hold the two figures.
- 5a: `serialize_join_schema` in `wire/join.rs` wraps `serialize_schema`'s #249 refusal with
  the side. 5b: `join_scratch_bytes(knobs)` in `wire/mod.rs`, written by
  `Writer::with_join_scratch`, carried as `RecipePlan::join_scratch_bytes()` for the executor.
- `plan/join.rs`: `needs_finish(join_type)` (the five), which `capability` now reads; and
  `empty_build_answers_nothing` → `empty_build_owes_nothing` everywhere (driver, cpu, tests).

**What the plan got wrong, and what this round decided instead.**
- **The pipeline never builds recipes** — `planner/pipeline.rs` returns a tree and a model, and
  only tests and the harness call `attach_recipes`. So the budget reaches the writer as a second
  argument, `attach_recipes(root, join_scratch_bytes)`: planned queries pass
  `join_scratch_bytes(&knobs)` (plan goldens, `corpus_gpu.rs`, `corpus_benchmark.rs`,
  `node_timing.rs` through `DevicePlan.join_scratch`, `record/tests.rs`, the wire device walk),
  hand-built trees pass 0. `Session::open` (`test_support/gpu_session.rs`) takes it too.
  `join_scratch_bytes` is in `wire/` because nothing in a production build calls it, and wire's
  `allow(dead_code)` is the one already covering that.
- **`owes_nothing` is "no batch" only**, not the sketch's `taken.rows == 0`. Measured reasoning,
  not taste: the device corpus compares its per-node lists with the cpu goldens, and the cpu's
  adapter cannot drain a zero-row-build lane until task 8. Task 8 must switch **both** engines
  to the row count in one commit, and regenerate whatever cpu sections the skipped calls move.
- **Nine harness cases split by call rather than agreeing.** Same rows, different slots: the
  cpu answers in its one probe call what the session answers at finish — the build-side semi
  family over a residual (single-batch probe, cpu one legacy call) and every Left nested loop
  (DataFusion's `NestedLoopJoinExec` emits the unmatched build rows in the same call). Pinned
  `bug_` under #220 with `the_cpu_answers_in_its_probe_call_what_the_device_finishes`
  (`tests/gpu_tests/script.rs`): the cpu's finish slot empty, the device's not, every row of
  the calls together the same (nullability relaxed — one export calls a column non-nullable
  when none of its rows is NULL). Rust-only twin:
  `wire::tests::bug_the_cpu_answers_a_filtered_build_side_semi_join_in_its_probe_call`, and
  `the_recipe_and_the_executor_take_the_same_path_through_every_cell` skips those three cells.
- **No live-join count exists** for `dropping_a_probing_join_releases_its_session`; it asks the
  device to finish the dropped id and reads `unknown join N`.
- The plan's step 6 counted the #59 residual pins among those that flip to `same`; the two with
  a residual are among the nine above instead.

**Pins, by ticket.** `join_cases.rs` 43 `bug_` → 7 (#152 ×26, #173 ×4 and #59 ×7 flipped to
the positive case on the same script; #212 ×3 became `bug_*_with_no_build_batch_is_refused_on_the_cpu`
over `no_build_then_a_probe()`, asserting the device's 16 padded rows and the cpu's `NO_BUILD`;
#220 ×3 new; #243 unchanged). `join_dimension_cases.rs` 3 → 0 (#152 ×2, and the #59 composite
pin now `a_left_anti_join_on_a_composite_key_with_a_null_second_column_keeps_its_rows`, 30 of 32
by hand beside `same`; `crossing_projection` has real Left/Full arms). `nested_cases.rs`: #152 ×2
flipped, #215's refusal gone (the device answers; the case is one of the four #220 pins); #207's cross projection and #215/#190's projected Left kept their cpu halves;
#220 ×4 new; #246 new — `bug_a_like_against_a_column_pattern_is_refused_on_the_device`, the pin
deferred from task 2, green on the device. `nested_schema_cases.rs`: #207 flipped to
`a_cross_join_with_a_projection_declares_the_kept_columns_the_device_holds`.
`wire/gpu_tests/mod.rs`: the walk drives sessions per lane; `driven()`/`PROVEN` name
`FbKind::Join`; the one-`HashJoin{LeftSemi}`-call test became
`a_semi_join_with_a_residual_the_ast_cannot_take_is_one_session` (build, probe, finish).
Consts gone: `BUILD_COPY`, `PROBE_COPY`, `NO_KEYS`, `INNER_ONLY`, `device_answers_as_if_null_equals_null`.
`NO_BUILD`, `TWO_OF_SIXTEEN`, `THREE_OF_SIXTEEN` stay for task 8.

**Goldens moved, and only these.** The 15 `*.plans.txt` — with every `--- recipes ---`
section stripped, all 15 are byte-identical to `84561b80`; inside it, after `#N`
normalization, the only changed lines are join lines: 75 per tpch mode, 656 per tpcds mode,
27 per pbench tp1 mode (design §5.7's 75 and 656 exactly). `recipe-payloads.txt` regenerated
(`UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1`): 268 `CudfJoin` addresses where 272
old-kind ones were. No `.cpu.txt`, `.cost.txt`, `.result.txt`, `gpu-result.txt` or registry
change. One trap: the regeneration run reports
`the_payload_golden_covers_every_kind_and_call_shape_the_modes_produce` red because it reads
the goldens mid-rewrite; a plain rerun afterwards is green.

**Cost gate:** `cargo run -q -p cost-report -- --cost-diff --base 84561b80` → 703 compared,
0 changed, 0 regressions.

**Red-green evidence. Eleven mutations, each applied with an asserted single match, each
restored from a saved copy and checked with `cmp`, the final tree read back by grep.**
Rust-only, `--lib -- wire::tests` (20 cases):
- M1 `writer.node(0, …)` → `node(2, …)` (the join takes child stubs): 6 failed, the leaf case
  among them. (A first attempt matched nothing after rustfmt reflowed the line and reported 20
  passed; it is not counted.)
- M2 every join finishes: 4 failed, the recipe-line and same-path cases among them.
- M3 projection never written: 1 failed, `every_join_node_writes_one_cudf_join_leaf`.
- M4 `chunk_bytes: 0` always: 1 failed, `a_budgeted_join_carries_its_chunk_budget`.
- M5 the side dropped from the #249 message: 1 failed, the list pin.
- M6 the empty-pattern prefix rule removed: 1 failed, `a_join_recipe_line_is_the_session`.
- M7 the repeated-target rule removed: 1 failed, the same.
- M8 the 1 GiB cap removed: 1 failed, the budget case.
Device, `peacockdb_core_gpu_lib`:
- D1 `owes_nothing` ignoring whether a batch came: 132 failed across the harness.
- D2 `Drop` releasing nothing: 1 failed, `dropping_a_probing_join_releases_its_session`.
- D3 `matched` priced at 0: 1 failed, the residency case.

**A fifth workstation trap, paid for this round: `rsync -a` can leave a mutation compiled.**
It copies the local mtime, and a file restored locally after the mutated copy was synced and
built carries an mtime older than that build, so cargo calls the crate fresh. The first
"clean" device run after the mutations failed exactly D3's assertion (`left: 64`). `touch` the
restored file on the remote before the clean build; the rerun recompiled four binaries and
went green.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S,
0 MiB in use; cuDF 25.02 (`CUDF_VERSION_MAJOR 25`, `MINOR 2`); tree synced uncommitted,
`./scripts/build-test-shadgpu.sh --build`, each binary run directly with `--test-threads=1`.
**821 cases, all green** (the 816 baseline plus five):

| binary | result |
|---|---|
| `peacockdb_core_gpu_lib gpu_tests::` | **585 passed**, 0 failed |
| `test_gpu_corpus` | **95 passed** |
| `test_node_timing` | **1 passed** |
| `peacock_gpu_benchmarks --skip bench_` | **8 passed**, 3 filtered out |
| `peacock_gpu_tests` / `peacock_plan_tests` / `peacock_join_session_tests` | **2 / 77 / 53 passed** |

Not run, per the override: `peacock_cpu_tests` (local), the sf40 pair, the `bench_` cases. The
C++ binaries were not rebuilt (no C++ changed). Disk: 12 GB free of 96 after this round's
builds (14 at dispatch); nothing deleted.

**Local, rust-only, fresh after every edit:** `cargo test --features rust-only -p
peacockdb-core --lib --tests -- --test-threads=2` — `--lib` 724 passed, 2 ignored;
`test_cpu_corpus` 983; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18;
`test_ci_coverage` 11; `test_golden_format` 43; 0 failed anywhere.

**Warnings.** None new. `cargo build --features rust-only -p peacockdb-core` and the rust-only
lib tests are clean. The gpu-feature lib test build has one, pre-existing in an untouched file
(`aggregate_dimension_cases.rs`'s unused `AsArray`). `cargo-cudf.sh build -p peacockdb-core
--features gpu` and `-p peacockdb` report 26 dead-code warnings, every one in a file this round
did not change (`executor/mod.rs` 241–920, `driver/{measurements,mod}.rs`, `instrument.rs`,
`driver/index.rs:171`): test-support items unused by a non-test build, pre-existing.

**Formatting.** Files rustfmt-clean at HEAD were formatted whole through a temp copy (so `mod`
children are not followed); in the rest only this round's hunks were fixed. What rustfmt still
reports in touched files is pre-existing: `gpu_backend/backend.rs` (4 hunks),
`gpu_tests/{abi,accumulate,exec,mod}.rs`, `join_dimension_cases.rs:192`.

**build-test.md counts (coordinator's file).** `--lib` 730 → 726: "Recipes per join type"
24 → 20 (its prose describes the recipe seq sets that are gone; it is now the `CudfJoin` each
node writes and the session line). Device `--lib -- gpu_tests::` 580 → 585: "Executors on a
device" 32 → 36 (prose: the session lifecycle, `Drop`, `owes_nothing`, the journal and region
per call, residency and pair scratch — not "Inner at one probe batch … the refusals");
"Operator harness" 355 → 356 (#246). cpu block 1742 → 1738, gpu block 687 → 692, Rust 2549 →
2550, grand total 3119 → 3120. "Recipe walk on a device" stays 21; its "the inner and semi
joins' one call" is now a session's build, probe and finish.

**architecture.md sentences this round falsified** (the coordinator's to correct): "Four of
the sixteen wire kinds have no writer … `CudfJoin`, whose writer lands with the join backend"
(now the three old join tables have none); the "From node to seqs" rows for `GpuHashJoin` and
`GpuCrossJoin`, `GpuNestedLoopJoin`; Joins' opening "Everything below is `join.cpp` and the
recipe choreography, which still serve every plan" (the device is on the session; only the
cpu still runs the decomposition); "What a streamed probe costs" and the #152/#136 rows of
"What the frozen surface costs", true now of the cpu alone; the "Three refusals" paragraph's
#173 and #212 device halves.

**What task 8 inherits.** Flip the #220 ×7 pins (four `nested_cases.rs`, three `join_cases.rs`)
and the wire twin back to agreement, and drop `the_cpu_answers_in_its_probe_call_what_the_device_finishes`
with its last user; the #212 ×3 `_refused_on_the_cpu` pins to `*_pads_every_probe_row` (or
`returns`); #190/#207/#208 pins as the plan lists. Switch `owes_nothing` to the row count on
**both** engines together (above). Delete `cpu_backend/backend.rs`'s `one_batch`,
`CpuJoin::without_build_allowed`, `CpuProbingJoin::build`, `feeds_owing_build`, the
scatter-drop exception and `dimensions.rs`'s use of it.

#### Round 2, plan task 8 — the cpu on one DataFusion stream per lane

Rust only in the code it changes, plus one line of the device's (`owes_nothing`), proved on
nebius-gpu. Closes the cpu halves of #220, #212, #190, #207 and #208.

**What shipped.**
- `cpu_backend/join.rs`, rewritten. `CpuJoin` holds a `join::Plan`: the DataFusion join built
  once over placeholders (`HashJoinExec` with the node's projection and `CollectLeft`,
  `NestedLoopJoinExec` with the node's projection — #190 — or `CrossJoinExec`), and for a
  projecting cross join a `ProjectionExec` over the crossed schema (#207). `set_build(Option)`
  feeds the build side as a replayed stream (none is an empty build table, #212) and the probe
  side through an unbounded `futures` channel, then executes once. A probe call sends its batch
  and reads every chunk the stream has ready (`now_or_never` until pending); the finish drops the
  sender and collects to the end. Every call answers one batch, concatenated and relabelled with
  `declared_as`, a zero-row one where DataFusion yielded none (#208); `None` per probe for
  LeftSemi, LeftAnti and LeftMark and at finish for the four non-finishing types, each checked
  to have dropped no row. `owes_nothing` is `build rows == 0 && empty_build_owes_nothing(type)`,
  a cross join reading as Inner. `CpuJoin::hash` still calls `node.capability()?`, so the #153
  and #159 refusals stand on the cpu until task 9.
- `single_node.rs`: `fed(node, streams)`, the child-replacing half of `execute_single_node`,
  which both now call. `StreamSourceExec` already took any stream, so no `ChannelExec`.
- `backend.rs`: `one_batch` gone, the trait impls delegate. A probing join's residency is its
  build side; a probe call's scratch is its batch alone, since nothing re-reads the build side
  per call any more.
- Gone: `Calls`, `key_project`, `finish_join`, `finish_project`, `pad_project`,
  `without_build_allowed`, `CpuProbingJoin::{build, accumulated, accumulated_bytes,
  probe_reads_build}`, `cpu_backend/join/tests.rs`; in `plan/`, `JoinCapability::answers_in_one_call`,
  `per_call_join_type`, `finish_join_type`, `emits_both_sides` (dead with the decomposition, and
  on the spec's step-9 list); in the driver, `IndexedNode::feeds_owing_build`,
  `PlanIndex::feeds_owing_build`, `index.rs`'s climb and its test, and the scatter-drop
  exception — every zero-row scatter output is dropped, and an owing lane gets `set_build(None)`.
- `owes_nothing` is the row count on **both** engines: `gpu_backend/join.rs` reads
  `taken.rows == 0`, the mock reads its build rows, and `executor/mod.rs`'s trait doc says so.
- `has_finish_pass` (the `TEST_ONLY_ITEMS` pair) now asks the executor: `set_build(None)`, then
  whether the finish answers a batch.

**Divergences found, and how each resolved** — every one by the cpu mirroring the device:

| divergence | device | cpu before | resolution |
|---|---|---|---|
| filtered build-side semi family | answers at finish | answered in its one probe call | cpu at finish (#220) |
| Left nested loop | pads at finish | padded in the probe call | cpu at finish (#220) — q9, `scalar-subquery-cross`, `nested-loop-left-join` goldens moved |
| absent build, Right/Full/RightAnti | pads every probe row | refused (#212) | cpu runs over an empty build table |
| cross join over a zero-row build | one zero-row table | nothing | cpu answers one zero-row batch (#208) |
| nested-loop and cross projections | applied | dropped, refused by width | applied (#190, #207) |
| a zero-row build batch | probed (no batch only drained) | probed | both drain, on the row count |
| zero-row scatter output under an owing build | kept (shared driver) | kept | dropped on both; the lane gets `set_build(None)` |

None is left open, and no ticket was filed.

**What the plan got wrong.**
- **No `channel.rs`.** `single_node.rs`'s `StreamSourceExec` hands out any stream, so a
  `ChannelExec` would have been its copy. The build side is a replayed stream too, not a
  `MemoryExec`. `CpuJoin` holds a plan template and `with_new_children`, not a closure.
- **The finish drains with `block_on`, not `now_or_never`**: nothing more will arrive, so
  waiting is correct there and polling is not.
- **The goldens barely moved.** Step 6 expected the cpu goldens to move for "one batch per call,
  none per probe". Task 6's adapter already concatenated, so only two shapes moved, both below.
- **#208's harness pins cannot show the zero-row batch.** Over a zero-row build a cross join
  owes nothing on the row count, so both engines drain and the harness sees no slot. The two
  pins flip to agreement; the zero-row batch itself is pinned at node level,
  `cpu_backend::tests::join::a_cross_join_over_a_zero_row_build_answers_one_zero_row_batch`.
- **The accounting moved and the plan is silent on it.** `an_inner_joins_probe_is_charged_the_build_side_it_reads`
  became `an_inner_joins_probe_is_charged_only_its_batch`; the ignored #182 case's comment
  explained the old double charge and now does not.
- **Task 14 step 1's cpu cells landed here**, at the dispatch's request: tpch q11 and q22, tpcds
  q24 and q54 at every mode (#190 ×20) and q77's three tp4 cells (#212 ×3) — the estimate's 23
  exactly. Registry first, then `.inc`, then `PCK_UPDATE_SECTIONS=1` on those cases. Every cell
  passed against DataFusion and every DuckDB case is green: q22, q24 and q54 under
  `duckdb_exact`, q11 under `duckdb_fingerprint` (27,604 rows, over the cap), q77 unchanged.
  q11's `gpu_oracle` had to move `golden_exact` → `live_cpu`, as tpch q16's is:
  `each_declarations_two_oracles_suit_each_other` refuses a golden oracle over a fingerprint.
  The gpu cells and the `tickets` column are untouched (190 and 212 stay on those rows): task
  14 decides the gpu cells and strikes tickets.
- **A fixture that was wrong all along.** `wire/tests.rs`'s `join()` declares `[k, label, fk, v]`
  for all nine types. Invisible while `has_finish_pass` read a field; the behavioural reading
  runs a finish and `declared_as` refuses a two-column LeftSemi answer. The two cell tests use
  `typed_join`, which declares what each type emits, the mark `Boolean`.

**Goldens moved, by kind.** No `.plans.txt`, `recipe-payloads.txt`, `gpu-result.txt` or
`duckdb-result.txt`.

| kind | sections | what moved |
|---|---|---|
| `.cpu.txt`, Left nested loop at finish | tpch `nested-loop-left-join` ×5, tpcds q9 ×5, pbench `scalar-subquery-cross` ×2 | `[[51]]` → `[[50,1]]` and `[[1]]` → `[[1,0]]` up to the unload |
| `.cpu.txt`, zero-row scatter output dropped | pbench `sparse-build-{right,full,anti}` ×3 tp4, tpch q16 ×3 tp4 | an owing build side's empty lanes, `[0]` → `[]` at the emit and the coalesce |
| `.cpu.txt` + `.cost.txt`, new cells | tpch q11, q22; tpcds q24, q54 at five modes; tpcds q77 at three | new sections |
| `.cost.txt` | `nested-loop-left-join` ×5 | +46 bytes, the finish's own batch |
| `mini.result.txt` | tpch q11 (fingerprint), q22; tpcds q24, q54 | new sections; q77's author mode `tp1-rowgroup` → `tp4-sized`, rows unchanged |
| `cost-registry.csv` | 5 rows | 23 cpu cells `enabled`; `--numstat` 5/5, no CRLF |

A second whole-tier `UPDATE_CANONICAL=1` run left every golden byte-identical. The owes-nothing
switch moved no golden: no enabled corpus lane gets a zero-row build batch.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base a9ae581c`: 703 compared, 5
changed, **5 regressions, none over 10%** — `nested-loop-left-join` at every mode, +46 bytes
(+0.19%, +0.17% at tp4-single). The 23 new sections have no base and are omitted.

**Red-green evidence.** Red first, against the old executor: the eight new cases in
`cpu_backend/tests/join.rs` all failed for the reason each names (chunks > 1, a semi-family probe
answering, a zero-row probe answering two batches, `owes_nothing` false over a zero-row build,
#212 refused, both projections refused by width, the cross join answering nothing);
`an_inner_joins_probe_is_charged_only_its_batch` 1384 against 64; the two rewritten flow cases
(`a_zero_row_scatter_output_is_dropped_under_every_join_type`,
`holds_and_releases_balance_over_a_plan_with_empty_lanes`). The new flow case's first red was my
script's — `Script::source` appends, so a second `build` entry is never read — and M9 is its
proof. Then mutations, each applied with an asserted single match, restored from a saved
copy and checked with `cmp`:
- M1 `owes_nothing` on build bytes (batch presence): 1 failed, the zero-row build case.
- M2 the semi family answering per probe: 2 failed.
- M3 the first chunk only: 3 failed, the one-batch case among them.
- M4 the nested loop's projection not passed: 1 failed (#190).
- M5 the cross projection dropped: 1 failed (#207).
- M6 `None` for a call with no chunk: 1 failed, the #208 case alone. **DataFusion yields a
  zero-row chunk for a zero-row hash or nested-loop probe**, so the `new_empty` path is guarded by
  the cross case only.
- M7 no finish answering: 14 failed, both wire cell tests among them.
- M8 empties kept at the scatter: 8 failed, `dimensions`' two among them.
- M9 the mock's owes-nothing on batch presence: 1 failed, the new flow case.
- M10 the #212 refusal reinstated in `set_build`: 6 failed — the #212 case, `dimensions`' two
  and `tests::end_to_end::{tpcds_q93, tpcds_q97, tpch_anti_join}`.
- D1, device, `owes_nothing` on `handle == 0`: 18 failed in `peacockdb_core_gpu_lib`, every
  `*_over_a_zero_row_build_*` and `*_over_both_sides_empty_*` case of the six owes-nothing types,
  the cross and nested-loop ones and `join_dimension_cases`' crossing-projection LeftAnti. That
  set is the device's witness for the row-count switch. Restored, touched on the remote, rebuilt.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S,
0 MiB in use; cuDF 25.02. Final tree synced, `./scripts/build-test-shadgpu.sh --build` rc 0, each
binary run directly with `--test-threads=1`. **821 cases, all green**, unchanged from task 7:
`peacockdb_core_gpu_lib gpu_tests::` 585, `test_gpu_corpus` 95, `test_node_timing` 1,
`peacock_gpu_benchmarks --skip bench_` 8, `peacock_gpu_tests` 2, `peacock_plan_tests` 77,
`peacock_join_session_tests` 53. Disk 12 GB free during the round's builds, 28 GB at its end;
I deleted nothing, so something outside this task freed the difference.

**A sixth workstation trap: the board's rsync line deletes `.run-state/`.** It is not gitignored,
so `--delete-after` removes it, and a detached build logging there never starts — the redirect
fails and nothing reports it. Log outside the tree (`~/t8-build.log`).

**Local, rust-only, fresh on the final tree:** `cargo test --features rust-only -p peacockdb-core
--lib --test test_cpu_corpus --test test_corpus_goldens --test test_cost_model --test
test_module_layout --test test_ci_coverage --test test_golden_format -- --test-threads=2` —
`--lib` 731 passed, 2 ignored; `test_cpu_corpus` 1006; `test_corpus_goldens` 26;
`test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage` 11; `test_golden_format` 43.

**Warnings.** None new. rust-only build and test builds: none. The gpu lib test build: the one
pre-existing `AsArray`. `cargo-cudf.sh build -p peacockdb-core --features gpu`: the same 26
pre-existing, by file and item (`nodes_as_recorded` moved from `index.rs:171` to `:138`).

**Formatting.** `cpu_backend/join.rs` and `cpu_backend/tests/join.rs` formatted whole through a
temp copy; every other touched file is either rustfmt-clean or reports exactly the hunks it
reported at `a9ae581c`.

**build-test.md counts (coordinator's file).** cpu block 1738 → 1768: `--lib` 726 → 733,
`test_cpu_corpus` 983 → 1006. Rust 2550 → 2580, grand total 3120 → 3150. Device block unchanged.
Rows, with the prose now false:
- "Corpus, cpu" 982 → 1005: "144 of them running" → 148, `tpcds/q77` no longer among the
  partly disabled; "33 lines are out entirely … `tpch/q11`, `tpch/q22` and `tpcds/q24` on #190,
  `tpcds/q54`, and 29 of pbench's" → 29, all pbench's; "703 cells" → 726; "110 lines are
  `duckdb_exact` … 14 `duckdb_fingerprint` … 33 `duckdb_none`" → 113, 15, 29.
- "CPU backend executors" 72 → 79: "a build side that produced no batch at all is refused by
  name for the three types that owe their probe rows" is now "is joined over an empty build
  table"; add one batch per call however DataFusion chunks it, nothing per probe for the
  build-side semi family, a zero-row probe answered by a zero-row batch, both projections, the
  cross join over a zero-row build, and a zero-row build owing what none owes.
- "Drivers over a mock backend" 109 → 110: "the empty build lane a join owes rows for reaching
  `SetBuild` while every other empty lane still drops" → every empty scatter lane drops, and an
  owing lane reaches `SetBuild` with no batch; plus a zero-row build draining like none.
- "Driver internals" 44 → 43: "which lanes feed a build side that owes rows" goes.

**architecture.md sentences this round falsified** (the coordinator's to correct): the
capability matrix's Left outer ("the accumulated probe keys are resident"), build-side semi
family ("the per-call join disappears: a probe call is only the key project") and nested-loop
Left ("the finish trick accumulates keys") rows; "What the finish pass computes" (both engines
are sessions now); the hash-skew paragraph's "One exception: a scatter feeding the build side of
a Right, Full or RightAnti join keeps its zero-row table … The index decides it"; "A
build-preserving join's residency grows with the probe side"; "except the scatter lane under a
join that owes its probe rows"; the determinism breaks' "Right, Full and RightAnti refuse when
the build side sends no batch (#212)" and "a nested-loop Left make[s] no call over no probe
batch"; and the "Three refusals" paragraph's #212 clause. Task 7's list still stands beside
these, and its "true now of the cpu alone" items are now true of neither engine.

**Drift found, not mine.** `planner/tests/plan_goldens.rs:178-191`'s reasons for the payload
subset describe the recipe task 7 retired ("a key project per batch", "one legacy call that hands
the build side over").

**What plan task 4 inherits.** The cpu applies nested-loop and cross projections, which the
placeholder's drop through a join projection needs, and tpcds q9's cpu cells are green with its
Left nested loops in the session's shape. Four more lines carry cpu cells task 4's regeneration
will rewrite if they hold a zero-column projection: tpch q11, q22, tpcds q24, q54.

**What plan task 9 inherits.** On the cpu the #153 and #159 refusals are `CpuJoin::hash`'s one
`node.capability()?` line and `the_filtered_column_refuses_five_cells_by_ticket`; the stream
answers every type with a residual as DataFusion's own join, and `NestedLoopJoinExec` takes
every type. The planner's probe-side coalesce is no longer needed by either engine.
`JoinCapability.probe_streams` is still read by the planner and the wire.

#### Round 2, plan task 4 — `__rowmarker__` is a column the plan declares (#63)

Planner, validation, both scans, the wire's `CudfScan.rows_only`, and `project.cpp`'s refusal.
An fbs change, so the rust-only tier was re-proved in full. Closes #63's mechanism; its device
cells stay off for task 14.

**What shipped.**
- `plan/mod.rs`: `ROWMARKER` and `Schema::rowmarker()` (`[__rowmarker__: Int8]`, not nullable);
  `GpuLoadParquet::rows_only()` (the empty projection); `emitted_sides(join_type)`, the side half
  of `join.rs`'s `emits`.
- Translator: a `ProjectionExec` of no expression plans `GpuProject [0 as __rowmarker__]`; a scan
  of no column declares the marker itself. `translator/common.rs`'s `past_rowmarkers` renumbers a
  keyless join's projection past a marker side and drops the marker, or keeps one alone where the
  join would keep nothing; the cross and both nested-loop arms call it.
- Validation: no node declares no column (`declares_a_column`, in the walk); a rows-only scan
  counts one column in `declared_width` and must declare the marker (`source.rs`). The scan's
  nullability leaf is `[false]`.
- `planner/nullability.rs`: the `CrossJoin` arm reads its projection, through the rule the
  nested loop uses. It ignored it, harmless while no cross join had one; this task gives cross
  joins one, so the arm reported the marker's `false` for the column kept, which could skip a
  #137 filter above it. Found by reading task 5's note, not by a test; red first.
- cpu: `CpuSource` answers a rows-only read as one `Int8` zero a row of the rows it read.
- Wire and device: `CudfScan.rows_only: bool = false`, appended; `node_writer.rs` sets it,
  `fb_text.rs` renders it. `scan.cpp`'s `rows_of` reads the footer (`read_parquet_metadata`),
  sums the chosen row groups' `num_rows`, caps at `limit`, and builds the column. Nothing is
  decoded. The row-group block moved above the arm. `project.cpp`'s empty-expression arm
  throws `CudfProject with no expression: the plan declares __rowmarker__ (#63)`, before
  `take_input`, so the refusal consumes nothing.

**What the plan got wrong.**
- **pbench carries the shape too.** `scalar-subquery-cross`'s three marker projects (q9's shape
  by construction) and `cross-projection`'s scan of no column, at every mode. The plan named
  tpcds and tpch only.
- **The payload golden would not have shown `rows_only`.** nested-limits was not in the payload
  subset, and the coverage check reads kinds and call shapes, not fields. So it is the 21st
  payload query, with the reason beside it.
- **Step 1(a)'s SQL cannot run.** The minimal dataset has no `lineitem`, and DataFusion answers a
  one-table `count(*)` from statistics (#158). The case uses `count(*)` over a join, q88's shape.
- **The gtest's nation scan proves nothing about row groups.** nation is one row group, so "that
  group's count" equals the whole. The case reads customer: 150,000 whole, 27,120 for group 1,
  10 under `limit=10`.
- **Only keyless joins meet a marker.** A keyed side has a column, and DataFusion never embeds an
  empty projection into a join (`try_embed_projection` returns `None` for no indices). So step
  3's "task 9 makes the projection apply on every type" does not bear on the marker. Both
  engines already apply the keyless projections: the device from task 7, the cpu from task 8.
  Four joins per mode set gain a projection: tpcds q9's innermost Left nested loop, pbench's two,
  and nested-limits' cross join.
- **The memory sections move.** The estimator prices the marker at its bytes. Neither the plan nor
  the design says so.
- **One divergence the plan's arm creates, not reachable today.** The device's rows-only read
  caps at `limit` per call, as `set_num_rows` does for a column read. The cpu reader ignores a
  scan's limit (#186, pinned by `bug_a_limit_over_one_row_group_is_ignored_on_the_cpu`). The two
  differ only where a limit is under a batch's rows. nested-limits' region scan is `limit=23` over
  5 rows, and its device cells are off on #186. chain K's limits task takes the limit out of both
  readers, and this line goes with it.

**Goldens moved, by kind, checked arithmetically.** No `.result.txt`, `duckdb-result.txt`,
`gpu-result.txt`, `cost-registry.csv` or `corpus_cases.inc`: no answer moved.

| kind | files | what moved, per mode |
|---|---|---|
| `*.plans.txt` tpcds | 5 | 19 marker projects; q9's innermost nested loop gains `projection=[count(*)@1]`; 1–2 pass-throughs; 24–31 `--- memory ---` lines |
| `*.plans.txt` pbench | 5 | 3 marker projects, 1 rows-only scan, 2 keyless joins with a projection, 2–4 pass-throughs, 11–13 memory lines |
| `*.plans.txt` tpch | 5 | nested-limits: the rows-only scan, its coalesce, the cross join's `projection=[k@1]`, 4 memory lines |
| `recipe-payloads.txt` | 1 | nested-limits added, `rows_only: true` on its region scan; q87's `#86 CudfProject` gains `__rowmarker__: 0` and a new digest |
| `*-mini.cpu.txt` | 15 | 33 sections, every enabled cpu cell of the nine queries |
| `*-mini.cost.txt` | 15 | derived from those 33 |

Read by a script, not by eye. In the plan goldens every changed line is one of five kinds: a
marker project, a rows-only scan, a pass-through over one (`GpuCoalesceAllBatches`,
`GpuMergePartitions`), a keyless join, or a memory line. Nothing else changed, and no
`schema=[]` is left in any golden. In the `.cpu.txt` files the node-kind counts are identical
before and after, and so is every `output_rows`, `batch_rows` and `in_rows`. Only bytes moved,
on 136 marker-project lines, 20 scan lines, 34 coalesce and 8 merge lines, and 17 keyless-join
headers (the projection text only; their bytes did not move). Arithmetic: tpcds tp1 is 19
projects × 2 lines = 38, tp4 is 8 × 2 = 16, since q88, q90 and q96 run at tp1 only. The marker
costs 1⅛ bytes a row, an `Int8` and its validity bit (550,263 rows → 619,046 bytes).

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base 29937e22`: 726 compared,
33 changed, **33 regressions, none over 10%** — the 33 sections above, by `peacockdb_cost`:
tpcds q9 +3,094,392…3,094,446 bytes (+0.33%) at all five modes; q87 +53,211…53,212 (+0.01–0.03%)
at five; q88 +43,877 and +43,948 (+0.01%) at tp1; pbench `scalar-subquery-cross` +11,254 and
+11,259 (+0.94%) at tp1; q96 +984 and +994, q90 +355 and +358, q38 +121…122, pbench
`cross-projection` +18 (tp1) and +27 (tp4), tpch nested-limits +12 — all +0.00% at two decimals.
Pre-accepted by the board.

**Red-green evidence.** Red first: seven rust-only cases, each for its own reason. The three
translator cases found `exprs=[]` and `schema=[]`; the two-marker cross join had no projection;
validation passed a project of no expression, and refused a rows-only scan as "declares 1
columns and the columns it reads produces 0"; and the cpu reader answered zero columns. Two
added later went red first as well: the cross join's nullability (`[false, true]` against
`[true]`) and the marker scan's (R8 below). Then mutations, each applied with an asserted single
match against the whole `--lib` tier, restored from a saved copy and checked with `cmp`. The
whole working diff was snapshotted before the run and compared byte for byte after.
- R1 the project arm off: 17 failed — three translator cases, ten plan goldens with the payload
  golden, `tests::end_to_end::{tpcds_q38, tpcds_q87, a_two_key_group_by_over_many_rows_…}`.
- R2 `past_rowmarkers` a no-op: 19 failed, but **none of the translator join cases**. They read
  declared names, and `validate_all` runs each node's own check, not the walk's
  `declared_width`. Tightened: each now asserts the projection and runs `plan::validate`. Re-run,
  3 of 7 red where 0 were.
- R3 the keep-one-alone branch off: 1 failed, the two-marker cross join.
- R4 `declares_a_column` off: 1 failed, the project refusal.
- R5 the cpu reader not rows-only: 3 failed — its case, `tests::end_to_end::tpch_nested_limits`
  and `limits::a_limit_slices_at_most_two_batches_and_stops_the_scan`.
- R6 `rows_only` not written: 1 failed, the payload golden.
- R7 the scan's marker check off: 1 failed.
- R8 the scan's `[false]` leaf off: **0 failed**. Added
  `a_scan_of_no_column_says_its_marker_holds_no_null`; re-run, 1 failed.
- R9 the scan arm declaring DataFusion's empty schema: 14 failed.
- R10 `declared_width`'s rows-only arm off: 14 failed.
- Device, both in one build since they redden different tests: D1 the `rows_only` arm off
  (`if (false)`): `ScanRowsOnlyAnswersTheRowCount` and `a_rows_only_scan_agrees_on_both_backends`.
  D2 the project refusal off: `AProjectOfNoExpressionIsRefused` (the empty table is then refused
  by `register_handle`, which does not name the marker). 3 failed, 77 and 586 passed. Restored,
  `cmp`-checked, touched before the re-sync, and the next build re-ran both gtests green.
- `the_rowmarker_project_answers_a_zero_a_row_on_both` (`exec_cases.rs`) has no mutation. It pins
  what cuDF's `compute_column` makes of a bare `INT8` literal, the device path every marker
  project takes. Nothing in this task's code could be broken to redden it.

**The device run, recorded.** nebius-gpu, NVIDIA L40S, 0 MiB in use; cuDF 25.02; final tree
synced, `./scripts/build-test-shadgpu.sh --build` rc 0, each binary run directly with
`--test-threads=1`. **826 cases, all green**: `peacock_gpu_tests` 2, `peacock_plan_tests` 79
(+2), `peacock_join_session_tests` 53, `test_gpu_corpus` 95, `test_node_timing` 1,
`peacockdb_core_gpu_lib gpu_tests::` 588 (+3), `peacock_gpu_benchmarks --skip bench_` 8. Not
run, per the override: the sf40 pair and the `bench_` cases. Disk 28 GB free of 96 throughout;
nothing deleted.

**Local.** `peacock_cpu_tests` 15 passed, from `/tmp/dkb-cppbuild`. It had gone, so it was
re-configured: `cmake -S cpp -B /tmp/dkb-cppbuild -G Ninja -DCMAKE_BUILD_TYPE=Release
-DCMAKE_CUDA_ARCHITECTURES=89`, gcc-12 and nvcc from the rapids env as compilers, ccache as
the launcher, `-Dcudf_ROOT=` that env, `link_pool=1`, then `--target peacock_cpu_tests`.
Rust-only on the final tree: `--lib` 740 passed, 2 ignored;
`test_cpu_corpus` 1006; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18;
`test_ci_coverage` 11; `test_golden_format` 43.

**Warnings.** None new. rust-only build and test builds: none. On nebius,
`cargo-cudf.sh build -p peacockdb-core --features gpu` gives the same 26 pre-existing dead-code
warnings, none in a file this task touched. The gpu lib-test build gives the one pre-existing
`AsArray`. `build-test-shadgpu.sh`'s log shows no warning at all, since it builds with
`--message-format=json`; re-running the same build through `cargo-cudf.sh` replays them.

**Formatting.** `cpu_backend/tests/source.rs`, `translator/{common,nodes,tests}.rs` were
rustfmt-clean at HEAD and were formatted whole through a temp copy. Every other touched `.rs`
reports exactly its HEAD hunk count. The C++ hunks went through `git clang-format --diff HEAD`,
applied by hand, since applying it needs the files staged.

**build-test.md counts (coordinator's file).** cpu block 1768 → 1777: `--lib` 733 → 742.
"Translator, one rule at a time" 33 → 37; "Plan types" 38 → 40; "CPU backend executors" 79 → 80;
"Null analysis rules" 9 → 11. gpu block 692 → 695: `--lib -- gpu_tests::` 585 → 588; "Operator
harness" 356 → 359. "Plan-executor (C++)" 77 → 79, so C++ 169 → 171. Rust 2580 → 2592, grand
total 3150 → 3164. Each row was counted off `--list`, not by adding deltas. Prose now short of
the truth:
- "Plan types": its rules list lacks "no node declares no column (#63)".
- "CPU backend executors": the loader also answers a read of no column as `__rowmarker__`.
- "Plan-executor (C++)": add a scan of no column answering its row count from the footer
  (whole, one row group, under a limit), and a project of no expression refused.
- "Recipe payloads golden": "chosen as a cover over every fb kind and call shape" — one member,
  nested-limits, is there for a field, `rows_only`.
- "Operator harness": the marker project, a cross join of two marker sides, a rows-only scan.

**architecture.md sentences this round falsified** (the coordinator's to correct): the
`CudfScan` row of the payload table (`file_paths`, `projection`, `limit` …; "`scan.cpp` —
`cudf::io::read_parquet(opts)`, with `.columns(projected)` …"). It now has a `rows_only` field,
and that read decodes nothing: it reads the footer and builds the marker. And the
`CudfScan.limit` → `set_num_rows` row, which caps a rows-only read's count instead. The
`CudfProject` row is not false, but it now refuses an empty expression list.

**Drift found, not mine.** #63's ticket (`tickets/joins.md`) describes the mechanism this
round replaced: `project.cpp`'s `__rowcount__`, and `execute_cross_join`, gone with task 7. Its
"Fix proposed" (a rows-only cross-join arm) is superseded. It is the coordinator's to archive
with the chain.

**A seventh workstation trap, small.** `ssh nebius-gpu 'nohup … &'` can sit until its `timeout`
and exit 124 even with `-n` and `< /dev/null`, while the detached job runs. Read the job's own
log and rc file, never the ssh status.

**What plan task 9 inherits.** `past_rowmarkers` reads `emitted_sides`, so it already covers
every join type. Lifting #160 needs nothing more for a nested loop over a marker side, though no
SQL reaches a non-Inner, non-Left one today. Hash joins never meet a marker, so the #153 and
#159 lifts need nothing from this task. The null analysis now honours every keyless join's
projection. #63's device cells stay off for task 14 to run: tpcds q9, pbench
`scalar-subquery-cross` and `cross-projection`. Their registry rows still carry `63`.
nested-limits' device cells stay on #186: its part scan's `limit=28` is still refused beside
row groups.

#### Round 2, plan task 9 — the refusals lifted (#153, #159, #160; #59's narrowed to #250)

Planner, validation, the cpu executor's last refusal line, and tests. No C++, no fbs, no device
code: the session already answered every shape, so the device needed only harness cases.

**What shipped.**
- `plan/`: `JoinCapability`, `capability` and `GpuHashJoin::capability` gone; `needs_finish`
  (task 7's) is the one fact left beside "every probe streams". `GpuHashJoin`'s "cannot stream
  its probe" check and `GpuNestedLoopJoin`'s Left single-batch check gone. `validate.rs`'s
  `declared_width` counts a nested loop's columns by its join type, as the hash join's arm did.
- Translator: `hash_join` never coalesces its probe; `nested_loop_join` takes all nine types and
  never coalesces its probe.
- `planner/nullability.rs`: the physical `refuse_null_unsafe_joins` and `hardcodes_null_equality`
  gone, and `pipeline.rs`'s call with them. **Narrowed, not deleted:** what refuses a NULL now is
  task 2's `refuse_nullable_in_off_the_spine` (#250), which stays, beside `can_be_null`,
  `logical_can_be_null` and `in_subquery_may_meet_null`. The module doc says so.
- `cpu_backend/join.rs`: `node.capability()?` gone. `memory_estimation.rs` reads `needs_finish`.
- The two unreachable refusals stay: the mark column in a filter (`common.rs`) and a key that is
  not a bare column (`column_ordinal_of`), both still pinned in `join_capability.rs`.
- Tests: the five `join_refusals.rs` cases answer on the cpu against DataFusion through a new
  `Fixture::answers_as_datafusion`, each with a hand count (Left 10, Right 1000, Full 1005;
  RightSemi 5, RightAnti 995; anti 3, mark 8; seven nested-loop types). `join_capability.rs`'s
  matrix plans all 18 cells with the probe streamed, and its 9-type nested-loop case replaces the
  refusal. The #160 pin flipped to `a_predicate_free_outer_nested_loop_pads_its_preserved_side`.
  `plan/tests/joins.rs`: two validation refusals became one nine-type case each, a third merged
  away. `cpu_backend/tests/join.rs`: the five refused cells answer. `PBENCH_JOINS` +19.
- Device harness: `join_cases.rs` +8 (a residual on Left, Right, Full, RightSemi, RightAnti, and
  the build-side semi family over two probe batches); `nested_cases.rs` +7 (Right, Full, the
  three Left semi forms, RightSemi, RightAnti), `nested_with` building any type from
  `join_cases::output_of`, now shared.

**Divergences.** None found. All 15 new device cases agree with the cpu slot by slot.

**What the plan got wrong.**
- **A third site refused the shape.** `declared_width` counted both sides for every nested
  loop, which refused a projection-free LeftSemi nested loop as "declares 2 columns and its two
  sides produces 4". The hand-built matrix found it. No corpus query reaches the arm: every
  semi-family nested loop DataFusion plans carries a projection, which takes the projection arm.
- **`needs_finish` already existed** (task 7), and `answers_in_one_call`, `per_call_join_type`,
  `finish_join_type` went in task 8, so the Interfaces line was mostly done.
- **"8 padded rows over `empty`" cannot run over `empty`.** pbench's `empty.parquet` has no row
  group and meets #256 at plan time. The flipped pin builds the same predicate-free Right nested
  loop over `tiny` filtered empty by a predicate no footer prunes (`t_id + 1 < 0`), and asserts
  DuckDB's 8 rows for `outer-on-true-empty`.
- **The registry move is plan columns, not `plan_status`.** `plan_status` is whether DataFusion
  plans the query, and it did all along.
- **Two lifted queries meet something else.** `outer-on-true-empty` now reaches #256 (was #160).
  `not-in-uncorrelated` plans at tp1 but at the three tp4 modes fails validation inside an
  aggregate: DataFusion plans the rewrite's `count(*)` over `sub` as `mode=Single`, pbench's
  small-table rule is off so `sub` gets four lanes, and `aggregate_sequence` finalizes on four
  lanes with no merge. Not a join and out of the spec's restriction: filed as
  [#267](../tickets/corpus-coverage.md#t267). The CLI, whose small-table rule keeps `sub` in one
  lane, answers it. The two rows carry `256` and `267` beside their old tickets.
- **Comments the lift made false**, fixed here: `plan_goldens.rs`'s payload reasons for
  `nested-loop-left-join`, q21 and q22, and `end_to_end.rs`'s for the Left nested loop, q21 and
  `left_join` (the last two were drift from task 7).

**Goldens moved, by kind, checked by script.** No `.result.txt`, `duckdb-result.txt`,
`gpu-result.txt` or `corpus_cases.inc`.

| kind | files | sections per mode | what moved |
|---|---|---|---|
| `*.plans.txt` pbench | 5 | 23 | 20 lifted (19 plan; `not-in-uncorrelated` plans at tp1, #267's text at tp4); `nl-left`, `nl-left-decimal` lose a probe coalesce; `outer-on-true-empty`'s refusal #160 → #256 |
| `*.plans.txt` tpch | 5 | 2 | q21 (2 coalesces), `nested-loop-left-join` (1) |
| `*.plans.txt` tpcds | 5 | 2 | q16, q94 (1 each) |
| `recipe-payloads.txt` | 1 | 3 of 21 | `nested-loop-left-join`, q21, q16: seqs renumbered, digests |
| `*-mini.cpu.txt` tpch, tpcds | 10 | 2 | the same four queries |
| `*-mini.cost.txt` tpch, tpcds | 10 | 2 | derived |
| `cost-registry.csv` | 1 | 21 rows | 20 pbench rows' plan columns, 97 cells `disabled` → `enabled` (19 × 5 + 2); tickets on two |

The arithmetic. In every tpch and tpcds plan golden, once coalesce lines are dropped, seqs
normalized and `--- memory ---` figures set aside, the file equals the base. The one exception
is tpch tp4-sized, whose two q21 lineitem scans read one batch per lane where they read two.
The sizer derives the batch from the memory model, and the coalesce no longer costs anything.
In the `.cpu.txt` files `GpuCoalesceAllBatches` is the only node kind whose count changed. tpch
goes 76→73 at tp1 and 102→99 or 104→101 at tp4, so −3. tpcds goes 618→616, 767→765 and 775→773,
so −2. With each coalesce and its `in_rows` line dropped, nine files equal the base. The tenth
is tpch tp4-sized, which differs on those scans' batching alone; every `output_rows` holds. The
payload sections are equal once seqs, indentation, digests and the coalesce are set aside.
pbench's `.cpu.txt` did not move: every lifted cell and both `nl-left` cpu cells are off.

**Which cpu cells become enable-able: all 20 newly planned pbench queries, measured, left to
task 14.** Each was run through the CLI (rust-only, tp1 and tp4) and compared with its committed
DuckDB section: full rows for 15, rows plus per-column non-null counts for the five over the cap.
All 40 runs agree. Enabling them is task 14 step 2's: registry first, `duckdb_oracle` off
`duckdb_none`. Caveats: `not-in-uncorrelated`'s tp4 cells wait on #267; `indf-full-join`
plans as a Full nested loop over `IS NOT DISTINCT FROM`, which the device's AST refuses until 9c;
`outer-on-true-empty` waits on #256.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base d7042a8c`: 726 compared, 20
changed, **20 improvements, 0 regressions**. These are the four queries that lost a coalesce, at
five modes. q21 −16.78% at tp1 (904.54 → 752.75 MB) and −8.44% at tp4; q16 −14.77% / −4.40…4.62%;
q94 −14.60% / −4.36…4.57%; `nested-loop-left-join` −9.94% (24.16 → 21.76 KB, the 2,458 bytes the
nation coalesce emitted) and −8.90% at tp4-single.

**Red-green evidence.** Red first: 15 lib cases, each on the refusal or validation it names.
One red was mine. Both `join_refusals` and `join_capability` named a fixture `nlj-types`, so one
deleted the other's parquet mid-run. Renamed. Then mutations, each applied by an asserted single
match against the whole `--lib` tier, restored from a saved copy and checked with `cmp`. The
production diff was then read line by line.
- M1 the filtered semi family's probe coalesce back: 17 failed — the matrix case, all 15 plan
  goldens, the payload golden.
- M2 the #160 refusal back: 8 failed — both nested-loop type cases, the flipped pin, pbench ×5.
- M3 the Left nested loop's probe coalesce back: 12 failed — the nested-loop case, pbench and
  tpch goldens, the payload golden.
- M4 `declared_width` counting both sides: 1 failed, the hand-built nested-loop matrix alone.
- M5 the cpu refusing a residual on Left/Right/Full/RightSemi/RightAnti: 4 failed — the five
  cells, both SQL residual cases, the wire same-path case.
- M6 the estimator's matched column priced at 0: 16 failed — the residency case and 15 goldens.
- Device, one build: D1 = M5, D2 the cpu refusing a nested loop beyond Inner and Left. 12
  failed, exactly the 5 + 7 new harness cases, 591 passed. Restored, `cmp`-checked, touched,
  re-synced; the final build ran 603 green.

**The device run, recorded.** nebius-gpu, NVIDIA L40S, 0 MiB in use; cuDF 25.02; the final tree
synced, `./scripts/build-test-shadgpu.sh --build` rc 0, each binary run directly with
`--test-threads=1`. **841 cases, all green**: `peacock_gpu_tests` 2, `peacock_plan_tests` 79,
`peacock_join_session_tests` 53, `test_gpu_corpus` 95, `test_node_timing` 1,
`peacockdb_core_gpu_lib gpu_tests::` 603 (+15), `peacock_gpu_benchmarks --skip bench_` 8. Not
run, per the override: the sf40 pair and the `bench_` cases. The 20 newly planned pbench
queries' device cells are off, so no corpus cell of theirs ran; the node-level cases above are
the device's evidence for every type with a residual and every nested-loop type. Disk 28 GB free
of 96 throughout; nothing deleted.

**Local, rust-only, fresh on the final tree:** `--lib` 739 passed, 2 ignored; `test_cpu_corpus`
1006; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage`
11; `test_golden_format` 43.

**Warnings.** None new. The rust-only build and lib-test build have none. On nebius,
`cargo-cudf.sh build -p peacockdb-core --features gpu` (with `CUDF_ROOT` set, which the script
demands) gives the same 26 pre-existing ones, by file (`executor/mod.rs` 14, `instrument.rs` 5,
`driver/{measurements,mod}.rs` 3 + 3, `driver/index.rs` 1). The gpu lib-test build gives the one
pre-existing `AsArray`.

**Formatting.** The crate is edition 2024, and `rustfmt --edition 2021` re-sorts its imports
the other way — a trap worth the line. Every touched file was rustfmt-clean at HEAD under 2024
but `nullability.rs`. Those that gained hunks were formatted whole through a temp copy, and every
touched file is clean now. `nullability.rs` reports one pre-existing hunk where it had two.
`plan_goldens.rs` is 1,145 lines, 1,121 at HEAD and over the cap already; `PBENCH_JOINS` grew it.

**build-test.md counts, counted off `--list`.** cpu block 1777 → 1776: `--lib` 742 → 741, "Join
declarations" 24 → 23. "Planner join capability" 14, "Planner join refusals" 10, "Translator"
37 and "CPU backend executors" 80 (+1 in "Executor contract") hold. gpu block 695 → 710: `--lib
-- gpu_tests::` 588 → 603, "Operator harness" 359 → 374. Rust 2592 → 2606, grand total 3164 →
3178. Prose rewritten in this change: "Planner join capability" (every cell plans; the null
analysis' new witness), "Planner join refusals" (the five shapes that now answer), "Operator
harness" (a residual on every hash type, every nested-loop type; the #59 composite pin's
sentence, stale since task 7).

**architecture.md sentences this round falsified** (the coordinator's, or task 15's rewrite):
- the node table's `GpuHashJoin` row, "the capability matrix below" — it is two facts now;
- the matrix's build-side semi row, "the filtered forms, which take a single-batch probe";
- its "Nested-loop Left" row, "with a single-batch probe", and the absence of the seven other
  nested-loop types;
- the whole "Four shapes are refused at plan time" paragraph. The two narrower refusals beside
  it stand, and #250's off-spine `IN` is the NULL refusal now;
- "Keys rather than probe rows, because the alternative is … the single-batch probe";
- "anti and mark stay `EQUAL` until the planner distinguishes `NOT IN` from `NOT EXISTS`".

**Ticket text made stale.** #153, #159, #160, #59 and #80 each say their pbench rows are refused
"at all five modes". They now plan, and answer as DuckDB on the cpu. These close with this task.
#256's corpus line gained `outer-on-true-empty`, in this change. Drift, not mine: #282
(corpus-coverage, master's) describes #256's defect.

**A trap for the next test writer.** `join_fixture::Fixture::new(name)` makes its directory from
the process id and the name. Two modules using one name race in one test binary, and the loser
reads "Object at location … not found". Names must be unique across `planner/tests/`.

**What plan tasks 9b and 9c inherit.**
- 9b: `empty-side-left-join` is still refused on `EmptyExec`. `cross-empty-build` and
  `outer-on-true-empty` are refused on #256, whose ticket names a `GpuEmpty` as one fix. Both
  rows' DuckDB answers are committed.
- 9c: `indf-full-join` now plans as `GpuNestedLoopJoin{Full}` over `d_k IS NOT DISTINCT FROM t_k`
  and answers as DuckDB on the cpu (2,009 rows). Its device run would meet the AST's refusal. The
  promotion to a hash join moves its five plan sections and its `PBENCH_JOINS` row, which reads
  `GpuNestedLoopJoin: join_type=Full` today.
- 10: the device harness already has a residual on all nine hash types and every nested-loop
  type over two probe batches. Step 2's "per type" lists should not add duplicates.

#### Round 2, plan task 9b — a side DataFusion folds to nothing: `GpuEmpty` (review row 8)

Plan, translator, null analysis, estimator, renderer, wire, both source executors, and tests.
No C++ and no fbs change: the empty relation calls nothing, and its wire slot is the stub
`CudfScan` the writer already made for hand-built leaves.

**What shipped.**
- `plan/`: `GpuEmpty { kind }`, `GpuEmpty::new(schema)` (`plan/source.rs`, beside the scan):
  `NodeKind::Source`, one lane, `SingleBatch`, no validation of its own. `NodeRef::Empty`,
  `ExecutorCategory::Source`, name `GpuEmpty`. The registry is nineteen kinds now.
- Translator: the `EmptyExec` arm, before the refusals at the end of `node()`.
- `planner/nullability.rs`: `vec![false; width]` — no row, no NULL. That is what spares a key read
  from it #137's filter, though every field is declared nullable.
- `planner/memory_estimation.rs`: `sources()` is the scans, not every `NodeKind::Source`. It
  prices 0 rows through the leaf fallthrough that already answered 0, so it needs no arm of
  its own.
- `plan_text/node_text.rs`: no fields; the line is `GpuEmpty: lanes=1, batches=single, schema=[…]`.
- `wire/attach.rs`: no recipe, and `writer.leaf()` — the stub a parent would have taken.
- Executors: `CpuSource` and `GpuSource` each hold `read: Option<LaneRead>`. `None` is the empty
  relation's lane, which answers `None` at its first `next_batch` and calls nothing.
  `GpuBackend::executors_for` answers it before the recipe lookup, as it answers the forwarders.
- `testdata/cost_model.conf`: `GpuEmpty` under `storage_read_bytes`, the source category, where
  it always counts 0 bytes. `test_cost_model` requires every kind in exactly one category.
- Tests (rust-only, +7): translator ×2 (the leaf under the swapped Right join at tp4; `WHERE false`
  as `GpuUnload` over the leaf alone), `end_to_end!(pbench, empty_side_left_join)` at five modes
  against DataFusion, `an_empty_relation_alone_answers_no_batch` at five modes, the estimator's
  `an_empty_relation_is_not_a_source_the_budget_sizes`, the wire's
  `an_empty_relation_is_a_stub_with_no_recipe`, and `an_empty_relation_says_no_column_holds_a_null`.
  Grown: `rebuild::every_kind` (two fixtures, nineteen kinds), `cpu_backend/tests/backend.rs`'s
  kind list, `plan_goldens.rs`'s recipe-numbering check (a `GpuEmpty` makes no call either), and
  `PAYLOAD_QUERIES` (+ pbench `empty-side-left-join`, 22 queries over three datasets).
- Device (+2, `source_cases.rs`): `an_empty_relation_emits_no_batch_on_either_engine` (the
  harness's `GpuEmpty` case, which the kind guard requires), and
  `a_left_join_over_an_empty_relation_pads_every_row_on_both_engines` — pbench's query planned at
  five modes and run whole through the driver on both engines, eight padded rows each.

**Divergences.** None found. The empty lane answers no batch on both engines, and both answer
the query DuckDB's eight rows.

**What the plan got wrong.**
- **"`wire/attach.rs` writes no node" leaves `SELECT … WHERE false` with no root.** DataFusion
  plans a bare `EmptyExec` for it — measured, and for `LIMIT 0` too — so the tree is `GpuUnload`
  over the leaf, and a writer that wrote nothing ended with 0 unconsumed nodes. Shown red first
  with the plan's version. The leaf writes the writer's stub; it is the first planned leaf that
  is not a scan, and `Writer::leaf`'s comment says so.
- **The estimator's `sources()` was the line that mattered**, not `:156`/`:185`. It counted
  every `NodeKind::Source`, so at tp4-sized the sizing pass reached two sources and the planning
  pass one, and the plan was refused. Red first through the estimator test and both end-to-end
  cases at tp4-sized.
- **`validate.rs:220`'s width arm is a tautology** — the schema's width against itself — and the
  `_` arm already passes it. Not written.
- **The `join_cases.rs` case proves nothing new.** The harness drives one node over a script and
  never runs the children. A `GpuEmpty` child writes the same stub a `Given` child does, so the
  case would be `right_with_no_build_batch_pads_every_probe_row` again. The shape under a join is
  proved by the whole-plan device case instead.
- **The plan named neither the payload golden nor the cost taxonomy.** `GpuEmpty: none` is a call
  shape the payload coverage check demands a query for, and `cost_model.conf` must name every
  kind. The second rewrites one comment per cost section (below).
- **The query is a Right join over an empty build**, not a Left join over an empty probe:
  DataFusion swaps it. Every lane meets `set_build(None)`, and Right owes its probe rows padded.

**Goldens moved, by kind, checked by script.** No `.cpu.txt`, `.result.txt`, `duckdb-result.txt`,
`gpu-result.txt` or `corpus_cases.inc`: the query's cpu and gpu cells stay off.

| kind | files | what moved |
|---|---|---|
| `*.plans.txt` pbench | 5 | `empty-side-left-join` only, one hunk each: the refusal line → 18 lines at tp1 (5 nodes, 5 recipe lines, 5 memory lines, 3 headers) and 21 at tp4 (a `GpuMergePartitions` more in each part) |
| `recipe-payloads.txt` | 1 | +20: the new section, a header, its digest and 18 lines. The stub prints nothing; the digest covers it |
| `*-mini.cost.txt` | 15 | 726 lines, one per enabled cpu section: `# GpuLoadParquet` → `# GpuLoadParquet, GpuEmpty`. With that comment stripped, all 15 equal `7991fcf0` byte for byte |
| `cost-registry.csv` | 1 | `empty_side_left_join`'s five plan columns `disabled` → `enabled`; `--numstat` 1/1 |

**Cells.** `empty-side-left-join`'s ten cells become enable-able here; enabling them is task 14's.
The five cpu cells answer as DataFusion at every mode (`end_to_end`) and as DuckDB's committed
section row for row (the CLI at tp1 and tp4). The five gpu cells answer as the cpu at every mode on
nebius-gpu (the whole-plan device case). The registry row still carries `155` on the tickets column.

**#256 is unchanged.** `outer-on-true-empty` and `cross-empty-build` are refused in
`scan_mapping::partition`, over a `ParquetExec` of a file with no row group. DataFusion never folds
those to `EmptyExec`, so the new arm never sees them. #256's fix can now be one line in the scan
arm: plan `GpuEmpty` where no row group survives. That would close #282 as well. Both are outside
this task's scope.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base 7991fcf0`: 726 compared, **0
changed, 0 regressions**. The cost comment moved and no figure did.

**Red-green evidence.** Red first: the five SQL-level cases (translator ×2, estimator, end-to-end ×2)
all on `unsupported: plan node EmptyExec`. With the node in, three stayed red on the sizing
pass's two sources against one, and the wire case on "0 unconsumed nodes rather than one root".
Then mutations, each applied by an asserted single match against the whole `--lib` tier
(`test_cost_model` for M7), restored from a saved copy, `cmp`-checked, and the final diff read
for each mutation's text:
- M1 the translator arm off: 11 failed — both translator cases, the estimator case, both
  end-to-end cases, the five pbench plan goldens, the payload golden.
- M2 no stub on the wire: 7 failed — the wire case, the five pbench plan goldens (seqs renumber),
  the payload golden.
- M3 `sources()` back to every `NodeKind::Source`: 8 failed — the estimator case, both end-to-end
  cases, the five pbench plan goldens (their memory sections).
- M4 the null analysis saying `true`: 1 failed, `an_empty_relation_says_no_column_holds_a_null`.
  That case was written after the code, so this is its only red.
- M5 the cpu backend refusing the node: 3 failed — the kind-list case, both end-to-end cases.
- M6 the leaf declared `MultipleBatches`: 8 failed — both translator cases, the five pbench plan
  goldens, the payload golden.
- M7 `GpuEmpty` out of `cost_model.conf`: 2 failed, both `test_cost_model` checks.
- Device, D1, `GpuBackend` sending the node to the recipe lookup: exactly the two new device
  cases failed ("GpuEmpty is at 0 in the tree, where the recipes hold no entry"), 603 passed.
  Applied on the remote copy only, overwritten by the next sync, the file touched there before
  the final build, which recompiled `peacockdb-core`.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S, 0 MiB
in use; cuDF 25.02. Final tree synced, `./scripts/build-test-shadgpu.sh --build` rc 0, each binary
run directly with `--test-threads=1`. **843 cases, all green**: `peacock_gpu_tests` 2,
`peacock_plan_tests` 79, `peacock_join_session_tests` 53, `test_gpu_corpus` 95, `test_node_timing`
1, `peacockdb_core_gpu_lib gpu_tests::` 605 (+2), `peacock_gpu_benchmarks --skip bench_` 8. Not
run, per the override: the sf40 pair and the `bench_` cases. Disk 28 GB free of 96 before and
after; nothing deleted.

**Local, rust-only, fresh on the final tree:** `--lib` 746 passed, 2 ignored; `test_cpu_corpus`
1006; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage`
11; `test_golden_format` 43; 0 failed.

**Warnings.** None new. The rust-only build and test builds have none. The local gpu-feature
lib-test build (`cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`) has the
one pre-existing `AsArray`. `NodeRef::Empty`'s borrow is read by nobody, so it carries the
`#[allow(dead_code)]` the three forwarders carry, with its reason.

**Formatting.** Under edition 2024. Every touched file that was rustfmt-clean at HEAD is clean
now; the four that gained a hunk were formatted whole through a temp copy. The rest report
exactly their HEAD hunk counts: `cpu_backend/source.rs` 1, `gpu_backend/backend.rs` 4,
`nullability.rs` 1. The three `mod.rs` were checked with stub children so rustfmt did not
follow them.

**build-test.md counts, counted off `--list`.** cpu block 1776 → 1783: `--lib` 741 → 748 — "End to
end" 30 → 32, "Translator, one rule at a time" 37 → 39, "Memory estimation" 11 → 12, "Null
analysis rules" 11 → 12, "Recipes per join type" 20 → 21. gpu block 710 → 712: `--lib --
gpu_tests::` 603 → 605, "Operator harness" 374 → 376. Rust 2606 → 2615, grand total 3178 →
3187 (the N columns summed by script: Rust 2615, C++ 171, Python 401). Prose rewritten: "End to
end" (18 queries, twelve cases, the empty relation alone), "Recipe payloads golden" (pbench's
member and why), "Recipes per join type" (the empty relation's stub), "Memory estimation", and
"Operator harness" (the empty relation, alone and planned whole).

**architecture.md sentences this round falsified** (the coordinator's, or task 15's rewrite):
- "The node set" table has no `GpuEmpty` row (Source; one lane, no batch, no call).
- "From node to seqs" has no `GpuEmpty` row: a stub `CudfScan`, never called, no recipe.

**Ticket text made stale.**
- #256 (`joins.md`): "a `GpuEmpty` the way #155's `EmptyExec` arm will need one" — it exists.
- #257 (`df-upgrade.md`): "Today the engine refuses the plan — `unsupported: plan node
  EmptyExec`, #155 … That arm lands in join-backend". It has landed; a bare `EmptyExec` now plans
  and answers DataFusion's 0 rows. The ticket's later paragraph — the rule refusing first, on
  #250 — is what still holds, and `in-is-null` still meets #250 at all five modes.
- #217 (`corpus-coverage.md`): "DataFusion is expected to turn `LIMIT 0` into an empty relation
  before a sort exists. Unconfirmed". It does: `… ORDER BY n_name LIMIT 0` and `… LIMIT 0` both
  plan to a bare `EmptyExec`, which now plans as `GpuEmpty` and answers no row.

**Drift found, not mine.** `planner/tests/null_analysis.rs`'s module doc says an anti or mark join
is refused when NULLs can meet on both sides; that refusal went in task 9.
`plan_goldens.rs`'s `PAYLOAD_QUERIES` doc said "the ten mode goldens" where pbench made fifteen;
corrected in this change, since the line was being edited.

**What plan task 9c inherits.** Nothing from this task touches the nested loop or `expr.cpp`.
`indf-full-join` is as task 9 left it. Two shared files gained lines: `PAYLOAD_QUERIES`, now 22,
reads pbench, so a 9c payload member can come from pbench without widening the loop; and
`source_cases.rs` has the template for a whole-plan, two-engine device case (plan, `run` on
each, `Device::open` over the whole tree), which 9c's promoted hash join can copy if the harness
alone does not show it. For task 13: exec_model has no `GpuEmpty` and models no pbench query, so
nothing there moved.

#### Round 2, plan task 9c — `IS [NOT] DISTINCT FROM` as a join condition (review row 9)

The device's two evaluators, the translator, and tests. No fbs change: `IsDistinctFrom` and
`IsNotDistinctFrom` were already on the wire, and the cpu already evaluated both.

**What shipped.**
- `cpp/src/expr.cpp`: `build_expr`'s binary arm answers `IS NOT DISTINCT FROM` with the AST's
  `NULL_EQUAL` and `IS DISTINCT FROM` with `NOT` over it, ahead of `fb_to_ast_op`. The column
  path maps them to `NULL_EQUALS` and `NULL_NOT_EQUALS`, and `is_predicate_op` names both, so
  that path declares a `BOOL8` result. A nested loop, or a keyed join's residual, takes either
  path. A non-AST conjunct is not hoisted (`is_comparison` does not name them); it goes to the
  pair-gather residual, which is correct.
- Translator: `hash_join`'s body is now `keyed_join(t, join, join_type, KeyedOn)`. `KeyedOn`
  holds the keys, the residual and its column map, the flag and the projection, and both join
  arms build their join through it. `nested_loop_join` first asks `null_equal_keys(join)`. That
  splits the filter's top-level `AND`s. Each `Column IS NOT DISTINCT FROM Column` over one
  column of each side, in either order, becomes a key. The rest is translated and re-joined with
  `AND` as the residual, under the filter's whole column map. If there is a key, the node is a
  `GpuHashJoin` with `null_equals_null = true`. Otherwise the arm is unchanged.
- `GpuHashJoin.null_equals_null`'s doc names the second source of `true`.
- Tests. `planner/tests/distinct_from_keys.rs` has 4 cases, each answering as DataFusion with a
  hand count: Full 16, Inner 16 with the probe's column written first, LeftSemi 7, LeftAnti 3;
  a residual beside the key, 3 + 8 + 8; `=` beside it, 7; `IS DISTINCT FROM` and `a.k + 1`, which
  keep the loop, 84 and 12. The device has `tests/gpu_tests/distinct_cases.rs` with 3 cases: the
  four arms through the AST and through the column path, each held to the truth table, and
  pbench's query planned whole at five modes on both engines. `nested_cases.rs` has 4 more: Full
  on `IS NOT DISTINCT FROM`, Left on `IS DISTINCT FROM`, and both over decimals on the
  cross-then-mask path. `test_plan_executor.cpp` has 2: each operator against a typed NULL, one
  per evaluator. `PBENCH_JOINS`' `indf-full-join` row reads the hash join.
- `tests/gpu_tests/whole_plan.rs`: `answers_on_both(dataset, query, mode)`, the whole-plan run
  task 9b wrote inline in `source_cases.rs`. Its case now calls it too.
- `cost-registry.csv`: `indf_full_join`'s feature tag `outer_join nested_loop_join` →
  `outer_join`; `--numstat` 1/1.

**Divergences.** None found. The four arms agree with the truth table on both engines and on
both evaluators. pbench's query answers 2,009 rows on both engines at every mode, the same rows.
Through the CLI at tp1 and tp4 the cpu's sorted rows equal DuckDB's committed section byte for
byte. **Not measured:** a float operand. The AST's `NULL_EQUAL` compares by value, so NaN is
unequal and -0.0 equals 0.0. DataFusion's comparison is by bits. That is #243's class. No
corpus or pbench query has a float `IS [NOT] DISTINCT FROM`.

**What the plan got wrong.**
- **The column path needed a third line.** `binop_output_type` declares a non-predicate's
  result in the left operand's type, so a decimal `NULL_EQUALS` asked cuDF for a decimal result
  and cuDF refused: "Unsupported operator for these types" (D1 below). `is_predicate_op` names
  both operators now.
- **A filter cannot show the arm that matters.** A filter drops a NULL and a `false` alike, so
  the plan's `GpuFilter` cases pass on an engine that answers NULL for the one-NULL arm. The
  cases are projects held to the truth table. The plan's `synthetic` pairing (`i32` cast against
  `i64`) rarely makes the equal arm. A five-row batch makes each arm once.
- **The `=`-beside case is a guard and was never red.** DataFusion keys on the `=`, so the
  shape is a `HashJoinExec` and never reaches `nested_loop_join`. The test pins that the flag
  stays `false` there.
- **sqlparser binds `IS NOT DISTINCT FROM` looser than `AND`.** `a.k IS NOT DISTINCT FROM b.k
  AND a.v < b.v` parses as `a.k IS NOT DISTINCT FROM (b.k AND a.v < b.v)`, and DataFusion
  refuses to coerce it. The test parenthesizes. A trap for the next test writer.
- **The design's "the hash join shuffles into lanes where the nested loop ran in one" is
  false.** DataFusion puts no repartition under a nested loop, so `co_partitioned` says no and
  both sides merge into one lane, as before. Every tp4 plan of the query shows `lanes=1`.
  Shuffling would mean the planner makes up a partitioning DataFusion never chose. That is
  outside this task, and no ticket is filed: the answer is right, only the lane count differs.
- **`hash_join_from_parts` with nine arguments** became `keyed_join` over a `KeyedOn`. The
  residual keeps the filter's whole column map. A column no conjunct reads is harmless:
  validation checks only the columns referenced, and both engines carry the extra column.
- **The test files.** `translator/tests.rs` is past the 1,000-line cap (1,014 at HEAD), and
  `exec_cases.rs` is near it, so the cases got files of their own.
- **The memory estimate moves +136 bytes.** The promoted join prices its hash table: 8 build
  rows × 16, plus 8 bytes for which rows matched. That is the same +136 at every mode.

**Goldens moved, by kind, checked by script.** No `.cpu.txt`, `.cost.txt`, `.result.txt`,
`duckdb-result.txt`, `gpu-result.txt`, `recipe-payloads.txt` or `corpus_cases.inc`. The query's
cells are off, and it is not a payload member.

| kind | files | what moved |
|---|---|---|
| `*.plans.txt` pbench | 5 | `indf-full-join` only, 6 of its 21 (tp1) or 27 (tp4) lines, 30/30 by numstat: the node line (`GpuNestedLoopJoin` with `filter=… IS NOT DISTINCT FROM …` → `GpuHashJoin` with `on=[(t_k@1, d_k@1)], null_equals_null=true`, the projection unchanged), its recipe line (the name alone), and four memory lines, each +136 |
| `cost-registry.csv` | 1 | the feature tag above |

The arithmetic. Every other section of the five files is equal to HEAD's. The recipe line is
equal to HEAD's once `GpuNestedLoopJoin` is replaced with `GpuHashJoin`. The memory figures read
`accumulators` 160 → 296, `GpuUnload` and `GpuProject` 80 → 216, and the join 24,211 → 24,347,
at every mode.

**Cells that become enable-able: all ten of `indf-full-join`'s, left to task 14.** Task 9 made
the five cpu cells enable-able; they still answer DuckDB's 2,009 rows, now through the hash
join. The five gpu cells answer the cpu's rows at every mode on nebius-gpu (the whole-plan
case). The registry row still carries `155 160` on its tickets column.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base b153a3eb`: 726 compared, **0
changed, 0 regressions**.

**Red-green evidence.** Planner, red first: two of the four `distinct_from_keys` cases, both on
"nested loop where a hash join was expected". The other two are the guards above. Device, a red
build of the tests over HEAD's code: all 9 new cases red. The 7 Rust cases were "unsupported
BinaryOp: 19/20", in the column path's wording where it applies; the whole-plan case failed on
the nested loop's probe. The 2 gtests threw the same. Then mutations. Each local one was
applied by an asserted single match, run against the whole `--lib` tier, restored from a saved
copy, and checked with `cmp`.
- M1 the promotion off: 7 failed — two `distinct_from_keys` cases and the five pbench plan
  goldens.
- M2 the flag `false`: the same 7.
- M3 a pair with the probe's column first not taken: 6 failed — the "alone" case (its Inner
  query was rewritten probe-first so that it sees this) and the five plan goldens.
- M4 the residual dropped: 1 failed, the residual case.
- M5 `IS DISTINCT FROM` promoted too: 1 failed, the stays-a-loop case.
- Device, one build, on the remote copy only: D1 `is_predicate_op` without the two operators,
  and D2 the AST answering `NULL_EQUAL` for `IS DISTINCT FROM` too. 5 Rust cases failed and 607
  passed. D1 accounts for the column-path case and both decimal nested loops ("Unsupported
  operator for these types"). D2 accounts for the AST case and the Left `IS DISTINCT FROM`
  nested loop (rows differ). The column-path gtest failed too, 80 passed. The local `expr.cpp`
  was touched and re-synced. The diff against the pre-mutation copy was empty, and the final
  build recompiled `expr.cpp`.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S, 0
MiB in use; cuDF 25.02. The final tree was synced, `./scripts/build-test-shadgpu.sh --build` gave
rc 0 with no warning in its log, and each binary was run directly with `--test-threads=1`.
**852 cases, all green**: `peacock_gpu_tests` 2, `peacock_plan_tests` 81 (+2),
`peacock_join_session_tests` 53, `test_gpu_corpus` 95, `test_node_timing` 1,
`peacockdb_core_gpu_lib gpu_tests::` 612 (+7), `peacock_gpu_benchmarks --skip bench_` 8. Not run,
per the override: the sf40 pair and the `bench_` cases. Disk was 28 GB free of 96 before and
after; nothing was deleted.

**Local, fresh on the final tree.** rust-only: `--lib` 750 passed, 2 ignored; `test_cpu_corpus`
1006; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage`
11; `test_golden_format` 43; 0 failed. `peacock_cpu_tests` 15 passed, from `/tmp/dkb-cppbuild`
rebuilt over the new `expr.cpp`; `binop_output_type` is one of its subjects.

**Warnings.** None new. The rust-only builds have none. The local gpu-feature lib-test build
has the one pre-existing `AsArray`. The nebius build log has no warning.

**Formatting.** Under edition 2024, every touched Rust file is rustfmt-clean, as each was at
HEAD. `git clang-format` was applied to the changed C++ lines. It split the two new column-path
`case`s onto two lines each, where their neighbours are on one line.

**build-test.md counts, counted off `--list`.** cpu block 1783 → 1787: `--lib` 748 → 752, and a
new row, "`IS NOT DISTINCT FROM` as a join key", with 4. gpu block 712 → 719: `--lib --
gpu_tests::` 605 → 612 (`--list` 612), "Operator harness" 376 → 383. Plan-executor (C++) 79 → 81
(`--gtest_list_tests` 81). Rust 2615 → 2626, C++ 171 → 173, grand total 3187 → 3200, the N
columns summed by script. Prose rewritten: "Operator harness" (the four arms, the nested loop on
either path, pbench's query planned whole) and "Plan-executor" (each operator against a typed
NULL).

**architecture.md sentences this round falsified** (the coordinator's, or task 15's rewrite):
- "Join types and NULL key equality": "`null_equals_null` travels in the flat buffers per join,
  mirroring DataFusion", and "`true` means NULL = NULL, which is what a set operation lowered to
  a join needs". The planner now also sets it, for keys made of `IS NOT DISTINCT FROM`.
- The wire table's `CudfHashJoin.null_equals_null` row: "which the planner set from
  `HashJoinExec::null_equals_null()`". It is also set from a nested loop's `IS NOT DISTINCT FROM`
  conjuncts. The row's `CudfHashJoin` has been stale since task 7, too.

**Ticket text made stale.** None. No ticket mentions `IS [NOT] DISTINCT FROM`.

**Drift found, not mine.** `planner/translator/tests.rs` is 1,014 lines, past `coding-style.md`'s
cap. Task 9b's two cases took it over.

**What plan task 10 inherits.**
- The device harness has the nested loop on `IS NOT DISTINCT FROM` (Full, AST), on `IS DISTINCT
  FROM` (Left, AST), and both over decimals (the cross-then-mask path). Step 2 need not add them.
- The promoted Full hash join under `null_equals_null = true` is proved only whole-plan, through
  pbench's query. Step 2's "Left and Full new … under `true`" node-level cases are still owed.
- `whole_plan::answers_on_both` is the shared whole-plan runner, for any case the harness cannot
  show.
- For task 13: exec_model has no `IS [NOT] DISTINCT FROM` and models no pbench query, so nothing
  there moved. For task 14: `indf-full-join`'s ten cells, as above.

#### Round 2, plan task 10 — the 56 pins accounted for, every pbench join shape at node level, and the order guard

Tests and one test-support module. No production code, no golden, no fbs, no C++.

**Step 1: all 56 pins were already flipped, none missed.** Tasks 7 and 8 flipped each in the
commit that changed its behaviour. Seven went through a second pin on the way: task 7 re-pinned
them under #220 or as a cpu-only #212 refusal, and task 8 flipped those. Every target name below
exists once at `3880153c`. `git grep -nE 'fn bug_'` over the five join harness files lists only
#243's float pin and #246's `LIKE` pin. `git grep -nB3 'fn bug_' -- peacockdb-core/src` cites
none of the tickets this task closes. `BUILD_COPY`, `PROBE_COPY`, `NO_KEYS`, `NO_BUILD`,
`INNER_ONLY`, `TWO_OF_SIXTEEN`, `THREE_OF_SIXTEEN` and `device_answers_as_if_null_equals_null`
are gone from `peacockdb-core`. The ledger, `jc` = `join_cases.rs`, `jd` = `join_dimension_cases.rs`,
`nc` = `nested_cases.rs`, `ns` = `nested_schema_cases.rs`; T7 = `a9ae581c`, T8 = `29937e22`:

| # | ticket | file | flipped to | in |
|--:|---|---|---|---|
| 1–2 | #152 | jc | `a_{left,full}_join_over_one_probe_batch_agrees` | T7 |
| 3–8 | #152 | jc | `{an_inner,a_left,a_right,a_full,a_right_semi,a_right_anti}_join_over_two_probe_batches_agrees` | T7 |
| 9–10 | #152 | jc | `inner_{with_a_zero_row_probe_between_two_with_rows,finishing_after_only_zero_row_probes}_agrees` | T7 |
| 11–15 | #152 | jc | `left_over_a_zero_row_build_agrees`, `left_over_a_zero_row_probe_pads_every_build_row`, `left_over_both_sides_empty_agrees`, `left_with_a_zero_row_probe_between_two_with_rows_agrees`, `left_finishing_after_only_zero_row_probes_agrees` | T7 |
| 16–17 | #152 | jc | `right_{with_a_zero_row_probe_between_two_with_rows,finishing_after_only_zero_row_probes}_agrees` | T7 |
| 18–22 | #152 | jc | `full_over_a_zero_row_build_pads_every_probe_row`, `full_over_a_zero_row_probe_pads_every_build_row`, `full_over_both_sides_empty_agrees`, `full_with_a_zero_row_probe_between_two_with_rows_agrees`, `full_finishing_after_only_zero_row_probes_agrees` | T7 |
| 23–26 | #152 | jc | `right_{semi,anti}_{with_a_zero_row_probe_between_two_with_rows,finishing_after_only_zero_row_probes}_agrees` | T7 |
| 27–28 | #152 | jd | `a_right_join_with_a_crossing_projection_over_two_probe_batches_agrees`, `an_inner_join_with_a_residual_over_two_probe_batches_agrees` | T7 |
| 29–30 | #152 | nc | `a_cross_join_over_two_probe_batches_agrees`, `an_inner_nested_loop_join_over_two_probe_batches_agrees` | T7 |
| 31–35 | #59 | jc | `a_left_anti_join_keeps_its_null_key_build_rows`, `a_right_anti_join_keeps_its_null_key_probe_rows`, `a_left_mark_join_marks_its_null_key_build_rows_false`, and the two over two probe batches | T7 |
| 36–37 | #59 | jc | `a_left_{anti,mark}_join_with_a_residual_filter_agrees` (T7 re-pinned under #220) | T8 |
| 38–39 | #59 | jc | `left_{anti,mark}_with_a_zero_row_probe_between_two_with_rows_agrees` | T7 |
| 40 | #59 | jd | `a_left_anti_join_on_a_composite_key_with_a_null_second_column_keeps_its_rows`, 30 of 32 by hand | T7 |
| 41–44 | #173 | jc | `left_…_pads_every_build_row`, `full_…_pads_every_build_row`, `left_semi_…_answers_nothing`, `left_mark_…_marks_every_build_row_false` (each `_finishing_with_no_probe_batch_`) | T7 |
| 45–47 | #212 | jc | `right_…_pads_every_probe_row`, `full_…_pads_every_probe_row`, `right_anti_…_returns_every_probe_row` (each `_with_no_build_batch_`; T7 re-pinned as a cpu refusal) | T8 |
| 48–50 | #190 | nc | `an_inner_nested_loop_join_with_a_projection_agrees`, `a_left_nested_loop_join_with_a_projection_agrees`, `an_inner_nested_loop_join_with_a_decimal_predicate_and_a_projection_agrees` | T8 |
| 51 | #190, #215 | nc | `a_left_nested_loop_join_with_a_decimal_predicate_and_a_projection_agrees` (T7 left its cpu half) | T8 |
| 52 | #215 | nc | `a_left_nested_loop_join_with_a_decimal_predicate_agrees` (T7 re-pinned under #220) | T8 |
| 53 | #207 | nc | `a_cross_join_with_a_projection_agrees` (T7 left its cpu half) | T8 |
| 54 | #207 | ns | `a_cross_join_with_a_projection_declares_the_kept_columns_the_device_holds` | T7 |
| 55–56 | #208 | nc | `a_cross_join_over_{a_zero_row_build,both_sides_empty}_agrees` | T8 |

**Steps 3 and 4 were done too**: `dimensions.rs` by task 8 (no `feeds_owing_build`, every lane
of an owing join reaches `SetBuild`, the answer checked against the oracle), the wire walk and
`driven()` by task 7. `SEMI_JOIN` stays.

**What shipped.**
- `tests/gpu_tests/join_shape_cases.rs` (new, 13 cases and a guard). `PBENCH_SHAPES` names, per
  pbench query that plans a join, the node-level cases holding its shapes: 47 queries, the two
  #256-refused ones included for the day they plan. `every_pbench_join_shape_has_a_node_level_case`
  reads pbench's tp1-single plan golden, collects every section whose tree has a join, and fails
  on a join query the table lacks, a table key that is no pbench query, or a case name the
  harness never registered. The new cases: Left and Full with NULL keys on both sides under the
  SQL default and under `null_equals_null`, by hand (the build's two NULL keys padded once each,
  the probe's four too for Full; 2 × 4 pairs under `true`, nothing padded);
  `a_left_semi_join_over_many_to_many_keys_emits_each_build_row_once` (distinct `b_id`, count
  computed off the inputs); the preserved-side NULL condition, kept by the anti join and marked
  `false` (never NULL) by the mark, on build row 22, which a precondition shows meets a probe key;
  `IS NULL` residuals on the probe (RightAnti) and on the build (LeftMark), pbench's `NOT IN`
  rewrite shapes; `*_finishing_with_no_probe_batch_answers_nothing` for Inner, Right, RightSemi and
  RightAnti, the four the per-type list lacked.
- `tests/gpu_tests/join_key_type_cases.rs` (new): every `key_types` column, 19, as an Inner and a
  LeftSemi key, 38 cases from one macro, plus a guard that the column count is 19. Build 72 rows,
  probe 96, one seed, so the build is the probe's first 72 and every column has matches, misses
  and a NULL on each side. The float columns drop the rows holding -0.0 and the NaNs (#243's).
- `join_dimension_cases.rs` (+7): crossing projections on Left and Full over two probe batches;
  RightAnti and LeftMark on a composite key; `a_keyed_left_semi_join_with_a_decimal_residual_agrees`;
  Inner and Full over a decimal residual with 64 bytes of pairs a call. `hash_join_keyed` builds
  any type through `join_cases::output_over`, which `output_of` now calls. Three stale comments
  rewritten (the #152 and #59 refusals, the "accumulated-keys finish").
- `nested_cases.rs` (+12): `a_cross_join_emits_each_build_row_against_the_whole_probe_batch_in_turn`
  (`Order::AsEmitted` on both, then every row's `(b_id, p_id)` checked as `(i / 12, i % 12)`);
  a Left nested loop over two probe batches; predicate-free Left, Right and Full over two probe
  batches, over the empty side each pads from (16 rows, by hand), and Right with no build batch;
  Full nested loops chunked to one probe row a call on the AST path and the cross-then-mask path.
- `script.rs`/`device.rs`: `run_both_with_join_scratch` and `Device::with_join_scratch`, the
  `chunk_bytes` a case writes; `run_both` and `open` pass 0 as before.
- The spec's risk 4. `test_support/order_hazard.rs` (new) and one `device_order_<ds>_<q>` case per
  corpus line with a device cell, from `test_cpu_corpus.rs`'s `device_cell_cases!` (renamed from
  `duckdb_device_cases!`, since it now makes both kinds). Over DataFusion's plan at one partition:
  a positional selection (`GlobalLimitExec`, `LocalLimitExec`, any other node with a fetch) that
  reaches a hash, nested-loop, sort-merge or symmetric hash join with no sort and no keyless
  aggregate between is refused. A sort with a fetch over such a join is re-run with every sort
  cutting at that row widened by one, and refused if the rows either side of its fetch or of the
  limit's offset above it tie on every sort key and differ in any column. A cross join is
  descended through, not counted: both engines emit it build-row-major, which the new harness
  case pins. The `data_fusion_subset` half was already held:
  `an_undetermined_lines_rows_are_compared_only_against_committed_files` keeps such a line's
  device cells off. 11 unit cases in `order_hazard/tests.rs`. `corpus.rs`'s `session_for` and
  `query_text` became `pub(crate)` for it.

**Divergences.** None. Every new case agrees slot by slot on nebius-gpu, the hand counts hold on
the device's answer, and all 38 key-type cases agree, unsigned, decimal(38), timestamp and boolean
keys included.

**What the plan got wrong.**
- **Step 1 was already done**, and steps 3 and 4 with it. `crossing_projection`'s Left and Full
  arms were real since task 7, but no case used them; and no Left nested loop had two probe
  batches. Both added.
- **Most of step 2 existed already**: three probe batches with a zero-row one between for all nine
  types, no-build for all nine, no-probe for five, `null_equals_null` for seven types (under the
  names `an_anti_join_under_null_equals_null_agrees`, `a_mark_join_under_null_equals_null_agrees`
  rather than the plan's). Only what was missing was added.
- **`inner-join-hot-keys` does not chunk.** An unfiltered keyed join answers through
  `inner_join` in one piece; only the pairs path (a non-AST residual) and the nested loops chunk.
  So its node-level case is the plain many-to-many Inner, and the chunking cases sit on the paths
  that chunk.
- **An unordered `LIMIT` over a keyless aggregate is no hazard.** tpcds q16, q32, q38, q92, q94,
  q95, q96 and q97 put a `LIMIT 100` over one aggregate row; read as "a limit over a join" they
  were refused, falsely. A keyless aggregate is one of the two nodes that fix the order.
- **Ties cannot be read off a plan.** The plan text says which sort cuts; whether the cut falls on
  a tie is the data's. Hence the widened re-run. It costs little: all 176 plannable lines ran in
  6.6 s.
- **The device exports a column non-nullable in a batch that holds no NULL**, as task 7 noted,
  so a case that concatenates a join's calls must relax nullability first.

**Measured over every corpus line, not only the device ones**, by temporarily giving the `none`
arm the case too: 176 pass, `pbench/in-is-null` does not plan in DataFusion (#250). No line has an
unordered selection over a join. 34 lines cut a sorted answer with rows past the cut over a join:
tpch q2, q3, q10, q21, and tpcds q1, q5, q7, q14, q15, q18, q19, q21, q22, q26, q30, q33, q35, q40,
q46, q52, q55, q56, q60, q62, q65, q68, q69, q75, q76, q78, q79, q80, q81, q93. No cut falls on a
tie, so task 14's cells will not be refused by this check. The 25 device lines' four sorted cuts
(tpcds q37, q82, q84, q85) answer fewer rows than the fetch.

**Red-green evidence.** Rust-only, each mutation applied by asserted single match, restored from
a saved copy and checked with `cmp`.
- Red first: the 10 unit cases over a stub answering `None`: the 5 hazard cases failed. Then the
  keyless-aggregate case, added after the first corpus run: red, 1 of 11.
- M1 `tied` always true: 1 unit case red; over every line, 35 red — the 34 lines above and
  `in-is-null`, which fails on any build. Over the 25 device lines, 0 red: their cuts are past
  their rows (read off a temporary `eprintln!`, removed).
Device, on the remote copy only, restored, `md5sum`-checked against local, touched, rebuilt:
- D1 (`join_residual.cpp`'s `chunks` drops a chunked range's last row) with D2 (`probe_cross`
  answers `cudf::reverse` of the product) and D3 (the wire writes `!null_equals_null`): 120 red.
  D3 alone accounts for 117, every case with a NULL key among them, the 38 key-type cases and
  seven of the new shape cases included.
- D1 alone: the four chunked cases red (the first form of the AST chunk case, a Left nested
  loop, stayed green — the dropped probe row matched nothing, so Left could not see it; it is a
  Full now and goes red). The cross order case was red too, because the restored
  `join_session.cpp` was not recompiled: the rsync trap, here by accident isolating D2 — the
  `AsEmitted` case is the only one D2 reddens, every `Order::Any` cross case staying green.
- The guard: a `PBENCH_SHAPES` entry dropped → `pbench joins with no node-level case named:
  ["nl-left"]`; a case name misspelt → `named and not registered`. The first attempt at the drop
  matched nothing after rustfmt reflowed the table and passed; it is not counted.
- The new agreement cases (key types, predicate-free loops, no-probe, crossing projections,
  composites) were written against code that already answered them and were never red: there is
  no production change in this task for them to precede.

**The device run, recorded.** nebius-gpu, NVIDIA L40S, 0 MiB in use; cuDF 25.02. The final tree
synced, `./scripts/build-test-shadgpu.sh --build` rc 0 with no warning in its log, each binary run
directly with `--test-threads=1`. **924 cases, all green**: `peacock_gpu_tests` 2,
`peacock_plan_tests` 81, `peacock_join_session_tests` 53, `test_gpu_corpus` 95, `test_node_timing`
1, `peacockdb_core_gpu_lib gpu_tests::` 684 (+72), `peacock_gpu_benchmarks --skip bench_` 8. Not
run, per the override: the sf40 pair and the `bench_` cases. Disk 28 GB free of 96 before and
after; nothing deleted.

**Local, rust-only, fresh on the final tree:** `--lib` 761 passed, 2 ignored; `test_cpu_corpus`
1031; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage`
11; `test_golden_format` 43; 0 failed. No golden moved, so no cost diff.

**Warnings.** None new. The rust-only build has none; the gpu-feature lib-test build
(`cargo-cudf.sh test -p peacockdb-core --lib --features gpu --no-run`) has the one pre-existing
`AsArray`.

**Formatting.** Edition 2024, through stdin so no `mod` child is followed. Every touched file is
rustfmt-clean; all were at HEAD but `join_dimension_cases.rs`, whose one HEAD hunk was the
signature of `hash_join_keyed`, which this task rewrote.

**build-test.md counts, counted off `--list`.** cpu block 1787 → 1823: `--lib` 752 → 763 (a new
row, "Order hazard", 11), `test_cpu_corpus` 1006 → 1031 ("Corpus, cpu" 1005 → 1030: the order
tier, 25). gpu block 719 → 791: `--lib -- gpu_tests::` 612 → 684, "Operator harness" 383 → 455.
Rust 2626 → 2734, grand total 3200 → 3308, the N columns summed by script. Prose rewritten:
"Corpus, cpu" (the order tier) and "Operator harness" (the key types, the pbench shapes and their
guard, the chunked paths, the cross join's order).

**What plan tasks 11–14 inherit.**
- 11: nothing here touches the plan-executor gtests.
- 13: exec_model models no pbench query and no `key_types` join; nothing moved.
- 14: every line it will enable passes the order tier today, so the check refuses none of the
  estimate's cells; a line whose answer comes to tie at a cut, or a new query with a `LIMIT` over a
  join, is refused at its first device cell. Every pbench join row's shapes are proven at node
  level on the device before its cells turn on, so a pbench cell that fails there is a planning
  or driver question, not a join-arm one. `PBENCH_SHAPES` must gain a row for any pbench join
  query added, or the guard goes red.

#### Round 2, plan tasks 11–13 — the plan-executor gtests on the session, the goldens checked current, exec_model's recipe join retired

Three tasks in disjoint files, one commit each. No production code, no fbs, no Rust.

##### Task 11 — the four plan-executor gtests drive the session

**What shipped.** `cpp/tests/gpu/test_plan_executor.cpp` only. `run_join(session, seq, build,
probe)` builds, probes, finishes and releases one session over one batch each side, and
expects exactly one of the probe and the finish to answer. `WholePlan` recognises the
layout the wire writes a join in (`is_join_union`): the build side, the probe side and the
`CudfJoin` leaf under one structural union. It runs the two sides, hands their handles to
`run_join` at the leaf's seq, and counts the union's seq without executing it.
`nation_join_region(fbb, projection)` builds that layout once for all four tests. Nation is
the build side, so the output is still `[nation…, region…]`: the projections (1, 2, 5 / 5 /
1, 5) and every assertion are unchanged — 25 rows of 7 columns, 25 of 3, 5 groups of 5, and
ALGERIA in AFRICA first. 61 lines in, 142 out.

**What the plan got wrong.**
- **No `run_join` existed in the file.** It is new. It calls `NodeSession::join_*`, not the
  C symbols: the file drives `NodeSession` throughout, and the symbols wrap those methods.
- **"Build a `CudfJoin` leaf" left the sides nowhere to go.** A leaf has no children, so
  the two scans need a place in the tree to get a seq. The wire's own answer is a structural
  union over `[build, probe, join]` (`wire/writer.rs`'s `reduce`), and the test uses it.
  The seqs therefore match what a real plan hands the C++.

**Red-green.** The session existed before the tests moved onto it, so there was no red
before the change. Mutations stand in, each on the remote copy only and rebuilt with
`cmake --build cpp/build --target peacock_plan_tests`:
- D1, the session's Inner arm answers nothing (`return std::nullopt` ahead of
  `inner_join_size`): **exactly the four red**, each "the probe answered 0 and the finish 0".
- D2 + D3 together. D2: the Inner/Left arm skips `projected`. D3: `execute_hash_join`
  throws on entry. **Three red**: `HashJoinWithProjection` (7 columns),
  `AggregateGroupBy` (25 groups of 1), `JoinProjectSort`. `HashJoinNationRegion` stayed
  green with the old operator throwing, so no case in the binary reaches it.
- The first D2 pattern matched twice (the pairs path repeats the line) and the
  asserted-single-match script wrote nothing. It was re-anchored on three lines and is not
  counted twice.
- Restored from saved copies, md5 equal to local (`join_session.cpp` 6defdc79…,
  `join.cpp` eb943efd…). The tree was re-synced, then **both files touched on the remote**
  (rsync's mtime trap). The final `--build` recompiled both.

**Local.** `/tmp/dkb-cppbuild` `ninja peacock_plan_tests`: compiles and links with no
warning. `git clang-format` was applied to the changed lines only (three hunks).

##### Task 12 — the goldens checked current: nothing moved

- **Step 1.** `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1 cargo test --features
  rust-only -p peacockdb-core --lib plan_goldens`: **26 passed**. All 15 `*.plans.txt` and
  `recipe-payloads.txt` were rewritten (their mtimes are the run's), and `git diff` is empty.
  The end state is the plan's: the session line on 75 joins per tpch mode and 656 per tpcds
  mode, and no `CudfHashJoin`, `CudfCrossJoin` or `CudfNestedLoopJoin` in any golden.
  The payload coverage case did not go red mid-rewrite this time (task 7's trap), since
  nothing was being rewritten to a different content.
- **Step 2.** `UPDATE_CANONICAL=1` over `test_cpu_corpus`: **1031 passed**. 33 files
  rewritten: 15 `.cpu.txt`, 15 `.cost.txt`, 3 `mini.result.txt`. Diff empty. Then
  `test_cost_model` under the same variable: 3 passed, no diff. With no file changed, #213's
  lost section cannot have happened.
- **What the plan got wrong.** pbench now carries 54 session lines per tp1 mode and 51 per
  tp4 mode, not task 7's 27: tasks 9–10 made more pbench joins plan. Nothing to regenerate.

##### Task 13 — exec_model: the recipe join retires

**What shipped.**
- **Deleted** `operators/recipe_join.py`, `operators/recipe.py` and
  `operators/cudf_calls.py`, about 1,250 lines in all.
- `join_types.py`: the two facts of §4.1. `NEEDS_FINISH` / `needs_finish()` name the five
  finishing types, as `plan::needs_finish` does, and `FINISH_ONLY` stays.
  `Capability`/`capability()`, `PROBE_LOCAL`, `ONE_SIDED` and `IGNORES_NULL_EQUALS_NULL` are
  gone.
- `joins.py`:
  - every probe streams, and no join refuses;
  - a residual is part of each pair's match on every type;
  - `null_equals_null` is one rule for every type, anti and mark included (#59, #80);
  - `NestedLoopJoin` answers all nine types (#160) with a `matched` flag per build row, and
    `predicate=None` is the literal `true`.
- `nodes.py`, `node.py`, `tests/corpus.py`: `recipe_factory`, `RecipeJoinBackendSelector`
  and `PCK_BACKEND` are gone.
- **Two corpus lowerings collected their probe for a reason §4.1 removed.** TPC-H q21 (`l2`,
  `l3`) and TPC-DS q16/q94 (`siblings`) put a `coalesce_all` under the probe of a filtered
  semi/anti join. The collects are dropped. `plans.py --write` then moved the prototype's
  plan goldens by exactly those four `BatchAccumulator`s and nothing else.
- README: the file table, a "The joins" section in place of "The two join backends", the
  corpus' `PCK_BACKEND` paragraph gone, and F11/F12 brought to the current tests.
- `.github/workflows/exec-model-corpus.yml`: the `backend` input and `PCK_BACKEND` are gone.
  Checked mechanically: the YAML parses with `filter` as its only input, and the rendered
  `run:` block passes `bash -n`.

**`test_join_capability.py`, case by case** (21 → 17):
- **Removed, recipe-only:** the node-sequence case, the build-copy count, the
  single-batch-no-copy case and the legacy single call (plan :421, :438, :481, :516).
- **Flipped:**
  - :329, the outer residual refusal → `…_keeps_its_unmatched_rows` (Left, Right and Full at
    every config);
  - :349, the second-batch refusal → `test_a_filtered_semi_join_streams_its_probe_side`
    (asserts more than one probe call);
  - :365, the #159 refusal → `test_a_right_semi_join_with_a_residual_filter_answers_as_the_oracle`;
  - :527, the #153 pin → `test_a_left_join_pads_the_build_rows_its_residual_rejects`, counted
    by hand: under `v > 80`, D matches ten probe rows and no pair passes, and E matches no
    key, so both come back padded;
  - :564, the null-key asymmetry → `test_null_keys_follow_null_equals_null_for_every_type`,
    counted by hand. Under the SQL default anti keeps C and D and mark marks D false; under
    `true` anti keeps C alone and mark marks D true.
- **:623** (the recipe layer throws for a filtered RightSemi) has no shape left. Its
  replacement is :365's flip.
- **New:**
  - `test_every_join_type_matches_the_oracle_as_a_nested_loop` (all nine; was Inner and Left);
  - `test_a_predicate_free_nested_loop_over_an_empty_probe_pads_every_build_row`;
  - `test_the_types_that_finish_are_the_build_preserving_five`.
- **`on_oracle`** crosses the sides and applies the whole ON clause per pair. That is the
  oracle for every residual and nested-loop case, written from the SQL.
- **The join-type guard** reads `cpp/src/operators/join_session.cpp`, and its nine
  `fb::JoinType_` names equal the enum.

**What the plan got wrong, and what was decided.**
- **`recipe.py` and `cudf_calls.py` went whole, not just `copy_handle`.** Once
  `recipe_join.py` is gone, nothing imports either. Their join models
  (`CudfHashJoin`, `execute_hash_join`, …) are what task 15's grep over `scripts` names.
- **The cuDF-call guard (:666) went with them. This departs from design §4.6**, which
  retargets it to the session's file. With no model left, the guard would hold a list of
  function names that nothing runs, and code kept only to satisfy a test is the
  antipattern. If the human wants that name registry kept, it is one file and one test to
  restore against `join_session.cpp`. Its free cuDF calls are `apply_boolean_mask`,
  `binary_operation`, `conditional_{inner,left}_join[_size]`, `conditional_left_semi_join`,
  `cross_join`, `gather`, `mixed_left_semi_join`, `repeat`, `sequence` and `tile`. The hash
  joins are member calls, which the guard's regex never saw.
- **The plan missed :564**, which pinned #80/#59's always-EQUAL anti and mark. It is
  flipped.
- **The nested loop's refusal beyond Inner and Left (#160) was in the model too.** With
  `capability()` gone the executor must answer every type, and it does.
- **Left unchanged:** `merged_scratch` still prices a per-call rebuild of the build side
  (#136's) into a join's scratch, where §4.3 prices the session's resident build. §4.6 asks
  the model to follow §4.1, not §4.3.

**Red-green.** The 17 cases were written first, against the old model (with only
`needs_finish` added so the file imports). **8 red, each for the reason meant**: the
nested-loop and empty-probe cases on "takes a single-batch probe", the residual-rides case
and the filtered semi on the same, the outer and Left-pads cases on the #153 refusal, the
RightSemi case on #159's, and the null-key case on its anti assertion. Then 17/17 green.
Mutations of `joins.py`, each applied by asserted single match, restored by copy and
checked with `cmp`:
- **M1**, anti and mark EQUAL again: 1 red (the null-key case).
- **M2**, the nested loop's Left/Full finish answers nothing: 2 red.
- **M3**, the nested loop never pads a Right/Full probe row: 1 red.
- **M4**, the residual ignored: 4 red. The filtered-semi case stayed green under M4, since
  every key passes `v > 50` either way. It now uses `v > 80`, where key 3 drops out, with
  a precondition on the expected labels, and goes red under M4.

**Counts, off pytest collection** (`python3 -m pytest scripts/exec_model/tests
--collect-only`): 328 at HEAD (from a `git archive` copy) → 324. The cost-report set
(every file but `test_tpch`, `test_tpch_corpus`, `test_tpcds`) is 216 → 212; `test_tpch`
stays 19 and the corpus 93. Each non-corpus file was run as CI runs it (`python3 <file>`),
at HEAD and now. Only the join file differs (21 → 17); the other ten are equal and green.

**The corpus, since every anti and mark join in it now runs under the SQL default.** No
lowering sets `null_equals_null`, so each one used to run as EQUAL and now does not. Both
corpus files ran in six `PCK_SHARD` processes over sf1, with DuckDB 1.5.4 from
`~/.local/bin`. **93 of 93 pass**: TPC-H 22, TPC-DS 71. Three of them — q21, q16 and q94 —
first failed on "plan changed". Each process had loaded the old lowering and then read the
plan golden I rewrote mid-run. Re-run alone on the new lowerings, all three pass. No corpus
answer depended on the old EQUAL rule. Then, on the final tree, `python3 -m pytest
scripts/exec_model/tests -q`: **324 passed** in 46 minutes, the corpus included, in one
process. The calibration set under `scripts/` is unaffected and green, 12 of 12, with
matplotlib installed outside the tree.

##### The device run, recorded

nebius-gpu, NVIDIA L40S, 0 MiB in use; cuDF 25.02. The final tree was synced and the two
restored files touched. `./scripts/build-test-shadgpu.sh --build` gave rc 0 with no warning
in its log. Each binary was run directly, the Rust ones with `--test-threads=1`. **924
cases, all green:**

| binary | passed |
|---|--:|
| `peacock_gpu_tests` | 2 |
| `peacock_plan_tests` | 81 |
| `peacock_join_session_tests` | 53 |
| `test_gpu_corpus` | 95 |
| `test_node_timing` | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 684 |
| `peacock_gpu_benchmarks --skip bench_` | 8 |

Not run, per the override: the sf40 pair and the `bench_` cases. Disk: 28 GB free of 96
before and after. Nothing was deleted.

**Local, rust-only, fresh on the final tree:**

| target | result |
|---|---|
| `--lib` | 761 passed, 2 ignored |
| `test_cpu_corpus` | 1031 |
| `test_corpus_goldens` | 26 |
| `test_cost_model` | 3 |
| `test_module_layout` | 18 |
| `test_ci_coverage` | 11 |
| `test_golden_format` | 43 |

0 failed anywhere and no warning. No golden moved, so there is no cost diff.

**build-test.md.** The header went 3308 → 3304 and Python 401 → 397; the sum of the N
columns was re-checked by script. "Exec-model prototype" is 216 → 212, its prose is
rewritten, and its example is renamed. "Exec-model corpus" loses `PCK_BACKEND=recipe`.
"Plan-executor" says how a join runs; its count stays 81.

**What tasks 14–16 inherit.**
- 15: every grep in its step 2 is already empty over `scripts`.
- 15: `hacks-audit.md:422` still describes `copy_handle` and its test as live. That is one
  of the join items step 3 marks resolved.
- 14: exec_model has no `GpuEmpty`, no `IS [NOT] DISTINCT FROM` and no pbench query, so
  nothing there waits on cells.

**Coordinator's note on task 13's deleted guard, for the review round.** Design §4.6 and plan task 13
step 1 keep the exec_model cuDF-call guard, pointed at `join_session.cpp`; the developer deleted it
with `cudf_calls.py`, reasoning that with no model of those calls left it checks names nothing runs.
That is a coverage removal no human authorized, so the reviewer is asked to judge it: restore it
against the session, or agree it can no longer go red for a reason that matters.

#### Round 2, plan task 14, tpch — the device cells, in batches

The tpch dataset only; tpcds and pbench are the next two dispatches. Every tpch row whose gpu
cells' last ticket closes in this chain, in the estimate's §7 order, about five queries a batch.
Each batch: registry first, then `corpus_cases.inc`, the tree synced to nebius-gpu, `--build`,
`test_gpu_corpus` filtered on the batch under `PCK_WRITE_GPU_RESULT=1`, `gpu-result.txt`
brought home with `scp` of that one file from `~/peacockdb-J` (never `--pull-results`, which
goes to shad-gpu), then the `duckdb_gpu_*` cases and the coverage guard locally.

**Running ledger.** One line per batch, written when the batch's DuckDB cases were green.

| batch | queries | gpu cells enabled | off, on which ticket | device | DuckDB |
|---|---|--:|---|---|---|
| 1 | q2, q3, q4, q5, q7 | 25 (all five modes each) | none | 25/25 in 46 s | 25/25 `duckdb_gpu_*`, coverage guard green |
| 2 | q8, q9, q10, q11, q12 | 22 (q11 at the two tp1 modes) | q11 at tp4-single, tp4-rowgroup, tp4-sized: [#199](../tickets/corpus-coverage.md#t199) | 22/25 in 235 s; q11's three tp4 cells red on the `.cpu.txt` section alone | 25/25 green, q11's three tp4 answers included, before they were turned off |
| 3 | q13, q14, q15, q16, q17 | 24 (q17's tp1-single was on already) | none | 25/25 in 34 s, q17's tp1-single re-recorded; the recording run dropped q11's three tp4 sections | 25/25 green |
| 4 | q18, q19, q20, q21, q22 | 23 (q19's tp1-single was on already; q22 at four modes) | q22 at tp4-single: #199 | 24/25 in 48 s, q19's tp1-single re-recorded; q22's tp4-single red on the `.cpu.txt` section alone | 25/25 green, q22's tp4-single answer included, before it was turned off |
| 5 | anti-join, cross-join, hash-join, join-int, left-join | 25 | none | 29/29 in 66 s, q22's four re-run to drop its tp4-single section | 25/25 green |
| 6 | nested-loop-join, nested-loop-left-join, rollup-over-join, semi-join | 14 (nested-loop-join's tp1-single was on already) | rollup-over-join at all five: [#65](../tickets/corpus-coverage.md#t65) | 15/20 in 27 s; the rollup refused by the schema hook at its first `GpuAggregate`, `__grouping_id: UInt8 vs INT32`, so no answer was recorded | 15/15 green |

No batch exposed a defect of this chain, so every batch ran. The two causes met are open tickets
outside the spec's scope, each on the mechanism its ticket names; neither is a new defect and no
ticket was filed. Next free is still **268**.

**Per query, the end state.** 133 device cells turned on; the tpch device column goes 38 → 171 of
195. Every cpu cell was already on (scan-limit's two tp1 cells stay off on #186).

| query | gpu on, by this task | gpu off, and why | tickets column |
|---|---|---|---|
| q2, q3, q5, q10, q12, q14, q18, q20, q21 | all five | — | `152 220` → none |
| q7, q8, q9 | all five | — | `152 220` → none |
| q4, q15 | all five | — | `183 220` → none |
| q13 | all five | — | `152` → none |
| q16 | all five | — | `59 80 152 220` → none |
| q17, q19 | 1r, 4s, 4r, 4z (1s was on) | — | `152` → none |
| q11 | 1s, 1r | 4s, 4r, 4z: #199 | `190` → `199` |
| q22 | 1s, 1r, 4r, 4z | 4s: #199 | `59 80 190` → `199` |
| anti-join, semi-join | all five | — | `152 183 220` → none |
| cross-join, nested-loop-left-join | all five | — | `183 220` → none |
| hash-join, join-int | all five | — | `152 220` → none |
| left-join | all five | — | `152` → none |
| nested-loop-join | 1r, 4s, 4r, 4z (1s was on) | — | `183` → none |
| rollup-over-join | — | all five: #65 | `152 220` → `65` |

Every closed ticket was struck where the row has no off cell left, and replaced where it has:
q11 and q22's off cells carry `199` and nothing else, the rollup's `65`. No tpch row carries 152,
220, 183, 190, 59 or 80 now.

**The #199 cells, read rather than guessed.** All four failed on the `.cpu.txt` section alone, at
the same five lines, and never reached the result compare. In q11 at tp4, the filter on GERMANY
keeps one nation row, which the scatter puts in lane 3. So the Inner join's build lanes 0–2 are
empty, the partial `sum` above it gets no batch in three lanes, and the keyless
`GpuAggregateBatches` at `lanes=4` reads `in_rows=[[0,0,0,16]]`. The cpu answers an identity row
for each lane (`batch_rows=[[1],[1],[1],[1]]`), the device answers none, and the merge above reads
`in_rows=[[4]]` against `[[1]]`. q22 at tp4-single is the same with the keyless avg over customer:
`in_rows=[[31186,6934,0,0]]`. Its other two tp4 modes plan the scan in one lane, which is why only
tp4-single is off. The device's answer was right each time: the four sections were recorded
before the compare, and their `duckdb_gpu_*` cases passed under `duckdb_fingerprint` (q11) and
`duckdb_exact` (q22) before the cells were turned off. The next recording cycle dropped the
sections, as the writer prunes cells the registry no longer enables.

**The rollup, as the estimate said.** All five failed in the run itself: `GpuAggregate lane 0:
the output hook refused a batch: 2 __grouping_id: UInt8 vs INT32`. The validator refuses before
the export, so no answer was recorded and there was nothing to compare.

**Against the estimate (§7), each miss with its cause, for plan task 16.**
- **q7, q8, q9: STAY on #191 predicted, 15 turned on.** #191 closed on master
  (`5a1eac45`, date-part-return-type) after the estimate was taken, and the rows had lost `191`
  before this task. The estimate's "chain B's task held" no longer held.
- **q11 at 4s, 4r, 4z and q22 at 4s: FLIP (M-L) predicted, 4 stayed off on #199.** §3 says
  "scanning every tp4 cpu golden finds this shape in q32 alone", and so it did then: q11's and
  q22's cpu cells were off on #190, so their tp4 sections did not exist. Task 8 wrote them, and the
  shape is in them. A scan of today's tp4 goldens finds it in tpch q11 ×3, q22 at tp4-single and
  tpcds q32 ×3, nowhere else.
- **The 25 STALE cells (q4, cross-join, nested-loop-left-join, semi-join, anti-join at 1r/4s/4r/4z,
  q15 at 1r, nested-loop-join at 1r/4s/4r/4z): run, as the spec asks, and all 25 on.** §6 predicted
  "FLIP, M" if run.
- **rollup-over-join ×5: NEXT on #65, confirmed**, by the schema hook, at all five modes.
- Every other tpch FLIP cell turned on: 93 of the 97 FLIP cells, plus the 25 STALE and the 15
  STAY. Categorical prediction 97 + 25 = 122 for this dataset if the STALE rows ran; measured 133.
  Task 10's order guard refused none.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S, 0
MiB in use; cuDF 25.02. Six batch cycles: the tree synced uncommitted with the board's rsync line,
`./scripts/build-test-shadgpu.sh --build` rc 0 each time (an incremental rebuild of
`test_gpu_corpus`), the batch's cells run with `PCK_WRITE_GPU_RESULT=1`, and
`testdata/goldens/tpch.sf1/gpu-result.txt` brought home by `scp` of that file from
`~/peacockdb-J` before the next sync. Then the final tree, built and run unrecorded, every binary
the override lists, `--test-threads=1` on the Rust ones. **1,057 cases, all green:**

| binary | passed |
|---|--:|
| `peacock_gpu_tests` | 2 |
| `peacock_plan_tests` | 81 |
| `peacock_join_session_tests` | 53 |
| `test_gpu_corpus` | 228 (95 + 133; 171 of them tpch cells) |
| `test_node_timing` | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 684 |
| `peacock_gpu_benchmarks --skip bench_` | 8 |

Not run, per the override: the sf40 pair and the `bench_` cases. Disk 28 GB free of 96 before and
after every build; nothing deleted. The tree was synced once more after the run, for the fixture
below, rebuilt (rc 0), and that case run on the device lib (4 passed); the remote and local
`gpu-result.txt` have the same md5.

**`gpu-result.txt`, read.** 5,546 lines added, none removed: 133 new sections, each a pure
addition. The re-recorded tp1-single sections of q17, q19 and nested-loop-join did not move. The `cudf=25.02` line is
unchanged.

**One test fixture leaned on the registry, and this task moved it.**
`corpus_golden::tests::a_mode_keyed_merge_drops_a_cell_the_registry_no_longer_enables` used tpch
q12 as the cell no registry row enables, and reads the live CSV. With q12 on, the merge kept its
section and the case went red. It now uses `mixed-join`, which #168 keeps off the device for as
long as an interval cannot cross the wire.

**Local, rust-only, fresh on the final tree:** `--lib` 761 passed, 2 ignored; `test_cpu_corpus`
1189 (1031 + 133 `duckdb_gpu_*` + 25 `device_order_*`); `test_corpus_goldens` 26;
`test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage` 11; `test_golden_format` 43; 0
failed, no warning.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base c5b3530f`: 726 compared, **0
changed, 0 regressions**. No cpu golden moved: the cells are the device's.

**build-test.md, counted off the runs.** cpu block 1823 → 1981 (`test_cpu_corpus` 1031 → 1189;
"Corpus, cpu" 1030 → 1188, the DuckDB tier 273 → 406, the order tier 25 → 50). gpu block 791 → 924
(`test_gpu_corpus` 95 → 228; "Corpus, device" 94 → 227, its cell list rewritten). Rust 2734 →
3025, grand total 3304 → 3595, the N columns summed by script.

**Tickets whose corpus lines this made stale, rewritten:** #199 (tpch q11 ×3 and q22 at
tp4-single), #65 (`tpch/rollup-over-join` now reaches it on a device), #190 (tpch q11 and q22
answer on both engines), #220 (75 rows carry it now, none of them tpch's).

**What tpcds and pbench should expect.**
- **#199 is the one mechanism this dataset found that the estimate did not.** The same scan of
  every enabled tp4 cpu section finds it in tpcds q32 at the three tp4 modes, as §3 predicted, and
  in no pbench section. A cell that turns on later at a tp4 mode can bring it, so read its
  keyless `GpuAggregateBatches` at `lanes=4` for a 0 in `in_rows` before the run.
- **Expect the device to agree with the cpu's per-node sections otherwise.** 133 of 133 cells
  outside #199 and #65 matched the `.cpu.txt` section exactly, batch lists and bytes included, at
  every mode. tpch has no NULL key and no #137 filter, though, so tpcds's filtered tp4 shuffles
  and pbench's NULL keys are new ground for the corpus tier.
- **A run is quick.** A batch of 20 to 29 tpch cells took 27–235 s on the L40S, and the whole
  device corpus 325 s. Most of a cycle is the incremental build.
- **tpcds q78 (#60) and q32 at tp4 (#199) are the estimate's other NEXT cells.** tpcds's rows
  carrying `65` (q5, q14, q18, q22, q77, q80) should refuse as the tpch rollup did, at the schema
  hook.

#### Round 2, plan task 14, tpcds — the device cells, in batches

The tpcds dataset only; pbench is the next dispatch. The tpch method unchanged: registry first,
then `corpus_cases.inc`, the tree synced, `--build`, `test_gpu_corpus` filtered on the batch under
`PCK_WRITE_GPU_RESULT=1`, `gpu-result.txt` brought home by `scp` before the next sync, then the
`duckdb_gpu_tpcds_*` and `device_order_tpcds_*` cases and the corpus tier's ten meta checks
locally. Batches in the estimate's §7 order, the rows whose every ticket closes in this chain.
Before the first run, a scan of every tp4 cpu section for a keyless `GpuAggregateBatches` at
`lanes=4` with a 0 in `in_rows` (#199's shape) found q32 at the three tp4 modes and nothing else.

**Running ledger.** One line per batch, written when the batch's DuckDB cases were green.

| batch | queries | gpu cells enabled | off, on which ticket | device | DuckDB |
|---|---|--:|---|---|---|
| 1 | q1, q3, q4, q6, q7 | 25 | none | 25/25 in 74 s | 25/25, coverage guard green |
| 2 | q8, q9, q10, q11, q13 | 25 | none | 25/25 in 56 s | 25/25 |
| 3 | q15, q16, q19, q21, q23 | 25 | none | 25/25 in 50 s | 25/25 |
| 4 | q24, q25, q26, q29, q30 | 25 | none | 25/25 in 97 s | 25/25 |
| 5 | q31, q32, q33, q34, q35 | 22 | q32 at tp4-single, tp4-rowgroup, tp4-sized: [#199](../tickets/corpus-coverage.md#t199) | 22/25 in 40 s; q32's three tp4 cells red on the `.cpu.txt` section alone | 25/25, q32's three tp4 answers included, before they were turned off |
| 6 | q37, q38, q40, q41, q42 | 24 (q37's tp1-single was on) | none | 25/25 in 35 s; the next recording dropped q32's tp4 sections | 25/25 |
| 7 | q43, q45, q46, q48, q50 | 25 | none | 25/25 in 68 s | 25/25 |
| 8 | q52, q54, q55, q56, q58 | 25 | none | 25/25 in 36 s | 25/25, q58 under `duckdb_divergent(251, …)` |
| 9 | q59, q60, q61, q62, q64 | 25 | none | 25/25 in 65 s | 25/25 |
| 10 | q65, q68, q69, q71, q73 | 25 | none | 25/25 in 43 s | 25/25 |
| 11 | q74, q75, q76, q78, q79 | 20 | q78 at all five: [#60](../tickets/corpus-coverage.md#t60) | 20/25 in 51 s; q78 red on its answer alone | 20/20; q78's five red, `2.7800000000000002` against DuckDB's `2.78` |
| 12 | q81, q82, q83, q84, q85 | 22 (q82, q84, q85's tp1-single were on) | none | 25/25 in 45 s; q78's sections dropped | 25/25 |
| 13 | q87, q88, q90, q91, q92 | 19 (q88, q90 at the two tp1 modes, as on the cpu) | none | 19/19 in 29 s | 19/19 |
| 14 | q93, q94, q95, q96, q97, q99 | 27 (q96 at the two tp1 modes) | none | 27/27 in 39 s | 27/27 |
| 15 | q66 | 5 | none | 5/5 in 24 s | 5/5, under `duckdb_divergent(251, …)` |

Batch 15 came out of the probe below, not out of the estimate's FLIP list. No batch exposed a
defect of this chain's code. Every cause met is an open ticket outside the spec's scope, on the
mechanism its ticket names; no ticket was filed, and the next free is still **268**.

**The probe of the rows whose own tickets stand.** q2, q5, q14, q17, q18, q22, q39, q66, q77 and
q80 carry a ticket that does not close here, so their cells are not this task's to enable. But
their `152`/`220`/`212` tags could be struck only if the other ticket really holds them. So I
enabled all ten in the `.inc` alone, ran them on the device without recording, and restored the
file. 8 of 50 passed:

| query | what the device met, every mode unless named | tickets column |
|---|---|---|
| q14, q18, q22, q77, q80 | #65: `__grouping_id: UInt8 vs INT32` at the first `GpuAggregate` | `65 220` / `65 152 212 220` / `65 152 220` → `65` |
| q5 | #210: `return_amt` and `net_loss` `Decimal128(7, 2) vs FLOAT64` at a `GpuProject`, before #65 | `65 152` → `65 210` |
| q17 | tp1 modes: #225's Welford names at the `GpuAggregate`; the three tp4 modes pass | `205` → `205 225` |
| q39 | #225's Welford names, at the aggregate below #57's filter | `57 220` → `57 225` |
| q2 | every per-node section matches, the answer differs; see below | `56 152` → `56 60` |
| q66 | **all five pass** | `55 152 220` → none, enabled as batch 15 |

- **q66 is enabled.** #55's own text calls itself most likely stale, and here its five cells
  matched every cpu section and the golden, then met DuckDB under the line's #251 oracle. A green
  cell held off by a ticket that never showed is the shape this task exists to turn on. #55 stays
  open: its fix asks for a walk test, which is not this task's. If the coordinator reads q66 as
  #55's to enable, revert batch 15: five cells, one row, 50 lines of `gpu-result.txt`.
- **q17's tp4 cells pass and stay off.** Its line is `duckdb_divergent(205)`, and chain K's
  empty-sorts rewrites q17's result section. Enabling three cells of a row another chain is moving
  invites a conflict for nothing.
- **q2, read further.** One more device run of q2 at tp1-single, recorded to a scratch
  `gpu-result-25.02.txt` with q2 enabled on the remote's registry copy (restored, md5-checked).
  The answer is over the cap there and under it in `mini.result.txt`: the device's digits are
  longer. Its fingerprint gives 2,513 rows, every column non-null, and each column's sum, min and
  max equal to the cpu's rows to the printed digit. So the rows agree and the digits do not —
  #60's one-ulp `round`, seven columns of it, as the estimate said. #56's shape did not show.

**Per query, the end state.** 339 device cells turned on: 334 of the estimate's FLIP cells, and
q66's five. The tpcds device column goes 4 → 343 of 396, on 71 lines. No cpu cell moved: q24 and
q54's `duckdb_oracle` left `duckdb_none` in task 8, already `duckdb_exact`.

| query | gpu on, by this task | gpu off, and why | tickets column |
|---|---|---|---|
| the 62 lines of §7's FLIP rows bar those below | all five | — | `152 220`, `220`, `152`, `190`, `63`, `59 80 152 220`, `80 152 220` → none |
| q37, q82, q84, q85 | 1r, 4s, 4r, 4z (1s was on) | — | `152` → none |
| q88, q90, q96 | 1s, 1r | 4s, 4r, 4z on both engines: #199 | `152 199 220` → `199` |
| q32 | 1s, 1r | 4s, 4r, 4z: #199 | `220` → `199` |
| q78 | — | all five: #60 | `152` → `60` |
| q66 | all five | — | `55 152 220` → none |
| q2, q5, q14, q17, q18, q22, q39, q77, q80 | — | all five, as the probe table says | as the probe table says |

No tpcds row carries 152, 220, 212, 190, 63, 59 or 80 now.

**The #199 cells, read.** q32's three tp4 cells failed on the `.cpu.txt` section alone, at the
merge above the keyless `GpuAggregateBatches`: `in_rows=[[1]]` on the device against `[[4]]`.
Its keyless sum sits over an Inner join whose build lands in lane 2 alone (`in_rows=[[0,0,1,0]]`),
so the cpu answers an identity row for each of the four lanes and the device one. The device's
answer, `12129.44` at all three modes, was recorded before the compare and met DuckDB under
`duckdb_exact` before the cells were turned off.

**q78, read.** All five cells failed on the answer alone: every per-node section matched. Sorted
as sets against `mini.result.txt`, 100 rows each, exactly five rows differ, each in `ratio` alone:
`2.7800000000000002`, `1.3599999999999999`, `2.5300000000000002`, `1.6400000000000001`,
`1.9100000000000001` against `2.78`, `1.36`, `2.53`, `1.64`, `1.91`. That is #60's emulation to the
cell. The five modes' answers are identical.

**Against the estimate (§7), for plan task 16.**
- **Every tpcds FLIP cell turned on: 334 of 334**, the M and M-L rows included (q6, q9, q24, q41,
  q54, q64, q71, q75, q97 had never run on a device). Task 10's order guard refused none.
- **NEXT, all eight confirmed:** q78 ×5 on #60, q32 at the tp4 modes on #199, each on the
  mechanism §3 names.
- **STAY, 50 predicted off, 45 stayed off.** q66 ×5 turned on: #55 was stale, as its ticket and
  §4 suspected. Of the 45, the cause §4 names first was the one met for q14, q18, q22, q77 and q80
  (#65). It was not for q5 (#210 before #65, §3's "also"), q17 (#225 at tp1, §3's "then"; tp4
  passes), q39 (#225, not #57) or q2 (#60, §3's "then"; #56 never showed).
- **The 9 cpu-and-gpu STAY cells of q88, q90 and q96 at tp4** stayed off on #199, as §4 said.
- Categorical prediction for tpcds: 334 FLIP. Measured: 339 on.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S, 0
MiB in use; cuDF 25.02. Sixteen cycles — fifteen batches and the probe — each the tree synced
with the board's rsync line and `./scripts/build-test-shadgpu.sh --build` rc 0, 18–42 s, an
incremental rebuild of the four Rust binaries. Then the final tree, built (rc 0, no warning in its
log) and run unrecorded, every binary the override lists, `--test-threads=1` on the Rust ones.
**1,396 cases, all green:**

| binary | passed |
|---|--:|
| `peacock_gpu_tests` | 2 |
| `peacock_plan_tests` | 81 |
| `peacock_join_session_tests` | 53 |
| `test_gpu_corpus` | 567 (228 + 339; 343 of them tpcds cells), 685 s |
| `test_node_timing` | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 684 |
| `peacock_gpu_benchmarks --skip bench_` | 8 |

Not run, per the override: the sf40 pair and the `bench_` cases. Disk 28 GB free of 96 before and
after every build; nothing deleted. The remote and local `gpu-result.txt` have the same md5.

**`gpu-result.txt`, read.** 24,568 lines added, none removed: 339 new sections, no fingerprint
among the 343. The re-recorded tp1-single sections of q37, q82, q84 and q85 did not move. The
`cudf=25.02` line is unchanged. q32's tp4 and q78's sections were written and then pruned by the
next recording, as the writer drops cells the registry no longer enables.

**Local, rust-only, fresh on the final tree:** `--lib` 761 passed, 2 ignored; `test_cpu_corpus`
1595 (1189 + 339 `duckdb_gpu_*` + 67 `device_order_*`); `test_corpus_goldens` 26;
`test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage` 11; `test_golden_format` 43; 0
failed, no warning.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base 302ed3a3`: 726 compared, **0
changed, 0 regressions**. No cpu golden moved.

**build-test.md, counted off the runs.** cpu block 1981 → 2387 (`test_cpu_corpus` 1189 → 1595;
"Corpus, cpu" 1188 → 1594, the DuckDB tier 406 → 745, the order tier 50 → 117). gpu block 924 →
1263 (`test_gpu_corpus` 228 → 567; "Corpus, device" 227 → 566, its tpcds sentence rewritten).
Rust 3025 → 3770, grand total 3595 → 4340, the N columns summed by script.

**`corpus_cases.inc`'s tpcds comments** said which of #152 and #220 held each batch's device
cells. They now say the device column once, at the top of the section, and each batch comment
names only its off cells and their tickets.

**Tickets whose corpus lines this made stale, rewritten:** #199 (q32 ×3), #60 (q78 and q2,
measured), #65 (five rows reach it on a device, q5 meets #210 first), #210 (q5, its first corpus
line), #225 (q39, q17 at tp1), #55 (q66 on, the walk test still owed), #56 (q2 off on #60), #57
(q39 meets #225 first), #205 (q17's tp4 cells pass), #220 (ten pbench rows carry it, no tpch or
tpcds row), #190 (q24 and q54 answer on both engines), #63 (q9 answers at every mode), #212 (q77
runs past it, off on #65).

**What pbench should expect.**
- **The device agreed with the cpu's per-node sections wherever a cell ran**, at every mode: of
  351 cell runs in the batches, 343 passed first time, and the 8 that did not were #199's three
  and #60's five, each on its known mechanism. tpcds is the first dataset where #137's `IS NOT NULL` filters and real NULL join keys
  (`ss_sold_date_sk` holds 129,850) ran on the device. No divergence showed, so nothing here asks
  the cpu to mirror the device.
- **The schema hook is where an out-of-chain ticket shows first.** #65, #210 and #225 each refused
  a batch before any answer existed. pbench's `rollup-small-keys` carries `65`, and should
  refuse the same way.
- **#199 can only come from a tp4 cell.** None of pbench's tp4 sections has the shape today. A
  pbench row first enabled at tp4 can bring it, so scan its keyless `GpuAggregateBatches` for a 0
  in `in_rows` before the run.
- **A run is quick.** A batch of 19–27 tpcds cells took 29–97 s; the whole device corpus, 567
  cells, 685 s. Most of a cycle is the sync and the build, under a minute together.

**Coordinator's decision on q66 (2026-10-14).** Its five cells stay on: they pass on the device and
against DuckDB, and a green cell left off is coverage withheld for no reason. #55 stays open for the
walk test it still owes; its row no longer carries the cells, which is the record of what it blocks.

#### Round 2, plan task 14, pbench — the join rows on both engines, and the task's closing steps

The pbench dataset, cpu and gpu. Unlike tpch and tpcds, most pbench join rows had no cpu cell:
task 9 made twenty newly planned queries enable-able, 9b `empty-side-left-join`, 9c
`indf-full-join`, and task 8 the five `190` rows. So each of the first four batches turned the cpu
cells on first: registry, then `corpus_cases.inc` with the line's `duckdb_oracle` off
`duckdb_none`, then `PCK_UPDATE_SECTIONS=1` over the batch's `cpu_pbench_*` cases, then the whole
`test_cpu_corpus`. The device half then ran as tpch's and tpcds's did: registry, `.inc`, the tree
synced, `--build`, `test_gpu_corpus` filtered on the batch under `PCK_WRITE_GPU_RESULT=1`,
`gpu-result.txt` brought home by `scp` before the next sync (the md5s compared each time), then
`test_cpu_corpus` whole. Before the first device run, every enabled pbench tp4 section was scanned
for #199's shape: pbench has no keyless `GpuAggregateBatches` at `lanes=4` at all.

**Running ledger.** One line per batch, written when the batch's DuckDB cases were green.

| batch | queries | cpu cells | gpu cells | off, on which ticket | device | DuckDB, and the tier |
|---|---|--:|--:|---|---|---|
| 1 | `left-`, `right-`, `full-join-residual`, `probe-exists-residual`, `probe-not-exists-residual` | 25 | 25 | none | 25/25 in 22 s | 30/30; `test_cpu_corpus` 1752 |
| 2 | `anti-null-preserved-condition`, `exists-or-mark`, `mark-cross-residual`, `not-exists-null-keys`, `not-in-correlated`, `not-in-under-or`, `not-in-uncorrelated` | 32 | 32 | `not-in-uncorrelated` at tp4, both engines: [#267](../tickets/corpus-coverage.md#t267) | 32/32 in 33 s | 39/39, `not-in-uncorrelated` under `duckdb_divergent(205)`; 1791 |
| 3 | `nl-right`, `nl-full`, `nl-left-semi`, `nl-left-anti`, `nl-right-semi`, `nl-right-anti`, `nl-mark` | 35 | 35 | none | 35/35 in 33 s | 42/42; 1833 |
| 4 | `nl-inner`, `nl-left`, `nl-projection`, `nl-left-decimal`, `like-column-pattern`, `indf-full-join`, `empty-side-left-join` | 35 | 30 | `like-column-pattern`'s five device cells: [#246](../tickets/joins.md#t246) | 40/40 in 45 s (10 re-records); the `like` probe 0/5, unrecorded | 37/37; 1869 |
| 5 | `inner-join-hot-keys`, `left-`, `right-`, `full-join-null-keys`, `exists-null-keys`, `finish-without-probe` | — | 30 | none | 30/30 in 37 s | 30/30; 1905 |
| 6 | `sparse-build-right`, `-full`, `-anti`, `not-not-in`, `not-or-not-in` | — | 25 | none | 25/25 in 29 s | 25/25; 1935 |
| 7 | `decimal15-key-join`, `ts-key-join`, `uint-key-join`, `cross-projection`, `scalar-subquery-cross` | — | 21 (`uint-key-join`'s 1s was on; `scalar-subquery-cross` at the two tp1 modes) | `scalar-subquery-cross` at tp4 stays off on both engines: [#199](../tickets/corpus-coverage.md#t199) | 22/22 in 28 s (one re-record) | 21/21; 1960 |

No batch exposed a defect of this chain's code. One line met an open ticket on a mechanism the
design keeps, below. No ticket was filed; the next free is still **268**.

**`not-in-uncorrelated`, read.** Its first DuckDB case failed: "ours has 0 columns against 1 in
DuckDB's". Both answers have zero rows, so `sub` holds a NULL and `NOT IN` is empty, as SQL says.
Our section is a bare `++`/`++`, because the root receives no batch at all. The rewrite plans a
`GpuCrossJoin` whose build is `count(*) FROM sub WHERE s_y IS NULL` filtered by `= 0`, so it has zero
rows. Design §3.8 says an empty Inner build owes nothing, so the lane ends without a call. Then the
Right nested loop above it gets no probe batch, and the unload renders no header. That is
[#205](../tickets/corpus-coverage.md#t205)'s second half, "an empty answer must still have a
schema", reached without a sort. `tpcds/q17` is the same `++` under `duckdb_divergent(205)`, so the
line took that oracle. The device answers the same `++` at both tp1 modes (`gpu-result.txt`). Not
this chain's defect: the cpu and the device agree with each other and with the design's empty-build
rule, and #205 names the unload's fix.

**`like-column-pattern`, measured before striking `190`.** With its device cells enabled in the
`.inc` alone, an unrecorded run failed all five at the nested loop's probe: `LIKE pattern must be a
string literal`. That is #246's own refusal. The `.inc` was restored from a saved copy and touched.
The next sync carried it.

**Per row, the end state.** 127 cpu cells and 198 device cells turned on. pbench goes 137 → 264
cpu cells and 51 → 249 device cells. 26 lines left `duckdb_none`: 16 to `duckdb_exact`, 9 to
`duckdb_fingerprint` with `gpu_oracle` `live_cpu` (over the cap, as the line's other fingerprinted
rows are), and 1 to `duckdb_divergent(205)`. Three `duckdb_none` lines remain, all refused at plan
time.

| row | on, by this task | off, and why | tickets column |
|---|---|---|---|
| `left-`, `right-`, `full-join-residual` | cpu and gpu, all five | — | `153` → none |
| `probe-exists-residual`, `probe-not-exists-residual` | cpu and gpu, all five | — | `159` → none |
| `anti-null-preserved-condition`, `exists-or-mark`, `mark-cross-residual`, `not-exists-null-keys`, `not-in-correlated`, `not-in-under-or` | cpu and gpu, all five | — | `59 80` → none |
| `not-in-uncorrelated` | cpu and gpu at 1s, 1r | tp4, both engines: #267 | `59 80 267` → `267` |
| the seven `nl-*` of #160 | cpu and gpu, all five | — | `160` → none |
| `nl-inner`, `nl-left`, `nl-projection` | cpu and gpu, all five | — | `152 190` → none |
| `nl-left-decimal` | cpu and gpu, all five | — | `190 215` → none |
| `like-column-pattern` | cpu, all five | gpu, all five: #246 | `190 246` → `246` |
| `indf-full-join` | cpu and gpu, all five | — | `155 160` → none |
| `empty-side-left-join` | cpu and gpu, all five | — | `155` → none |
| `inner-join-hot-keys`, `right-join-null-keys`, `decimal15-key-join`, `ts-key-join`, `not-not-in`, `not-or-not-in`, `sparse-build-right`, `sparse-build-anti` | gpu, all five | — | `152 220` → none |
| `left-join-null-keys`, `full-join-null-keys`, `sparse-build-full` | gpu, all five | — | `152` → none |
| `exists-null-keys`, `finish-without-probe` | gpu, all five | — | `220` → none |
| `uint-key-join` | gpu at 1r, 4s, 4r, 4z (1s was on) | — | `152` → none |
| `cross-projection` | gpu, all five | — | `63` → none |
| `scalar-subquery-cross` | gpu at 1s, 1r | tp4, both engines: #199 | `63 199` → `199` |
| `cross-empty-build` | — | all, refused at plan time: #256 | `208 256` → `256` |
| `outer-on-true-empty` | — | all, refused at plan time: #256 | `160 256` → `256` |

Untouched: `in-is-null` (#250), the three float rows (commented out, #243), and the two non-join
rows `rollup-small-keys` (#65) and `timestamp-s-key-group` (#264). `208` and `160` were struck from
the two #256 rows by step 3's rule: every off cell there carries `256`. #208's evidence is its node
pins, and #256 names `cross-empty-build` as the row that shows it once the scan lifts.

**Step 3 across all three datasets.** A scan of every registry row for the eighteen tickets this
task closes (155, 152, 173, 212, 159, 59, 80, 137, 220, 190, 207, 208, 136, 153, 160, 215, 63,
154) finds none. tpch's and tpcds's rows were already clean after their dispatches, so this pass
struck only pbench's.

**Step 2b, `collapse-not-in`: not reached.** pbench.md's collapse table asks for the
`GpuMergePartitions` that puts the rewrite's joins against one-row counts in one lane, read off the
tp4 plan goldens. All three tp4 sections of `not-in-uncorrelated` are a `refused:` line: #267's
"a finalizing merge over several lanes needs its input hashed …". The rewrite's `count(*)` over
`sub` is a `mode=Single` aggregate that the planner splits across four lanes. Validation refuses
before any join is placed, so there is no merge to read. At tp1 every node is `lanes=1` and nothing
collapses. The reading moves with #267's fix; the other three collapses stand as pbench-detail.md
recorded them.

**`gpu-result.txt`, read.** 552,495 lines added and none removed (git's numstat shows 226 removed,
which is its alignment; a section-by-section comparison against HEAD finds all 51 old sections
unmoved and none lost). 198 new sections, 90 of them fingerprints. The `cudf=25.02` line is
unchanged. The file is 6.1 MB, against tpcds's 2.4 MB. pbench's answers are its tables' rows: the
`not-in-*` sections run 18,000 lines at each of five modes. The eleven re-recorded sections
(`nl-left-anti` and `nl-left-semi` at all five, `uint-key-join` at 1s) did not move.

**Goldens moved, by kind.** No `.plans.txt`, `recipe-payloads.txt` or `duckdb-result.txt`, and no
tpch or tpcds file. The section diff against HEAD finds exactly the 26 rows in each of the five
`.cpu.txt` and five `.cost.txt` (25 at tp4, where `not-in-uncorrelated` stays `skipped:`) and in
`mini.result.txt`, nothing else. Each was a `skipped:` stub. `test_cost_model` re-derives every
`.cost.txt` section green.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base bf4734f7`: 726 compared, **0
changed, 0 regressions**. The 127 new pbench sections have no base section, so the diff has
nothing to compare them with; no existing section moved.

**The device run, recorded.** nebius-gpu (`computeinstance-e00tnrse7ayntnzcyt`), NVIDIA L40S, 0
MiB in use; cuDF 25.02. Seven batch cycles, each the tree synced with the board's rsync line and
`./scripts/build-test-shadgpu.sh --build` rc 0 in 22–25 s, with `df -h ~` before and after. Then
the final tree, built (rc 0, no warning in its log) and run unrecorded, every binary the override
lists, `--test-threads=1` on the Rust ones. **1,594 cases, all green:**

| binary | passed |
|---|--:|
| `peacock_gpu_tests` | 2 |
| `peacock_plan_tests` | 81 |
| `peacock_join_session_tests` | 53 |
| `test_gpu_corpus` | 765 (567 + 198; 249 of them pbench cells), 690 s |
| `test_node_timing` | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 684 |
| `peacock_gpu_benchmarks --skip bench_` | 8 |

Not run, per the override: the sf40 pair and the `bench_` cases. Two comment-only edits to
`corpus_cases.inc` followed that run, so the tree was synced once more and rebuilt (rc 0).
`--list` gave 765 again, and `not-in-uncorrelated` and the registry case passed on it. Disk 28 GB
free of 96 before and after every build; nothing deleted. The remote and local `gpu-result.txt`
files have the same md5 in all three datasets. Logs on the host: `~/t14pb-b{1..7}.log`,
`~/t14pb-likeprobe.log`, `~/t14pb-full/`.

**Local, rust-only, fresh on the final tree:** `--lib` 761 passed, 2 ignored; `test_cpu_corpus`
1960 (1595 + 127 cpu cells + 198 `duckdb_gpu_*` + 40 `device_order_*`); `test_corpus_goldens` 26;
`test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage` 11; `test_golden_format` 43; 0
failed, no warning.

**build-test.md, counted off the runs.** cpu block 2387 → 2752 (`test_cpu_corpus` 1595 → 1960;
"Corpus, cpu" 1594 → 1959: cpu cells 726 → 853, the DuckDB tier 745 → 943, the order tier
117 → 157). gpu block 1263 → 1461 (`test_gpu_corpus` 567 → 765; "Corpus, device" 566 → 764).
Rust 3770 → 4333, grand total 4340 → 4903, the N columns summed by script. Prose rewritten: the
"Corpus, cpu" line count and the out-entirely lines (29 → 3), the oracle tally, and pbench's
sentences in "Corpus, device".

**`corpus_cases.inc`'s pbench header** said 30 device cells were on and 29 rows had no cpu cell.
It now names the off cells and their tickets. `not-in-uncorrelated`'s line carries a trailing
comment for #205, as `timestamp-s-key-group`'s does for #264.

**Tickets whose corpus lines this made stale, rewritten:** #153, #80, #59, #159, #160 (their pbench
rows answer on both engines), #215 (`nl-left-decimal` answers), #190 (no row carries it;
`like-column-pattern` is #246's), #208 and #256 (the two rows carry `256` alone), #207 and #63
(`cross-projection` answers; `scalar-subquery-cross` at tp1 on the device, tp4 on #199), #212 (the
`sparse-build-*` rows answer), #220 (no row carries it), #246 (measured now), #267 (tp1 answers),
#205 (`not-in-uncorrelated` is its second corpus query).

**Against the estimate, for plan task 16.** The estimate counts tpch and tpcds only, and its note
under §2 moves the cpu figure to 48 once pbench's five `190` rows landed. Measured for those five:
25 cells on, every mode of each, `like-column-pattern`'s included, so join-backend's cpu total is
the note's 48. The other 102 pbench cpu cells and all 198 device cells are outside the estimate. The spec's Registry section asks for
`empty-side-left-join` and `indf-full-join` "with the rest". Both are on at every mode on both
engines.

**Task 14's totals, every dataset.** Cells turned on by plan task 14, and the 23 cpu cells of its
step 1 that landed in plan task 8. "Off" lists only cells on a row this task worked on.

| dataset | engine | on | off, on which ticket |
|---|---|--:|---|
| tpch | cpu | 10 (task 8: q11, q22) | — |
| tpch | gpu | 133 | q11 ×3 and q22 ×1 at tp4: #199; `rollup-over-join` ×5: #65 |
| tpcds | cpu | 13 (task 8: q24, q54 ×10, q77 ×3) | — |
| tpcds | gpu | 339 | q32 ×3 at tp4: #199; q78 ×5: #60; the probed rows' 45 on their own tickets (#65, #210, #225, #60, #205) |
| pbench | cpu | 127 | `not-in-uncorrelated` ×3 at tp4: #267; `scalar-subquery-cross` ×3 at tp4: #199; `cross-empty-build`, `outer-on-true-empty` ×10: #256; `in-is-null` ×5: #250 |
| pbench | gpu | 198 | the same 21 as the cpu, plus `like-column-pattern` ×5: #246 |
| **all** | cpu | **150** | |
| **all** | gpu | **670** | |

Against the estimate's tpch-and-tpcds prediction: gpu 431 FLIP categorically (about 306 weighted),
472 measured. cpu 23 predicted for tpch and tpcds, 23 measured.

#### Round 2, plan task 15 — nothing of the old join path survives

**What shipped.**
- **C++.** `cpp/src/operators/join.cpp` deleted (540 lines: `execute_hash_join`,
  `execute_cross_join`, `execute_nested_loop_join`) with its `cpp/CMakeLists.txt` line, its three
  `operators.h` declarations, its three `dispatch.cpp` arms and names, and the three
  `node_children` arms in `node_session.cpp`. The map arm's mismatched-partition refusal stays, as a
  guard for any multi-child node, and loses its "multi-partition joins are not implemented yet"
  clause.
- **fbs.** `CudfHashJoin`, `CudfCrossJoin` and `CudfNestedLoopJoin` removed, tables and union
  members. The union is renumbered, so every payload digest moved: all 22 `sha256=` lines of
  `recipe-payloads.txt`, rewritten under `UPDATE_CANONICAL=1 PEACOCK_REWRITE_RECIPE_BYTES=1`, and
  nothing else in that file or any other golden. Red first: the plain run failed
  `the_payload_golden_carries_what_each_call_hands_the_executor` on the digests alone.
- **Rust.** `wire/fb_text.rs` and `wire/read.rs` lose their arms for the three tables, and `read.rs`
  its `pair` helper, which only they called. Comments: `plan_goldens.rs` (the coverage case's
  `CudfHashJoin{LeftAnti}` example, "ten mode goldens", "both project roles"),
  `memory_estimation.rs` (a history clause), `wire/writer.rs` (`CudfScan` called the only leaf
  beside `left`/`right` tables), `tests/end_to_end.rs` (q77 "refused by name" on #212, which
  closed), `test_join_session.cpp` (`join.cpp` named as the copy's site).
- **Already gone before this task, each checked:** `wire/join.rs`'s recipe writers, `attach.rs`'s
  `cross_join`, the join `Input`s and `ProjectRole`s and `FbKind`s, `NestedLoopJoinType`,
  `empty_build_answers_nothing`, `feeds_owing_build` and the scatter-drop exception (the emitter's
  drop is unconditional, `partitioned.rs`), `dimensions.rs`'s use of them, the cpu's `Calls`,
  `GpuJoin`'s call lists, `planner/nulls.rs`, the probe-side coalesce, and exec_model's recipe join.
- **One C++ warning that was not the join's.** `expr.cpp` forward-declared a
  `build_column_binary(const fb::Expr*, …)` that nothing defines (the definition takes a
  `BinaryExprNode`), the one `-Wunused-function` in the tree. The declaration is deleted.

**Step 2's checks, quoted for the PR.** Each exits 1 with no output, on the final tree:

```
$ git grep -nE 'BuildSideCopy|BatchCopy|AccumulatedKeys|Input::BuildSide\b|ProbeKeys|NullPad|ProjectRole::Narrow' -- peacockdb-core scripts
$ git grep -nE 'FbKind::(HashJoin|CrossJoin|NestedLoopJoin)|CudfHashJoin|CudfCrossJoin|CudfNestedLoopJoin' -- peacockdb-core cpp flatbuffers scripts
$ git grep -nE 'execute_(hash|cross|nested_loop)_join|without_build|feeds_owing_build|empty_build_owes_its_probe' -- peacockdb-core cpp scripts
$ git grep -nE 'copy_of|build_copy|finish_without_keys|answers_in_one_call|per_call_join_type|finish_join_type|refuse_null_unsafe_joins|copy_handle|recipe_join' -- peacockdb-core scripts
$ git grep -n 'multi-partition joins are not implemented' -- cpp
```

Before this task the second named the fbs tables, `dispatch.cpp`, `node_session.cpp`, `join.cpp`,
`operators.h`, `fb_text.rs`, `read.rs` and a `plan_goldens.rs` comment; the third `join.cpp`,
`dispatch.cpp` and `operators.h`; the fifth `node_session.cpp:665`.

**The dead-code counts.**
- **C++: 0.** No `-Wall` in `cpp/CMakeLists.txt`, so the count is taken by recompiling every
  translation unit `ninja -t commands` lists for `peacock_gpu`, `peacock_plan_tests`,
  `peacock_join_session_tests`, `peacock_cpu_tests` and `peacock_gpu_tests` (22 of them) with
  `-fsyntax-only -Wunused-function`: 1 once `join.cpp` was gone (not measured with it), 0 after
  the `expr.cpp` fix. The `.cu` kernel is not
  in the count.
- **Rust: 25, none of them the join's.**
  `CUDF_ROOT=…/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core --features gpu 2>&1 | grep -c 'never used\|never constructed'`
  gives 25. Every one is instrumentation that only test and benchmark binaries call — timing,
  NVTX, the RMM pool, measurements — in `executor/{mod,instrument}.rs` and
  `executor/driver/{mod,measurements,index}.rs`, files this branch has not changed since
  `3d9a122e`. The plan's "is 0" was never true of the crate; the join half of it is.
  `coding-style.md` calls such a warning the honest signal, so nothing was done to it.
- **A trap, paid for once.** To count at HEAD I built a `git archive` of it with the workspace's
  `target-cudf-rapids-cuda-12.2`. The two build scripts bake `CARGO_MANIFEST_DIR` in, so the shared
  dir then pointed at the deleted copy and the next workspace build failed in cmake. `cargo clean -p
  peacockdb-ffi -p peacockdb-core` in that dir (locally, 10.4 GiB) restored it. Count at HEAD not
  taken; never share a target dir with another tree.

**The wiki.**
- `architecture.md`'s Joins section rewritten from the design and the code: the session on both
  engines, the type table, the matchers, NULL keys, the empty build and `owes_nothing`, pricing,
  where the sides run, #137's filter, and the planner's three rewrites (`NOT IN`,
  `__rowmarker__`, `GpuEmpty`). Every sentence the rounds listed as falsified is fixed: the node
  table (`GpuHashJoin`'s row, a `GpuEmpty` row), the hash-skew exception, the measured memory rule
  on a build-preserving join's residency and the scatter-lane clause, the determinism paragraph
  (spec risk 6: the same rows in the same batches, not the same order, and the order guard), the
  zero-row breaks, the wire kind count (3 of 13 unwritten), both seq tables, the `CudfScan` and
  `CudfProject` rows, "What the frozen surface costs", the column-index table and counts (19 and
  32), the cuDF options rows, and the flat-buffer field table. "Join types and NULL key equality"
  is gone into the Joins section. The page is 8 lines shorter.
- `reports/hacks-audit.md`: P1 resolved (its #199 half still open), and the Joins section is one
  resolved line each for 5, 11, 12, P4, P8, P9, P10, P13.
- `build-test.md`: the Join session row loses two history clauses. No count moved: the binaries
  ran the counts the page has.
- Not done here: `join-rewrite-design.md` to the archive (spec step 9). The helper archives specs at
  merge, and code comments still cite its sections.

**Red-green.** A removal has no red of its own. The payload digests went red before their rewrite,
and the five greps were non-empty before and empty after.

**Local, rust-only, fresh on the final tree:** `--lib` 761 passed, 2 ignored; `test_cpu_corpus`
1960; `test_corpus_goldens` 26; `test_cost_model` 3; `test_module_layout` 18; `test_ci_coverage`
11; `test_golden_format` 43; 0 failed, no warning. `/tmp/dkb-cppbuild` built `peacock_gpu` and the
four test binaries with no warning; `peacock_cpu_tests` 15 passed. Nothing under `scripts/` but a
README moved, so exec_model's pytest was not re-run.

**Cost gate**, `cargo run -q -p cost-report -- --cost-diff --base 889ca031`: 853 compared, 0
changed, 0 regressions.

**The device run, recorded.** nebius-gpu, NVIDIA L40S, 0 MiB in use; cuDF 25.02. The final tree
synced with the board's rsync line (`join.cpp`'s deletion carried by `--delete-after`), then
`./scripts/build-test-shadgpu.sh --build` rc 0 with no warning in its log; the 14 changed code
and golden files have the same md5 on both sides. Every binary the override lists, run directly,
`--test-threads=1` on the Rust ones. **1,594 cases, all green:**

| binary | passed |
|---|--:|
| `peacock_gpu_tests` | 2 |
| `peacock_plan_tests` | 81 |
| `peacock_join_session_tests` | 53 |
| `test_gpu_corpus` | 765, 692 s |
| `test_node_timing` | 1 |
| `peacockdb_core_gpu_lib gpu_tests::` | 684 |
| `peacock_gpu_benchmarks --skip bench_` | 8 |

Not run, per the override: the sf40 pair and the `bench_` cases. Disk 27–28 GB free of 96 before
and after; nothing deleted there. Logs: `~/t15-build3.log`, `~/t15-dev/`.

**A slip, reverted.** One `git rm --cached` on `join.cpp` touched the index; `git restore --staged`
put it back at once, and the index matches HEAD.

#### Round 2, plan task 16 — the estimate compared, and the record

Markdown only. Scored against the registry at the end of plan task 14 (`889ca031`) and against
the estimate's base `fc3b0b55`, by script (`csv` over `cost-registry.csv` at both revisions).
The estimate counts tpch and tpcds only; pbench is outside it.

**§7, each miss with its cause.** Every other cell §7 called FLIP turned on, and no other cell
turned on.
- **tpch q11 at 4s, 4r, 4z and q22 at 4s — FLIP (M-L), stayed off on #199.** §3's scan of the
  tp4 cpu goldens ran while their cpu cells were off on #190, so the sections it would have read
  did not exist. Plan task 8 wrote them, and the keyless merge over an empty lane is in them.
- **tpch q7, q8, q9, all five modes — STAY on #191, turned on.** #191 closed on master
  (`5a1eac45`) after the estimate was taken.
- **tpch's 25 STALE cells** (q4, cross-join, nested-loop-left-join, semi-join, anti-join at
  1r/4s/4r/4z; q15 at 1r; nested-loop-join at 1r/4s/4r/4z) **— turned on.** The spec told this task
  to run them; §6 had said FLIP (M) if run.
- **tpcds q66, all five modes — STAY on #55, turned on.** #55 was stale, as §4 and its own text
  suspected; the cells met DuckDB under the line's #251 oracle. #55 stays open for its walk test.

**NEXT ISSUE, each confirmed.**
- tpch rollup-over-join ×5 on #65: confirmed, at the schema hook (`__grouping_id: UInt8 vs INT32`).
- tpcds q78 ×5 on #60: confirmed, five `ratio` cells one ulp off, as #60's emulation said.
- tpcds q32 at 4s/4r/4z on #199: confirmed, the merge reading `[[1]]` against `[[4]]`.
- The next issues behind STAY rows: q2 → #60 confirmed, #56 never showed; q5 → #210 confirmed,
  ahead of #65; q17 → #225 confirmed at the tp1 modes, its tp4 cells pass; q39 → #225 confirmed,
  ahead of #57; q7/q8/q9 → #191 alone, confirmed by their turning on once it closed.
- "Not ruled out": the Float64→Decimal128 residual thresholds (q1, q30, q81, q32, q92, q58) did
  not show: q32's three tp4 cells failed on #199 alone, and every other cell of those queries passed.

**§8, the eight statements.**
1. **True.** cpu: 551 on at `fc3b0b55`, 589 now, exactly the 38 FLIP cells turned on (q11, q22,
   q24, q54 ×5; rollup-over-join, q5, q18, q22, q80, q77 at the three tp4 modes), none off. The 11
   off are tpcds q88/q90/q96 at tp4 and tpch scan-limit at 1s/1r.
2. **True.** The 13 NEXT cells are off with `65`, `60` and `199` on their rows.
3. **False.** Of the 89 STAY cells, 20 turned on: q7/q8/q9 (#191 closed) and q66 (#55 stale). The
   STALE clause holds as conditioned: all 41 turned on because tasks ran them — 16 in stale-cells,
   25 here.
4. **False.** The device column is 514 on of 600 (tpch 171, tpcds 343), above the 290–400 range.
   Of the 431 FLIP cells, 427 turned on, not about 306. Its shortfall clause is true in letter:
   the four FLIP cells off are on q11 and q22, which it names — but for #199, which it does not.
5. **True.** No cell on at `fc3b0b55`, on either engine, is off now. R1 to R8 did not turn a cell off.
6. **False in its first clause, true in the other three.** tpch q2 and q15 gained #137 filters at
   the three tp4 modes (one and two filter lines): they join on an aggregate's output, which
   `can_be_null` counts as possibly NULL. Plan task 3 recorded it. q16's and anti-join's plan trees
   are unchanged at all five modes. tpcds q9's 15 `GpuCrossJoin` lines are 15 `GpuNestedLoopJoin`
   over `true` at every mode. nested-limits' region scan declares `schema=[__rowmarker__:Int8]`.
7. **True.** `git diff fc3b0b55` over tpch's and tpcds's `mini.result.txt` removes no value line:
   only `mode=` stamps change and `skipped:` stubs become answers. No cell duckdb-oracle enabled is
   off (statement 5's check covers it).
8. **Not decidable yet.** verify-26.02 has not run.

**Against the totals.** gpu: 431 FLIP predicted categorically for tpch and tpcds, about 306
weighted; 472 turned on by this task (427 FLIP, 20 STAY, 25 STALE). cpu: 23 predicted for this
task in tpch and tpcds, 23 measured; 48 with pbench's five `190` rows, as §2's note says, and
measured 48.

**The record.** The eighteen tickets moved to `archive/archived-tickets.md`, Done 2026-10-14, each
saying what fixed it, in the order the spec closes them: #155, #152, #153, #80, #59, #215, #208,
#207, #190, #63, #212, #173, #136, #137, #159, #160, #220 from `tickets/joins.md`, and #154 from
`tickets/corpus-coverage.md`. `joins.md` keeps five tickets (#243, #245, #246, #250, #256) under a
new header; its rewrite-era preamble said the work was still to come. `corpus-coverage.md` loses
its Performance section, #154 its only ticket. `tickets.md`: 124 → 106 open, joins 22 → 5,
corpus-coverage 33 → 32, each row's count, list and anchors checked equal by script. The old
"#173 (done half)" entry now points at the new #173. No ticket filed: every cell that met an issue
met an open ticket, and #267 was filed in plan task 9. Next free is still **268**.

**Links moved to the archive**, every one to the eighteen across `llm-wiki/` and the code:
`archive/archived-tickets.md` (5, now in-page), `tasks/join-backend.md` (15), `join-session-cpp.md`
(5), `tasks.md` (2, the board line), `exit-copies.md`, `join-rewrite-design.md`,
`reports/join-rewrite-cell-estimate.md`, `tickets/complete-coverage.md` (three in-page anchors to
#59, #80 and #137 that never resolved there), `df-upgrade.md`, `optimizer.md`, and
`scripts/exec_model/README.md`. The frozen specs got link targets only. `architecture.md` and
`build-test.md` name #136, #137 and #154 without a link, so task 15's commit stands on its own.
`git grep -nE 'joins\.md#t(155|152|153|80|59|215|208|207|190|63|212|173|136|137|159|160|220)\b|corpus-coverage\.md#t154\b|\]\(#t(…)\)'`
over `llm-wiki scripts peacockdb-core cpp` is empty outside the archive.

### Round 2 closed, 2026-10-14

Plan tasks 7, 8, 4, 9, 9b, 9c, 10, 11–13, 14 (tpch, tpcds, pbench), 15 and 16 committed one or two
per developer dispatch, each green on rust-only and on nebius-gpu. PR #179 opened against
`ENS-join-session-cpp`, 24 commits. Owed before `completeness approved`: the whole-task cost diff
against the branch point `3d9a122e` (spec risk 1), for the PR body; the coordinator cannot run it.

## Review round 1 — 2026-10-14, on `7fa0013c`: 0 blocking, 7 important, 12 nits

Checked clean by the reviewer: the 56 pins (none deleted), the pbench shape guard (45 queries), the
registry ↔ `.inc` ↔ `gpu-result.txt` agreement, the `NOT IN` rewrite worked by hand, #137's sides
for all nine types, the archived tickets' links. Q1 (task 13's deleted cuDF-call guard): not a
coverage loss — the model it compared against is gone and the 53 session gtests cover the calls;
record it as a departure in the signoff. Q2 (25 dead-code warnings): none looks like this branch's
by reading; only a build settles it (I7).

Important, routed to the developer unless marked *coordinator*:
1. `test_support/order_hazard.rs:170-185` `tie_at_a_cut` compares only the two rows at the cut; a
   three-row tie (a=(1,p),(2,q),(3,q),(4,s), b=(1,10),(2,10),(3,10),(4,30), `ORDER BY b.w LIMIT 2`)
   passes when the cpu orders the two (q,10) at the cut. Read the whole tie group; also an offset
   cut is read only when the `GlobalLimitExec` is the sort's direct parent. Add the case.
2. `cpp/src/operators/scan.cpp:33` `rows_of` caps per call at `limit`, so a rows-only scan with a
   limit beside row groups answers batches × min(rows, limit) where the base refused (#186). Refuse
   it as `read_parquet` does; pin it under #186. Needs a device run.
3. `tests/gpu_tests/join_schema_cases.rs` has no Left/Full cases (header still says they refuse,
   #152), nor nested-loop types past Inner/Left in `nested_schema_cases.rs`. Add them; fix header.
4. `build-test.md:702-708` device-schema row: Left/Full "refuse"; "Five" `bug_` pins → four.
5. *coordinator* — #214 (`corpus-coverage.md:427,439-444`) describes `without_build` and #212's
   refusal, which this branch removed; reword to what is still broken or close.
6. #267 has no `bug_` test (planner, `// #267`); its line 345 says `count(*)` where the rewrite
   writes `count(lit(1i32))` (*coordinator* for the text).
7. The spec's no-dead-code-warning bar: list the 25 `--features gpu` warnings at `3d9a122e` and at
   HEAD; if equal, the signoff reads the bar as "no new warning".

Nits: (1) comments over 10 lines — `gpu_backend/mod.rs:1-11`, `planner/tests/join_refusals.rs:1-11`,
`wire/mod.rs:338-348`, `tests/common/corpus_cases.inc:265-276`; (2) *coordinator* — tickets the diff
lengthened: #257 29→43 and its line 175 stale on `EmptyExec`, #205, #199, #225, #56, #60; (3)
*coordinator* — `corpus-coverage.md:466` #205 reads as if `not-in-uncorrelated`'s device cells are off;
(4) *coordinator* — `coding-style.md:165` "30 of the 46" now 47; (5) `wire/tests.rs:458` compares
`needs_finish` with itself; (6) stale comments `driver/tests/flow.rs:388`, `tests/test_cpu_corpus.rs:143`;
(7) `plan/join.rs:406-411`, `plan/mod.rs:1314-1319` docs still say "produced no batch"; (8)
*coordinator* — `architecture.md:881` `GpuEmpty` "routed as a merge"; (9) `wire/writer.rs:42`
`Writer::new` test-only; (10) `memory_estimation.rs:248` a finishing nested loop priced without its
matched byte; (11) *coordinator* — PR body over 10 lines; (12) signoff: name the retired guard and
`pub trait IntoAnyBox`.

Also owed before the PR settles: the whole-task cost diff against `3d9a122e` (spec risk 1).

### Round 3 — review round 1's findings

Dispatched 2026-10-14 on `77a18d46`. Every item routed to the developer, in the order given; the
*coordinator* items (I5, I6's ticket text, nits 2, 3, 8, 11) were already done, and nit 12 is the
signoff's. Every mutation below was reverted, and the file was confirmed back to its pre-mutation copy
(`cmp`) before anything else ran.

**I1 — the order hazard reads the whole tie (`test_support/order_hazard.rs`).** A cut now finds
its tie group by widening the sort's fetch, doubling it until a row past the tie, or the input's end,
is in the prefix. It reports a hazard when the tie spans the cut and holds two rows that differ
(`arrow::row` comparisons, not one `ScalarValue` per cell). The offset is carried down through
`ProjectionExec` and `CoalesceBatchesExec`, which keep row positions. A sort with no fetch becomes a
cut when an offset sits above it: `ORDER BY … OFFSET n` had no cut at all before. Four cases; the
reviewer's arrangement could not be made red as given:
- DataFusion's top-k is not stable. For `v = (p, q, q, s)` it emits `q, q, p` at fetch 3, so the
  old two-row check already saw `p` at the cut. `v = (q, p, q, s)` emits `p, q, q`, and the two
  `(q, 10)` meet at the cut: `a_sorted_cut_inside_a_tie_of_three_rows_is_a_hazard`.
- `an_offset_through_a_projection_inside_a_tie_is_a_hazard`: `SELECT a.v … ORDER BY b.w OFFSET 2`
  plans `GlobalLimitExec → ProjectionExec → SortExec` (no fetch).
- `a_tie_that_runs_past_the_row_after_the_cut_is_read_to_its_end`: four tied rows, and the top
  three DataFusion keeps are the same, so the differing row lies past the first prefix.
- `a_sorted_cut_inside_a_tie_of_identical_rows_is_not_a_hazard`, against over-refusal.
- Red before: the first two failed on `77a18d46`'s code (a hazard of `None`).
- Mutations: widening never done (`.or(Some(n))`) failed the run-past case; the old two-row check
  together with no widening failed it and the three-row case; `keeps_positions` set to `false`
  failed the offset case. One mutation said nothing: the two-row check *with* widening stays green,
  because the 6-row top-k reorders the tie. So the three-row case pins the old code, not the
  backward scan alone.
- Green: 15 order-hazard cases. All 157 `device_order_` corpus cases still pass, so no enabled
  device cell is newly refused.
- Not handled, and said so: an offset over a node that changes positions (a `FilterExec` between
  the limit and the sort) is not read as a cut. No corpus query has that shape.

**I2 — a rows-only scan refuses a limit beside row groups (`cpp/src/operators/scan.cpp`).**
`execute_scan` now builds `parquet_reader_options` first. It sets `set_num_rows(limit)` and
`set_row_groups` on them before branching, and the column read adds `set_columns` later. cuDF
refuses the pair in `parquet_reader_options::set_row_groups` itself, so the rows-only path meets
cuDF's own refusal, with no copied message. Two pins in `gpu_tests/source_cases.rs`, under #186:
`bug_a_rows_only_scan_with_row_groups_and_a_limit_is_refused_on_the_device` and
`…_is_read_whole_on_the_cpu` (the cpu half: four batches of 16, the limit unread). The rows-only
construction became a helper the existing case shares.
- Red on device build 1, at the old `scan.cpp`: the device pin failed, because the device answered.
  Green on build 2: 706 passed in `gpu_tests::`, both pins among them.
- **Does `SELECT 1 FROM lineitem LIMIT 10` at tp1-rowgroup reach it? Yes.** A remote-only scratch
  test (`whole_plan::answers_on_both` over a remote-only `zz-rows-limit.sql`) gave, on the old code:

  | mode | cpu (rows, batches) | device |
  |---|---|---|
  | tp1-rowgroup | 6,001,215, 49 | 490 rows, 49 batches: ten per row group, a wrong answer |
  | tp1-single | 6,001,215, 1 | 10, 1 — one call over all 49 groups, right by accident |
  | tp4 ×3 | 10, 1 | 10, 1 |

  All five modes plan `GpuLoadParquet: projections=[], limit=10, schema=[__rowmarker__:Int8]` with
  no limit node above it at tp1. The cpu column is #186's cpu half. After the fix (build 3, the
same remote-only probe), the device refuses at all five modes, at the first call, with cuDF's
`row_groups can't be set along with skip_rows and num_rows`. That is #186's refusal for `SELECT *
FROM lineitem LIMIT 10`. tp4, right by the unload's interval before, now refuses too, and so does
tp1-single. No enabled device cell has this shape: `test_gpu_corpus` passed whole on the fix.
  `architecture.md`'s `CudfScan` row and #186's text say what the code now does.
- `peacock_cpu_tests` 15 passed locally (`/tmp/dkb-cppbuild` rebuilt clean). `git clang-format
  --diff HEAD` over `scan.cpp`: no change.

**I3 — the device-schema cases for Left, Full and the newly runnable nested loops.**
`join_schema_cases.rs` gains six cases: Left and Full, plain and with a crossing projection over two
probe batches, and each over `probes_with_misses`, whose finish pads build rows no probe matched.
`nested_schema_cases.rs` gains fourteen: Right, Full, LeftSemi, LeftAnti, LeftMark, RightSemi and
RightAnti, each plain and with `crossing_projection`, over two probe batches so a build-side answer
at finish is read. `nested_cases::nested` became `pub(crate)` for them. Both headers were rewritten:
the #152 refusal is gone, and so are the recipe words "side swap" and "accumulated-keys finish".
All twenty were green on their first device run (build 1) and again on build 2. The device holds
what these nodes declare, so no `bug_` pin. These are coverage, not a fix, so there was no red
phase. The read-at-the-handle machinery already fails elsewhere: the four schema `bug_` pins
assert divergences it reports.

**I4 — `build-test.md`.** The device-schema row now names all nine hash-join types, Left and Full
through the padding finish, and every nested-loop type. It says "Four are `bug_` pins": #65,
#216, and #225 at the init and at the merge. #207's cross-join pin is gone. The counts come off
`--list`:
- rust-only `--lib` 769 (763 + order hazard 4, `not_in` 1, memory estimation 1).
- device `gpu_tests::` 706 (684 + 22): the schema files 169 (149 + 20), and the rest of
  `tests::gpu_tests` 457 (455 + 2).
- The headers were derived by summing every row's N with a script that reproduces `77a18d46`'s
  4903 / 4333 / 173 / 397 exactly. Today: **4931 — Rust 4361, C++ 173, Python 397**; cpu 2758,
  gpu 1483.
- The three cpu rows' texts name what they now cover (#267's pin, the whole-tie read, the nested
  loop's `matched` byte).

**I6 — #267's pin.** `planner/tests/not_in.rs`
`bug_the_rewrites_count_over_a_scan_split_into_lanes_is_refused_at_tp4`, `// #267` above it. It plans
`pbench/not-in-uncorrelated` at each tp4 mode with pbench's knobs and asserts validation's "a
finalizing merge over several lanes needs its input hashed". Mutation: pointed at the tp1 modes, it
fails with "tp1-single planned: #267 may be fixed".

**I7 — the 26 `--features gpu` dead-code warnings.** Each revision was built locally from a `git
archive`, in its own directory with its own target dir (`/tmp/jb-{base,head}`, with
`third_party/cudf` a symlink to the workspace's checkout). Both target dirs were deleted afterwards,
and the workspace's own `target-cudf-rapids-cuda-12.2` was never pointed at another tree.
`CUDF_ROOT=…/rapids-cuda-12.2 scripts/cargo-cudf.sh build -p peacockdb-core --features gpu`:
**26 at `3d9a122e`, 26 at `77a18d46`, 26 on this round's tree — the same (file, item) pairs all
three times**, compared by script; only line numbers moved. The review's 25 was a grep for
`never used|never constructed`, which misses `field per_call is never read`. None is the branch's.
The branch changed `driver/index.rs` and `executor/mod.rs` around these items and moved them, nothing
more. So the signoff reads the bar as "no new warning". The pairs:
- `executor/mod.rs`: `recorded` (method), `per_call` (field), `call` (method), `join_regions`,
  `node_measured`, `nodes_as_recorded`, `RmmPool` (enum), `NodeTiming` (enum), `NvtxRange`
  (struct), `install_rmm_pool`, `set_node_timing`, `set_nvtx_ranges`, `nvtx_range`,
  `collect_regions`;
- `executor/instrument.rs`: `install_rmm_pool`, `set_node_timing`, `set_nvtx_ranges`,
  `nvtx_range`, `nvtx_pop`;
- `executor/driver/mod.rs`: `nodes_as_recorded`, `join_regions`, `node_measured`;
- `executor/driver/measurements.rs`: `join_regions`, `unanswered`, `node_measured`;
- `executor/driver/index.rs`: `nodes_as_recorded`.

The gpu lib-test build has only the pre-existing `AsArray` warning. The rust-only builds have none.

**Nits.**
1. The four comments are at most ten lines now. `gpu_backend/mod.rs` is 8 lines and
   `join_refusals.rs` 10, both merged rather than cut. `wire/mod.rs`'s `attach_recipes` doc is 10.
   `corpus_cases.inc`'s pbench block is 10, keeping `BOTH COLUMNS.`.
5. `wire/tests.rs`'s agreement could not fail. The recipe's `AtDone` and the cpu's
   `finish_and_fetch` returning `Some` both read `needs_finish`. Shown: `needs_finish(Inner) = true`
   left the test green. Now `has_finish_pass` (both rows of `TEST_ONLY_ITEMS`, signature only) takes
   real batches: `dim` = (1, 0), (2, 0) and one `fact` batch, (1, 10). It reports whether DataFusion
   yields rows at the finish, which no type that does not finish can do without `finish_and_fetch`
   failing. Mutations: `needs_finish(Inner) = true` → red ("Inner … disagree about the finish");
   `needs_finish(Left) = false` → red ("the finish of a Left join answers nothing, and DataFusion
   emitted 1 rows there").
6. `driver/tests/flow.rs`: the scatter test now says what it holds, that a lane with no probe rows
   makes no probe call; the "device refuses a second probe call" premise is gone.
   `test_cpu_corpus.rs`: the "#152 clears" sentence became "device cells are enabled in bulk".
7. `plan/join.rs` and `plan/mod.rs`'s `empty_build_owes_nothing` doc now says "a build side that holds
   no row — no batch, or batches of none". Both engines call it with `rows == 0`. The doc also names
   the driver's dropping of probe batches.
9. `Writer::new` deleted. Its eight callers, all in `wire/tests.rs`, call a two-line `writer()`
   helper there.
10. `memory_estimation.rs`: a `GpuNestedLoopJoin` is priced at its build bytes plus `matched_bytes`,
    one byte a build row for a type that finishes, as `gpu_backend/join.rs`'s `built_bytes` charges
    it. A cross join is Inner alone and stays at its build bytes. The red test,
    `a_build_preserving_nested_loop_is_charged_for_its_matched_column` (supplier LEFT vs INNER on
    `<`), failed at 280000 vs 280000 and expects the 10,000. The plan goldens moved and were
    regenerated (`UPDATE_CANONICAL=1 … plan_goldens`): 15 files, memory lines only
    (`estimated_max_resident_size` and the `budget=` line, checked by script). Nine queries move at
    every mode: tpch `nested-loop-left-join`, tpcds `q9`, and pbench `nl-full`, `nl-left`,
    `nl-left-anti`, `nl-left-decimal`, `nl-left-semi`, `nl-mark` and `scalar-subquery-cross`. No
    cpu, cost or payload golden moved. `architecture.md`'s Pricing paragraph says a keyed join's
    table and either kind's `matched`.
- `coding-style.md:164`, the split derived from `SURFACE` (47 items). The CLI names nine: `run`,
  `CpuBackend`, `record_batch` (main.rs calls `batch.record_batch()`), `plan`, `PlanKnobs`,
  `BatchSizing`, `SMALL_TABLE_BYTES`, `build_session_state`, `register_tables_for`. `run<B:
  Backend>` closes over the other 22 `executor/mod.rs` items and the eight `Cpu*`, 30. `plan`
  closes over `plan/mod.rs`'s seven, now with `IntoAnyBox`, plus `MemoryModel`: eight. 30 + 8 + 9 =
  47. The receipt line now also names `private_bounds`, which is what `IntoAnyBox`'s was.

**Spec risk 1 — the whole-task cost figure.** `cargo run -q -p cost-report -- --cost-diff --base
3d9a122e` on the final tree: **703 compared, 529 changed — 278 improvements, 251 regressions, 173
past the 10% gate.** It is red, and pre-accepted by the board.
- By dataset: tpch 106 improvements (25 queries), 11 regressions (3 queries), 0 past 10%. tpcds 156
  (71), 209 (70), 161 past 10%. pbench 16 (8), 31 (10), 12 past 10% (`exists-null-keys`,
  `not-not-in`, `not-or-not-in`, `uint-key-join` at the tp4 modes).
- tpch in full: q2 +0.72% at the three tp4 modes (374.74 → 377.45 MB); q15 +1.59 / +1.43 / +1.62%
  (56.60 → 57.50 MB); `nested-limits` +0.00% at every mode (976.44 → 976.45 KB, the marker's bytes).
- Worst five, by query: tpcds q33 +30.74% (605.22 → 791.24 MB), q56 +30.58% (606.42 → 791.85 MB),
  q61 +30.53% (1.74 → 2.27 GB), q31 +30.37% (1.04 → 1.36 GB), q60 +30.29% (618.50 → 805.86 MB),
  each at tp4-rowgroup. The spread over the 251: 14 at +0.00%, 39 under +5%, 25 to +10%, 47 to
  +20%, 112 to +30%, 14 above.
- Best five: tpcds q22 −17.54% (tp1-single), tpch q21 −16.78% (tp1), tpcds q16 −14.77%, q94
  −14.60%, tpch q13 −11.75%.
- Where they come from, per the rounds above, which overlap on some rows and so do not add up
  to this figure: the regressions are plan task 3's #137 filters at the tp4 modes (173 past 10% there,
  as here), plan task 4's `__rowmarker__` (33, none past 10%) and plan task 8's +46 bytes on
  `nested-loop-left-join`. The improvements are plan task 6's 445 and plan task 9's lost
  coalesces (q21, q16, q94, `nested-loop-left-join`).
- The per-row table is `cost_diff.html`, which the command writes.

**Device run, nebius-gpu, build 2** (`./scripts/build-test-shadgpu.sh --build` rc 0, the tree synced
uncommitted, each binary run directly with `--test-threads=1`): **1616 cases, every binary rc 0,
0 failed** — `peacock_gpu_tests` 2, `peacock_plan_tests` 81, `peacock_join_session_tests` 53,
`peacockdb_core_gpu_lib gpu_tests::` 706, `test_node_timing` 1, `peacock_gpu_benchmarks --skip
bench_` 8, `test_gpu_corpus` 765. Not run, per the board: `peacock_cpu_tests` (local, 15 passed),
the sf40 pair, the `bench_` cases. Build 2 is this round's code. After it, `source_cases.rs` was
rustfmt-ed whole (clean at HEAD) and `matched_bytes` moved above `mod tests;`. Build 3 carries
both, plus the probe, and reran the three touched device files: `source_cases` 12,
`join_schema_cases` 44 and `nested_schema_cases` 20, all passed. The final sync took the probe out
of the remote tree.

**Hosts.** nebius-gpu had 27 GB free before and after; nothing was deleted there. Locally, the two
archive trees and their target dirs (the base's was 4.6 GB) were removed after their counts.

## Review round 2 — 2026-10-14, on `4094f10b`: 0 blocking, 0 important, 4 nits

17 of round 1's 19 closed; nit 12 is the signoff's. I1's widened tie and nit 5's rewritten
`has_finish_pass` were each shown able to go red. I2's refusal judged right: neither engine's scan
limit is designed (#186), and one refusal rule is what #186's fix will change. Round 1's nit 2
(tickets over length) left partly open: #205, #199, #225, #56, #60 stay long; not reopened, since a
nit does not reopen a task. The four new nits fixed by the coordinator in `a356266e`.
Board → `completing`.

## Completeness — analyst, 2026-10-14

On `63d0e10b` against `3d9a122e`. I read the spec first: its Scope, then work items 1–10, risks
1–6, the Restriction, the Registry and the Verification bar. Then design §1.3, §3.5, §4 and §5.7,
the whole of `architecture.md` against the code, `build-test.md`, and this file's rounds (not its
review sections). I read PR #179's body and its CI runs with `gh`. No project code was built or run.

**0 blocking, 2 important.**

**1. important: the cost acceptance names 5 of 251 regressions, and the PR body has neither the
list nor the removal greps.** Spec risk 1 says "list every rise in the detail file". Chain J's
board header accepts join-backend's regressions on one condition: "Name each regression in the
task's detail file and its PR, with the queries and the byte delta, so the acceptance has a
record." Round 3's whole-task figure gives counts by dataset, tpch's three queries in full, the
worst five, the best five and a spread. Then it says "The per-row table is `cost_diff.html`, which
the command writes", and nobody committed that file. About 240 tpcds and pbench rows are named
nowhere. Plan task 3's figure has the same gap. PR #179's body names no regression. It also does
not quote the five removal greps that spec item 9 says "the PR quotes". They are in plan task 15,
under "quoted for the PR". Fix, with no code change: append the per-row table (query, mode,
base → head bytes, %) under the whole-task figure. Then put that table, or a link to it, and the
five greps in the PR body.

**2. important: work item 3's "pricing as §4.3 says" landed in part, and the rest is not recorded
as dropped.** This branch edited §4.3 and left it pricing three things the code does not price:
- the distinct-key table a RightSemi/RightAnti session holds ("plus that table");
- a build-side semi probe's `distinct_keys`;
- "for RightSemi/RightAnti with a cross residual the build's hash that `mixed_*` makes per call".

`GpuJoin::built_bytes` and `GpuProbingJoin::call_bytes` (`gpu_backend/join.rs`) price none of the
three. Plan task 7's pricing record states the formula that landed and does not name the
omission. The exposure today is small:
- A session that calls `mixed_left_semi_join` holds no hash table. It is still charged 16 B a
  build row as resident, and that covers the per-call rehash.
- A build-side semi probe is charged an output it never emits, and that covers its distinct keys.
- What stays unpriced is a residual-free RightSemi/RightAnti session's `Bd`, at most the build's
  key bytes.
- Device corpus runs pass no budget, so nothing tripped.

Fix: price the three terms. Or say here, and in `architecture.md`'s Pricing paragraph, which §4.3
terms are not priced and why.

**Checked, nothing missing:**
- Work items 1–10 are each present in code and tests. The cleanup greps are empty on the final
  tree; I re-ran them over the code, `scripts` and `flatbuffers`. Item 9's "`join-rewrite-design.md`
  archived" waits for the helper's merge commit, as plan task 15 records.
- Risks 2–6:
  - Goldens were regenerated in the commit that moved them (`94778b76`, `4ef0e714`, `d7042a8c`,
    `a9ae581c`).
  - `gpu-result.txt` was regenerated for all three datasets.
  - `order_hazard.rs` exists, with one `device_order_*` case for each line that has a device cell.
    The cross join's emitted order is pinned with `Order::AsEmitted`.
  - Both backends read `plan::empty_build_owes_nothing`.
  - The determinism sentence is reworded.
- The tests §5.7 names exist:
  - all nine types under both null rules (Left and Full are in `join_shape_cases.rs`);
  - lanes with no build batch and with no probe batch, for every type;
  - every `key_types` column as an Inner key and a LeftSemi key;
  - the #153, #159, #160, #215 and #63 cases;
  - the `IS NOT DISTINCT FROM` arms on both evaluators;
  - the `__rowmarker__` and `GpuEmpty` planner and validation tests;
  - the two timestamp wire cases;
  - the gtests on the session;
  - the driver's `set_build(None)` tests.

  The exec_model cuDF-call guard was dropped rather than moved to the new file. Plan task 13
  records why.
- Registry, as plan task 16 records:
  - 23 tpch/tpcds cpu cells, and 427 of 431 FLIP device cells.
  - rollup-over-join (#65), q78 (#60) and q32 at tp4 (#199) are off, with their tickets on their
    rows.
  - The 25 stale-#183 cells are on.
  - `empty-side-left-join` and `indf-full-join` are on at every mode on both engines.
- These files are outside the Scope table, and this file accounts for each:
  `testdata/cost_model.conf` (`GpuEmpty`), `test_support/*` (join scratch, order hazard),
  `tests/test_cpu_corpus.rs`, `.github/workflows/exec-model-corpus.yml`, `coding-style.md` (the
  SURFACE count), `planner/parquet_nulls.rs`, `scan_mapping/parquet_meta.rs` and
  `test_module_layout/visibility.rs`.
- Verification bar: the device ran on nebius-gpu under the board's override, which also deferred
  sf40 and the benchmarks. CI on `63d0e10b` is the first run to build `a356266e`'s `flow.rs` edit.
  Both pipelines and the GPU build are green. cost-report is red (pre-accepted) and `gpu-tests` is
  red (exempt).

**`architecture.md`: six sentences this branch falsified.**
1. Joins, opening paragraph: "The session keeps the build table, the join object and the
   build-matched column between calls, so nothing is copied and nothing is rebuilt per batch."
   - Take a RightSemi or RightAnti whose cross residual the AST takes whole. Its session builds
     no join object (`join_session.cpp:149-153`, `cross_is_ast`).
   - Each of its probes runs `mixed_left_semi_join(Pk, Bk, …)`, which hashes its right table
     every call (`third_party/cudf/cpp/src/join/mixed_join_semi.cu:114-115`). That right table
     is the build.
   - Keyless sessions hold no join object either.
2. The wire format → From flat buffer to cuDF call, the `CudfJoin` row: "the tables arrive through
   `peacock_join_build` and `_probe`, which hold one `cudf::hash_join` or `distinct_hash_join`
   across every batch." The cause is the same: keyless sessions, and the semi sessions in item 1,
   hold neither object.
3. The wire format → From node to seqs, the join row: "`join_build(seq, build)` once per lane,
   `join_probe` per probe batch, and `join_finish` at done for the five finishing types".
   - Every join lane that was not drained takes `LaneCall::Finish` (`single_partition.rs:215`).
   - `GpuProbingJoin::finish_and_fetch` calls `peacock_join_finish` whatever the type. So an
     Inner, Right, RightSemi or RightAnti lane makes a finish call. The call answers handle 0 and
     is timed on the seq.
   - The plan goldens' recipe line leaves that call out for those four types: 62 tpch join lines
     at tp4-single end at `join_probe`. `GpuJoin::new` refuses a recipe that names it. So the
     executor makes a call its recipe does not publish.

   The sentence and the recipe line, or the executor, should change.
4. Joins → What each type answers, **Pricing**: "The planner writes `chunk_bytes` from the mode's
   budget, a sixteenth of it, so the accountant and the session bound one call alike."
   `join_scratch_bytes` in `wire/mod.rs` writes `min(budget / 16, 1 GiB)` at `tp4-sized` only. The
   other four modes write 0, which the session reads as 1 GiB.
5. Interfaces, "Instrumentation is process-global and off by default…": "…`peacock_gpu_benchmarks`,
   `test_node_timing`, one `gpu_tests` case in the GPU backend, and the gtest fixtures on both C++
   tiers." There are two cases now:
   `a_slice_and_an_export_are_charged_to_the_node_that_produced_the_handle`
   (`gpu_tests/accumulate.rs`), and this branch's
   `every_session_call_is_journalled_and_timed_on_the_joins_seq` (`gpu_tests/join.rs`).
6. Joins → Cross join vs nested-loop join: "Both are a join with no equality to hash, and which one
   DataFusion plans — and so which node the translator meets — is decided by whether there is a
   predicate at all." and "**No join predicate ⇒ `CrossJoinExec`.**"
   - The join type decides too. A predicate-free non-Inner `NestedLoopJoinExec` now plans as
     `GpuNestedLoopJoin` over `true`: tpcds q9's 15 Left joins, at every mode. Only an Inner one
     plans as `GpuCrossJoin`.
   - This branch rewrote the Joins opening to say so, and this section now contradicts it.
   - DataFusion's half was already loose at the base: q9 reached the translator as a
     predicate-free nested loop.

Already wrong at the base, not this branch's doing: The scheduling rule, "A lane that receives
nothing is never runnable". A join lane whose inputs end empty is run. On this branch the run is
`set_build(None)`; at the base it was `NoBuild`.
