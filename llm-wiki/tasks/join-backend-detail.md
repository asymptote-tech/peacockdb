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

The board accepts in advance any cost regression this task brings. The spec's risk 1 still
applies: run `cargo run -q -p cost-report -- --cost-diff --base <base sha>` before the PR and
list every rise here with its queries and byte delta, so the acceptance has a record.

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
