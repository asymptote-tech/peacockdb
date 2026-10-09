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
