# distinct-companions — run detail

What a restarted coordinator needs. Spec: [`distinct-companions.md`](distinct-companions.md);
plan: [`distinct-companions-impl.md`](distinct-companions-impl.md).

## Standing facts

- Chain K, second task. Branch `ENS-distinct-companions`, forked off `ENS-guard-checks` at
  `6175d5fb`. Its PR targets `ENS-guard-checks`, not master — guard-checks is `done` and awaiting
  the human's merge, so the parent branch exists on the remote already.
- No GPU on this chain. No device build beyond compiling, no device run. `done` once every CI job
  but the GPU tests is green.
- Verification bar: rust-only `--lib`, `test_cpu_corpus`, `test_corpus_goldens`, `test_cost_model`;
  the ffi rung through `scripts/cargo-cudf.sh`; C++ `cpp/build` against cuDF 25.02 plus
  `ctest -L cpu`. No device run.

## Both of the spec's conditionals resolve to "has not merged"

The spec defers two things to whichever chain lands second. Checked on this branch at
`6175d5fb`:

- **chain J's duckdb-oracle has not merged.** `corpus_query!` (`tests/test_cpu_corpus.rs:22`)
  still takes `(dataset, sf, query, cpu_modes, gpu_modes, cpu_oracle, gpu_oracle,
  schema_validation)` and nothing under `src/test_support/` mentions duckdb. So the three corpus
  lines carry no `duckdb-result.txt` section and no `duckdb_none`, and `distinct-functions`' cpu
  oracle is the new `data_fusion_disabled` with no DuckDB oracle beside it.
- **repartition-keys has not merged.** No `drop_grouping_id` anywhere in `peacockdb-core/src`. So
  `rollup-distinct`'s tp4 cpu cells stay off on #189 and `189` stays in its registry row.

`data_fusion_disabled` does not exist yet; this task adds it. #261 and #262 already have ticket
bodies (`complete-coverage.md`, `corpus-coverage.md`), filed with the spec.

## Dispatch log

### 2026-10-08 — round 1, developer

verda unreachable (`Temporary failure in name resolution`), so everything runs locally. Board
moved `approved to build` → `building` in the same commit as this file.

#### Progress

Checkboxes are ticked in `distinct-companions-impl.md` as each plan task closes. The plan's
`git add`/`git commit` steps are **not** run: this dispatch forbids mutating git state, so every
task's work stays in the working tree and the human commits. Everything else in each task is run.

- Task 1 baseline: `cargo test --features rust-only -p peacockdb-core --lib -- planner::` →
  127 passed, 0 failed, 477 filtered out.
- **Task 1 done** (the `Stage`/`sequence` split). `--lib -- planner::` 127 passed / 0 failed;
  `test_cpu_corpus` **555 passed / 0 failed**; `git status --short testdata/` empty. The refactor
  moved no golden byte.
- **Task 2 done** (classify; #144, #261 and the net refused by name). RED was watched: with the
  route removed and `decompose`'s wording restored, all three new tests fail with
  `unsupported: DISTINCT inside count(DISTINCT …) (#62)`. With the implementation back,
  `--lib -- planner::` 130 passed / 0 failed, and q28's five golden sections still read the #62
  refusal.
  - **Drift from the plan, deliberate:** `distinct.rs`'s items are `pub(crate)`, not
    `pub(super)`. `coding-style.md`'s Visibility section says an implementation module uses
    `pub(crate)` items and "`pub(super)` is never needed", and `grep -rn 'pub(super)'
    peacockdb-core/src` finds none in the tree. Every ancestor module is private, so the items
    are unreachable from outside `translator` either way.
- **Task 3 done** (the two-stage lowering, grouping sets still refused). RED watched first: all
  nine new tests (5 plan + 4 end-to-end) failed with the temporary `#62` refusal, the empty
  keyless case among them. After the lowering: `--lib` **613 passed / 0 failed / 2 ignored**
  (the two `#[ignore]`d on #182), `test_cpu_corpus` **555 passed / 0 failed**.
  - Goldens: exactly the five `tpcds.sf1/*.plans.txt`, **one hunk each**, **one removed line
    each** — the `(#62)` refusal. Checked with `git diff -U0 <f> | grep -c '^@@'` and
    `git diff <f> | grep '^-[^-]'` per file. Each hunk sits between `== q28` and `== q29`.
  - `testdata/cost-registry.csv` row 29: the five plan cells `disabled` → `enabled`; its cpu
    cells stay `na` until Task 6.
  - The #62 pin `a_distinct_beside_a_companion_datafusion_cannot_rewrite_is_refused_naming_62`
    is gone from `join_refusals.rs`.
  - `decompose`'s loop now branches on `InitFrom` before anything else, and the net
    (`is_distinct()`) fires under `InitFrom::Values` alone. The `Deduplicated` arm skips
    `state_fields()` entirely rather than only its arity check: a DISTINCT's declared state is
    DataFusion's list of values, so neither its arity nor its nullability says anything about
    the non-distinct twin the init runs, and every column it declares is nullable.
- **Task 4 done** (grouping sets under a DISTINCT). RED watched: both tests failed with
  `Unsupported("a DISTINCT under grouping sets (#62)")`. After: `--lib -- distinct rollup
  translator::` 79 passed / 0 failed; `--lib -- planner::` 136 passed / 0 failed;
  `git diff --stat testdata/goldens/` unchanged from Task 3 (the same five q28 hunks and
  nothing else), so grouping sets moved no golden.
  - `Aggregate::grouping_id_type` confirmed in DataFusion 45: `<=8` → UInt8, `<=16` → UInt16.
    So 8 ROLLUP keys plus the argument is 9 → the inner id is UInt16 against DataFusion's
    UInt8, and the project above the outer stage narrows it. Two keys plus the argument is 3,
    still UInt8, and the first test's last assertion is the negative control for that.
- **Task 5 done** (the wire's `distinct` field deprecated). `distinct: bool (deprecated);` in
  `gpu_plan.fbs` with the slot kept; the writer's `distinct: false,` gone; the C++ guard block
  and the three `/*distinct=*/false` arguments gone. `clang-format -i --lines=` over the three
  changed call sites only (not whole files).
  - **`recipe-payloads.txt` did not move, proven two ways.** (1) Its sha256 is
    `b2f9cb2cf9650c780139ae526f19c5caa0006904972fc4d32b03303b4b3b4641` both before the change
    and after a **forced** `UPDATE_CANONICAL=1` regeneration of `planner::tests::plan_goldens`.
    (2) It never appears in `git status --short testdata/goldens/`. FlatBuffers omits a `false`
    at its default and nothing sets `force_defaults`, so the bytes are the same.
  - The generated Rust lost the accessor: `grep -c distinct` over the fresh
    `target/debug/build/peacockdb-core-*/out/gpu_plan_generated.rs` is **0**. (Older build
    dirs under `target/` are stale artefacts of earlier fingerprints and still carry it.)
  - `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12
    --build`: exit 0, 20/20 steps, `peacock_plan_tests` linked, **zero** lines matching
    `warning`. `ctest --test-dir cpp/build -L cpu`: **1/1 passed, 0 failed**.
- **Task 6 done** (the corpus). RED watched twice: the e2e oracle case failed `NotFound` before
  the query file existed, and the three corpus lines failed
  `unknown oracle keyword 'data_fusion_disabled'` before the variant existed.
  - **rollup-distinct's three tp4 cpu modes are red on #189**, verbatim its signature:
    `GpuEmitPartitions lane 0: assigning the scatter's lanes: External error: comet murmur3:
    Internal error: Unsupported data type in hasher: UInt8`. DataFusion's partial→final
    shuffle for a ROLLUP hashes the keys *and* `__grouping_id`, and `inner_shuffle` moves all
    of them one column right, so the inner shuffle does hash the id. Dropped from the line and
    `189` kept in the registry row, as the spec says. q28 and distinct-functions: all five
    green, no new ticket.
  - **The registry must be written before `.result.txt` can be.** `authoritative_mode`
    (`test_support/corpus.rs:400`) reads `cost-registry.csv`, not `corpus_cases.inc`, so with
    q28's cpu cells still `na` and the two tpch rows absent it returned `None` and the three
    result sections were silently not written — `mini.result.txt` came back unmodified. The
    plan has the registry as step 6 and the regen as step 5; the working order is registry
    first, then `UPDATE_CANONICAL=1 … plan_goldens`, then
    `PCK_UPDATE_SECTIONS=1 … test_cpu_corpus -- q28 rollup_distinct distinct_functions`.
  - `PCK_UPDATE_SECTIONS=1` (merge only) rather than `UPDATE_CANONICAL=1` (merge and prune)
    for the cpu/cost/result sections, so a filtered regen cannot prune another query's section
    (#213's hazard).
  - Registry tickets follow the plan's snippets: q28 `152`, rollup_distinct `65 189`,
    distinct_functions `262`. #262's body says q28 and rollup-distinct "carry it too once those
    close", which reads as latent rather than current, so #262 is not on their rows.
- **Task 7 done** (the docs). `PlanError::Unsupported`'s doc names #144 in #62's place.
  `architecture.md`: "DISTINCT lowers to grouping" rewritten to the lowering as built (the
  classify-before-translate rule, the widening casts, the two stages, the by-position pairing,
  the two finalize obligations, what stays refused and the net); "Grouping sets" gains the inner
  DISTINCT stage's width and the narrowing project; the wire paragraph says `(deprecated)`
  rather than "never set"; the `CudfAggregate` row drops `distinct`. The oracle sentence the
  plan wanted "where the corpus oracles are described" went into **`build-test.md`**, not
  `architecture.md` — `architecture.md` describes no corpus oracle and mentions neither
  `corpus_query!` nor `cpu_oracle`, so there was no such place there.
  Tickets: #65 one clause, #195's #144 bullet, #261's pin named. #62 untouched — the helper
  archives it at merge.
  - **`build-test.md` counts, each recounted from the code and the page re-summed
    mechanically.** `--lib` 604 → **618**, `test_cpu_corpus` 554 → **566** (+ the registry
    check = 567), cpu block 1188 → **1214**, Rust 1860 → **1886**, grand 2335 → **2361**.
    Rows: End to end 29 → 34, Planner join refusals 10 → 11, Translator 29 → 36, and a new
    module-unit row for `planner::translator::aggregate::tests` (1). Corpus prose: 116 → 119
    queries, 551 → 563 cells, rollup-distinct added to #189's list, the four cpu oracle
    keywords named with `data_fusion_disabled` and why. The ffi (7) and gpu (576) blocks and
    the 89 Rust rows of the second table are unchanged, each confirmed by running the target.

#### Final verification bar, all from the finished tree

| command | result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2` | 616 passed, 0 failed, 2 ignored (#182) |
| `… --test test_cpu_corpus -- --test-threads=2` | 567 passed, 0 failed |
| `… --test test_corpus_goldens -- --test-threads=2` | 26 passed, 0 failed |
| `… --test test_cost_model -- --test-threads=2` | 3 passed, 0 failed |
| `… --test test_ci_coverage -- --test-threads=2` | 9 passed, 0 failed |
| `… --test test_module_layout -- --test-threads=2` | 17 passed, 0 failed |
| `… --test test_golden_format -- --test-threads=2` | 26 passed, 0 failed |
| `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::` | 4 passed, 0 failed |
| `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib -- the_payload_golden` | 2 passed, 0 failed |
| `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build` | exit 0, `peacock_plan_tests` linked, 0 warnings |
| `ctest --test-dir cpp/build -L cpu` | 1/1 passed, 0 failed |
| `cargo build --features rust-only -p peacockdb-core` after `touch lib.rs` | 0 warnings |

**The cudf rung needs `LD_LIBRARY_PATH`.** `scripts/cargo-cudf.sh test … ffi_tests::` on its own
exits **127** — `error while loading shared libraries: libpeacock_gpu.so`, which `build-test.md`
documents and which is an environment requirement, not a failure of this branch. Prepend
`$PWD/target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-*/out/lib` and the cuDF root's
`lib`. Worth knowing on a restart: the bare command looks like a red test.

#### Goldens that moved, and every one audited

`for f in $(git diff --name-only testdata/); do git diff "$f" | grep -c '^-[^-]'; done`:

- the five `tpcds.sf1/*.plans.txt`: **one deletion each**, the `(#62)` refusal; the rest is
  q28's tree.
- every other file under `testdata/goldens/`: **zero deletions** — pure section insertions for
  q28, `rollup-distinct` and `distinct-functions` (`*-mini.cpu.txt`, the derived
  `*-mini.cost.txt`, `mini.result.txt`, and the five `tpch.sf1/*.plans.txt`).
- `cost-registry.csv`: one deletion (q28's row, rewritten) and three additions.
- `recipe-payloads.txt`: **absent from the diff**, sha unchanged.

The tpch cost/cpu insert sizes differ by tier on purpose: the tp1 modes gain two sections and
the tp4 modes one, because `rollup-distinct` is off at tp4 on #189.

#### No GPU, as the chain requires

No device run, no GPU cycle, `--host verda` never attempted (unreachable). The C++ was compiled
and `ctest -L cpu` run; `peacock_plan_tests` was linked and **not** executed. q28's and both new
queries' gpu cells are `disabled` in the registry and `none` in `corpus_cases.inc`.

#### One refactor after the bar, for `coding-style.md`'s 150-line rule

The plan's `lower` and the rewritten `decompose` came out at 184 and 187 lines, over
`coding-style.md`'s "under 150 lines in most cases". Each was split at its own seam, with no
behaviour change — the goldens did not move and every suite was re-run afterwards:

- `distinct.rs`: `inner_stage(t, partial, shuffle, base, base_type, inner_id_type)` holds the
  inner stage (70 lines); `lower` keeps the outer stage and the narrowing project (128).
- `aggregate.rs`: `state_and_init(aggregate, from, rule, input_schema) -> (Vec<Field>,
  Vec<AggCall>)` holds the whole `InitFrom` match — the only place the three arms differ (108
  lines); `decompose` keeps the merge, the finalize and the annotation (90).

Longest functions now: `sequence` 123 (mostly `aggregate_sequence`'s old body, moved),
`state_and_init` 108, `decompose` 90, `aggregate_sequence` 76, `inner_stage` 70, `lower` 128.
Every file under 1000 lines; `aggregate.rs` 574, `distinct.rs` 332, `translator/tests.rs` 902.
`rustfmt --edition 2024 --check` clean on all nine touched `.rs` files (named as leaves, never
the crate).

#### Nothing filed, and the one thing worth a human's eye

No new ticket: nothing in this branch is a production behaviour that is wrong. #189's three
red cells are an existing ticket's known shape, matched by signature.

**For the helper, not a ticket** (`coding-style.md`: a ticket is never about documentation):
#65 runs to 44 lines against the 30-line cap for a deferred-fix ticket. It was already over
before this task added one clause to it, and trimming it would be scope creep here — but it is
the longest ticket in the file and the next reader of it pays for that.

### 2026-10-08 — round 1 green, PR #171 open

All seven plan tasks closed. `--lib` 616 passed / 2 ignored, `test_cpu_corpus` 567,
`test_corpus_goldens` 26, `test_cost_model` 3, `test_ci_coverage` 9, `test_module_layout` 17,
`test_golden_format` 26; the ffi rung 4 plus the payload test through `scripts/cargo-cudf.sh`;
`scripts/build.sh` 20/20 with zero warnings and `ctest -L cpu` 1/1. Every test was watched red
first, including the empty-keyless case, which failed with the `(#62)` refusal before the lowering
existed.

Committed as `d5c9fd7c`, pushed, PR #171 against `ENS-guard-checks` (base verified, 2 commits).

Goldens: I audited the deletions mechanically rather than taking the report —
`git diff <file> | grep -c '^-[^-]'` over every changed path under `testdata/` is 1 for each of the
five `tpcds.sf1/*.plans.txt` (q28's refusal line) and 1 for `cost-registry.csv` (q28's row
rewritten), and 0 everywhere else. `recipe-payloads.txt` does not appear in the diff at all; the
developer also re-derived its sha256 after a forced `UPDATE_CANONICAL=1` regeneration and it did
not move. The new `translator/aggregate/` directory holds exactly `distinct.rs` and `tests.rs`.

Two things to carry into review:

- Two deliberate divergences from the plan. `distinct.rs` uses `pub(crate)` rather than the plan's
  `pub(super)`, since `coding-style.md` says `pub(super)` is never needed and the tree holds none.
  And the plan's corpus steps are in the wrong order: `authoritative_mode` reads
  `cost-registry.csv` rather than `corpus_cases.inc`, so with q28's cpu cells still `na` the result
  sections are silently not written. The working order is registry → plan goldens →
  `PCK_UPDATE_SECTIONS=1` corpus.
- `rollup-distinct`'s three tp4 cpu cells are off on the existing #189, its signature matched
  verbatim (`Unsupported data type in hasher: UInt8` out of the comet murmur3 hasher). No new
  ticket; `189` stays on the registry row, as the spec directs.
- The ffi rung needs `LD_LIBRARY_PATH` or it exits 127 on `libpeacock_gpu.so`, which reads like a
  red test and is not. `build-test.md` documents it.

Noted and not filed: #65 is 44 lines against the 30-line cap for a deferred-fix ticket. It was over
before this task added a clause, and trimming it here would be scope creep.

### 2026-10-08 — review round 1: 0 blocking, 3 important, 5 nits

The reviewer could not fault the lowering itself: every trap the spec names is built in
structurally rather than advisorily, the inner/outer split holds for all five admissible
companions, each stripped cast is injective (including the `Decimal128(p ≤ 15) → Float64`
boundary, which it re-derived), the grouping-set bit order and the 8/16/32 width boundaries check
out against DataFusion 45's `group_id_array` and `grouping_id_type`, and the inserted goldens'
measured row counts and answers cross-check arithmetically — `distinct-functions`' `stddev` of
1..50 is exactly `sqrt(10412.5/49)` and its `sum_distinct_qty` 1275.00 is 50·51/2.

**Handled by me, all markdown or ticket work:**

- **#62 archived.** Its body stated three things this branch made false, so it moved to
  `archive/archived-tickets.md` as Done, trimmed of the now-implemented "Fix proposed" block and
  carrying what closed it. The index bullet is gone, `tickets.md`'s corpus-coverage row is 32 with
  #62 out of the list, and the header is 114 — every row's declared count still equals its list
  and the column sums to the header. The three live links into the old anchor moved to the archive
  (`complete-coverage.md`, the board). `distinct-companions.md:5`'s link is left alone: the spec is
  frozen and the signoff is its one permitted later write.
- `corpus-coverage.md`'s "Queries only DuckDB answers" list is 17 with q28 dropped, since q28 now
  runs on the cpu at five modes.
- `reports/hacks-audit.md` §10's closing paragraph, which deferred the `distinct` guard's deletion
  to #62, is cut — the guard is gone in this branch.

**Routed to the developer:** the `data_fusion_disabled` guard gap (a corpus line can take the
keyword and get five green cells checking no answer, linked to the end-to-end case by a comment
only), `corpus.rs`'s two doc blocks still counting three oracles and claiming the tier stops every
wrong answer, and four nits — two over-cap in-body comments, two needlessly `pub(crate)` items, an
unused `#[derive(Clone)]`, and the #144 refusal message, which a `Float32` argument reaches with
wording about a second argument where the shape has one.

Not changed, and deliberately: `keeps_distinct` does not admit `Float32 → Float64`. The cast is
injective, but the spec's admitted list is frozen and does not name it, so widening the classifier
is the human's call. The refusal message is what gets fixed.

### 2026-10-08 — round 1 findings, developer's half

All six routed items done, in the working tree, nothing committed. No golden moved
(`git diff --name-only HEAD -- testdata/` is empty) and `recipe-payloads.txt` is still
`b2f9cb2c…`.

**important — `data_fusion_disabled` now has a register that can go red.** `ANSWER_HELD_ELSEWHERE`
in `tests/test_cpu_corpus.rs` is `&[(dataset, query, what holds the answer)]`, one row today:
`tpch/distinct-functions` → `distinct_functions_answer_as_their_hand_lowered_form`.
`every_unchecked_answer_is_held_somewhere()` asserts it equal to the declared set **both ways** —
an unregistered line, and a register row whose line is gone — and is called first from
`each_declarations_two_oracles_suit_each_other`, so an unregistered line is reported for being
unregistered rather than for whatever its goldens do not hold yet. A plain `fn` rather than a
second `#[test]`, so `test_cpu_corpus`'s count does not move. The third field is prose; what is
checked is that a human wrote it.

**Red proved in both directions, by hand, reverted after each:**

- switched `rollup_distinct`'s oracle to `data_fusion_disabled` in `corpus_cases.inc` →
  `these lines declare data_fusion_disabled, so their cpu cells check no answer, and nothing
  says what holds it: ["tpch/rollup-distinct"]. Add a row to ANSWER_HELD_ELSEWHERE naming the
  test that does, or give the line an oracle that compares.`
- added a `("tpch", "rollup-distinct", …)` row with the line left alone →
  `these ANSWER_HELD_ELSEWHERE rows name no data_fusion_disabled line, so each has outlived its
  reason: ["tpch/rollup-distinct"]`

The `corpus_cases.inc` comment now points at the register instead of restating the link, so the
answer lives in one place; `build-test.md` says the set is read off code rather than promised
there.

**important — `corpus.rs`'s two doc blocks.** `assert_answer`'s says four ways, and names the
exception outright: it stops a wrong answer reaching a golden **for the three that compare**, and
for `DataFusionDisabled` the answer is held by a test in `ANSWER_HELD_ELSEWHERE`. `CpuOracle`'s
opens "Three variants run the same oracle … The fourth asks nothing of it, because for that query
DataFusion is wrong". The variant's own doc now names the register rather than "the line's
comment", and says what the cells still do check: the plan, the run and the goldens.

**nit — the two over-cap in-body comments.** Both were 5 against the 4-line cap, counted
mechanically rather than eyeballed. The by-tag/by-position contrast moved into
`state_and_init`'s doc, which had room (5 of 10, now 9). Both bodies are 3 and 4 lines.
A script over every comment block in the five files I touched now reports all within cap.

**nit — visibility.** `DISTINCT_ARG` and `stripped` are private; `grep -rn` over
`peacockdb-core/` found them named nowhere outside `distinct.rs`. `Classified`, `classify` and
`lower` stay `pub(crate)` — `aggregate.rs` names all three.

**nit — `#[derive(Clone)]` on `InitFrom`** deleted; the crate still compiles, which is the proof
it was unused.

**nit — the #144 message, and a bug it exposed.** The old `{name}: a second DISTINCT argument
(#144)` was wrong about the shape for a one-argument query. Reproduced:
`SELECT count(DISTINCT arrow_cast(v, 'Float32')), sum(DISTINCT arrow_cast(v, 'Float32')) FROM
tiny` gave `sum(DISTINCT arrow_cast(tiny.v,Utf8("Float32"))): a second DISTINCT argument (#144)`.
The message now names **both** stripped arguments and says the two ways they can differ —
genuinely, or under a coercion cast the lowering cannot strip. `keeps_distinct` is **not**
widened: `Float32 → Float64` is injective, but the spec's admitted list is frozen and does not
name it.

Two tests, red before the reword:

- `bug_two_distinct_arguments_are_refused` now asserts the message names both arguments
  (`k@`, `v@` — DataFusion's own `name@ordinal` rendering, so the ordinal is not pinned). Without
  that it pinned only `#144`, which the Float32 shape also satisfies, so it no longer pinned
  what it means to pin.
- `bug_one_float32_distinct_argument_under_two_coercions_is_refused` pins the Float32 shape and
  asserts the message does **not** say "a second DISTINCT argument". It goes red the day someone
  widens `keeps_distinct`, which is the signal to delete it.

**Found while fixing, repaired here: a doc comment torn by an insertion** —
`coding-style.md`'s named antipattern, in `tests/test_cpu_corpus.rs` since `ddf3ca2c`, not from
this branch (`git diff HEAD` on that file was empty before round 1).
`every_device_cell_has_a_cpu_cell_at_the_same_mode` and its doc had been inserted into the
middle of `each_declarations_two_oracles_suit_each_other`'s doc, splitting one sentence: "…so it
needs no run — which is" sat above the wrong declaration and "what makes it catch the first
`live_cpu` query BEFORE the rollout…" was left stranded above the right one. Rejoined, and
brought from 17 lines to 10 — the original block was already 14 against the cap, so restoring it
verbatim would have put an over-cap block in this diff. Documentation, so fixed here rather than
filed (`coding-style.md`: a ticket is never about documentation). `test_golden_format`'s
`no_declaration_carries_a_block_left_behind_by_a_split` guards the *split* shape; the *insertion*
shape is contiguous and no guard sees it, exactly as the style page says.

**Counts moved again** by the one new test: `join_refusals` 11 → **12**, `--lib` 618 → **619**,
cpu block 1214 → **1215**, Rust 1886 → **1887**, grand 2361 → **2362**. The page was re-summed
mechanically and the headers equal the row sums. The refusals row's prose says three `bug_` tests
now and why the third is one.

#### Round-1 verification, all from the finished tree

| command | result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2` | 617 passed, 0 failed, 2 ignored |
| `… --test test_cpu_corpus -- --test-threads=2` | 567 passed, 0 failed |
| `… --test test_ci_coverage -- --test-threads=2` | 9 passed, 0 failed |
| `… --test test_corpus_goldens -- --test-threads=2` | 26 passed, 0 failed |
| `… --test test_cost_model -- --test-threads=2` | 3 passed, 0 failed |
| `… --test test_module_layout -- --test-threads=2` | 17 passed, 0 failed |
| `… --test test_golden_format -- --test-threads=2` | 26 passed, 0 failed |
| `cargo build --features rust-only -p peacockdb-core` after `touch lib.rs` | 0 warnings |

Not re-run, and why: the C++ build, `ctest -L cpu` and the two cudf-shape legs. Round 1 touched
no `cpp/`, no `flatbuffers/` and no wire code — `git diff HEAD --stat` is seven files, all Rust
test/planner code and markdown — and the payload golden's sha is unchanged.

#### Ticket candidate for the human, not filed

`count(DISTINCT f32), sum(DISTINCT f32)` is refused where the lowering could compute it: the
`Float32 → Float64` coercion is injective, so deduplicating under it or over it is the same set.
It is pinned by `bug_one_float32_distinct_argument_under_two_coercions_is_refused` and attributed
to #144's number, which is the number the message carries — but it is not #144's shape, and #144
is about an expand this does not need. Widening `keeps_distinct` by one arm would close it. Not
filed because the reviewer called the widening the human's call and a permanent ticket ID is the
same kind of decision; no corpus query reaches it (tpch and tpcds carry no `Float32` column).

### 2026-10-08 — round 1 findings closed

All six applied. `--lib` 617 passed / 2 ignored (the register's assertion is a plain `fn` called
from an existing test, so `test_cpu_corpus` stays at 567), `test_ci_coverage` 9,
`test_corpus_goldens` 26, `test_cost_model` 3, `test_module_layout` 17, `test_golden_format` 26,
zero warnings. No golden moved in this round — `git diff --name-only HEAD -- testdata/` is empty —
and `recipe-payloads.txt` is still `b2f9cb2c…`. The C++ build and `ctest -L cpu` were not re-run,
correctly: this round touched no `cpp/`, no `flatbuffers/` and no wire code.

The register, `ANSWER_HELD_ELSEWHERE` in `tests/test_cpu_corpus.rs`, is asserted equal to the
declared `data_fusion_disabled` set in both directions, and both directions were proved red by hand
and reverted: an unregistered line reports as unregistered, and a register row naming no line
reports as outlived. It is checked before the per-line oracle assertions so the diagnosis is the
right one.

**The Float32 subcase is recorded on #144 rather than filed.** `count(DISTINCT f32),
sum(DISTINCT f32)` is refused where the lowering could compute it, which would ordinarily be a
ticket — but #144 already carries that refusal as its user-visible symptom, and #144's expand
answers the shape correctly, just more expensively than the one `keeps_distinct` arm would. A
second number for a cheaper path to an already-ticketed refusal would duplicate the symptom, so
#144's body gains the subcase, the cheaper fix and the `bug_` test that pins it. `keeps_distinct`
is not widened: the spec's admitted cast list is frozen.

**Noted, not fixed: #65 is 44 lines against the 30-line cap** for a ticket carrying a deferred fix.
It was 43 before this task added the clause the spec asked for. Trimming it means deciding which
detail of a worked-out device fix to throw away, which is the thing the cap's own exception exists
to protect, so it is left for the human.

The developer also repaired a pre-existing torn doc comment in `tests/test_cpu_corpus.rs` —
`every_device_cell_has_a_cpu_cell_at_the_same_mode` had been inserted into the middle of
`each_declarations_two_oracles_suit_each_other`'s doc since `ddf3ca2c`, splitting one sentence
across two declarations. It was rejoined rather than restored verbatim, since the original was
already 14 lines against the 10-line cap.

### 2026-10-08 — completeness reading (analyst, "what is missing")

Read as `git diff ENS-guard-checks...ENS-distinct-companions`, against the spec's Scope,
Restriction, Tests and Verification bar, `architecture.md` and `build-test.md`. No code built or
run. **Three items: one test gap, two stale ticket bodies. Nothing blocking.**

#### `architecture.md`: four sentences this branch falsified

The three places Scope names are true as written. I checked each claim against the code: the
widening-cast list matches `keeps_distinct`; the inner stage's `(x, keys)` with `x` first and
unmasked in every set matches `inner_stage`; the by-position pairing, the two finalize duties and
the two named refusals match `state_and_init`, `decompose` and `classify`; `grouping_id_type`'s
8/16/32 boundaries match DataFusion 45 (`datafusion-expr-45/src/logical_plan/plan.rs:3223`); and
the `(deprecated)` claim holds on both generated sides — the regenerated
`target/debug/build/peacockdb-core-*/out/gpu_plan_generated.rs` contains the string `distinct`
zero times. "Node display" is unaffected.

Four sentences elsewhere are now false. Two are in sections Scope did not name, which is the
fourth-section case the coordinator asked about.

1. **"Grouping sets"** — a section the branch did edit, keeping this sentence and moving it into
   its own paragraph:
   > The gid's rendering is asymmetric on purpose — the init's `group_by` does not list it,
   > because there it is a tag being synthesized, while every node above lists it as an ordinary
   > key.

   The outer stage's first node is an init and it *does* list the gid.
   `tpch.sf1/tp1-single.plans.txt`, `== rollup-distinct`:
   `GpuAggregate: group_by=[l_returnflag@1, l_linestatus@2, __grouping_id@3]`, whose recipe is
   `execute_node(#4 CudfAggregate{Partial}, batch)`. The real rule is "the node that synthesizes
   the gid does not list it; every node that reads it does" — the outer init reads it.

2. **"The aggregate sequence"**:
   > The shuffle is skipped only for one-lane inputs or keyless aggregates — skipping on small
   > key cardinality needs estimators that do not exist (#141).

   There is a third case now, and it is multi-lane and keyed: the outer stage takes
   `Shuffle::None` because the inner's hash is on a subset of its keys.
   `tp4-single-mini.cpu.txt`, `== distinct-functions`: the outer `GpuAggregate` is
   `group_by=[l_returnflag@1], lanes=4, hashed_on=[l_returnflag@0]` with no `GpuEmitPartitions`
   beneath it. The architecture's own DISTINCT section asserts the opposite of the "only".

3. **"The aggregate sequence"**:
   > **init** — aggregators over raw rows, emitting *state* columns.

   The outer stage's init runs its aggregators over the inner stage's *state* columns
   (`sum(avg(lineitem.l_quantity)$sum@4)` in a `CudfAggregate{Partial}`). #261's body states this
   plainly; this line does not.

4. **"What the Rust side puts in the flat buffers"**, the `CudfAggregate.mode` row — same fact as
   3, lower confidence that it needs a word:
   > the phase: `Partial` builds state from values, `Merge` merges state into state.

   A `Partial` can now build state from another node's state.

Judgement call I did **not** count as falsified: the decomposition table's `count` finalize `o`.
The spec deliberately leaves `plan/aggregates.rs`'s finalize bare and wraps in the translator, and
the DISTINCT section documents the `CASE` wrap, so the table still describes the registry.

#### 1. (important) No test has a `sum`, `min` or `max` *companion* beside a DISTINCT

Every DISTINCT test in the branch pairs with `count`, `count(*)` or `avg`: the four plan tests,
the four DataFusion end-to-end cases, q28 (`avg, count`), `rollup-distinct` (`avg, count(*)`) and
`distinct-functions` (`count(*), avg`). Every `sum` in them is a `sum(DISTINCT …)`, never a
companion.

That leaves one branch of `decompose` unentered by the suite — the spec's fifth trap, the one the
review of the spec found:

    if matches!(spec.func, AggFunc::Sum | AggFunc::Min | AggFunc::Max)
        && state[0].data_type() != &out_type
    { finished = Expr::Cast { … } }

The shape is reachable: DataFusion's `single_distinct_to_groupby` declines a node as soon as one
companion is outside `{sum, min, max}` (`is_single_distinct_agg`, verified in 45.0.0), so
`sum(y), avg(z), count(DISTINCT x)` arrives with the flag and a `sum` companion. On a decimal the
outer init's `sum` over a `Decimal128(25,2)` state declares `Decimal128(35,2)` where DataFusion
declares `Decimal128(25,2)`, and only this cast reconciles them. Nothing proves the cast is there,
and nothing would notice if it stopped being emitted.

The spec's Tests section does not name this case, so the branch is letter-compliant; the gap is
the contract's. Cheapest close: add `sum(l_extendedprice)` to
`a_grouped_count_and_sum_distinct_beside_companions_answer_as_datafusion`
(`src/tests/end_to_end.rs`), which buys the five-mode DataFusion oracle for free, or assert the
narrowing cast in the existing `avg`-companion plan test.

#### 2. (important) `#189`'s body went stale, and this branch is what staled it

`llm-wiki/tickets/corpus-coverage.md`, #189. Two parts:

- **Corpus queries.** "`tpch/rollup-over-join` and tpcds q5, q18, q22 and q80 … 15 cpu cells" is
  now six queries and 18 cells: `rollup-distinct`'s three tp4 cpu cells are off on #189, as its
  registry row says. `build-test.md` was updated for exactly this (`tpch/rollup-over-join` **and
  `tpch/rollup-distinct`** three by #189); the ticket was not. "The 15 cpu cells turn on" in the
  Fix proposed is short by three.
- **Fix proposed**, which now points at code that does not exist: "In `aggregate_sequence`, before
  `tree = match shuffle`, drop the id's ordinal from `Shuffle::ByHash`'s keys … `keys.retain(|k|
  *k as usize != group.expr().len())`". `tree = match shuffle` moved to `sequence`
  (`translator/aggregate.rs:556`), `group` is not in scope there, and the inner stage's id ordinal
  is n + 1 rather than n. In `sequence` the id is simply the last `key_fields` entry, which covers
  both stages. The spec foresaw the ordinal ("its `drop_grouping_id` lands in `sequence`, keyed on
  each stage's own id ordinal (n + 1 in the inner stage)") but assigned only the registry half to
  "whichever lands second". This task landed first, so the next developer on #189 reads an
  accurate-looking one-liner against a function that moved.

#### 3. (important) `#65`'s corpus-query line is short by one, and its closing claim is now false

Same file, #65. The prose line the spec asked for landed. The line beneath it did not:

> **Corpus queries:** every corpus plan with a grouping-set id: `tpch/rollup-over-join` and tpcds
> q5, q14, q18, q22, q77, q80. None reaches it on the device yet: each cell is off first on #152,
> #189 or #220.

`tpch/rollup-distinct` is a new corpus plan with a grouping-set id and belongs in that list. And
"off first on #152, #189 or #220" no longer holds: `rollup-distinct`'s two tp1 gpu cells are off
on **#65 itself** (registry tickets `65 189`; #189 is tp4 only). #65 went from blocking no cell
by itself to being the first blocker of two — which is what a rollout reads this line for. (#262
sits behind it, per #262's own body, so #65's fix alone does not turn them on.)

#### Checked and sound — not findings

- **Every test the spec's Tests section names exists and asserts what the spec says.** Seven
  plan-shape cases (`translator/tests.rs`: grouped at four lanes with `hash_keys == [1]` and the
  co-located outer, keyless at four lanes, `x` already a key, the `avg` companion asserting
  `Decimal128(35,2)`, `count` + `sum(DISTINCT)` on one inner key, the ROLLUP with `!mask[0]` in
  every set *and* no narrowing cast, the eight-key ROLLUP with `UInt16` narrowed to `UInt8`); five
  end-to-end cases at all five `MODES`, four against DataFusion and `distinct-functions` through
  the new `sql_answers_match_oracle`; three `bug_` tests (#144 twice, #261); and the direct
  `decompose` refusal in the new `aggregate/tests.rs`. The empty-keyless case uses the spec's
  `ss_quantity + ss_item_sk < 0` predicate, not the prunable one.
- **The three corpus lines and the registry rows match the spec cell for cell.** 123
  `corpus_query!` lines expand to 563 cpu cells, exactly what `build-test.md` claims; q28 five cpu
  cells enabled with `62` struck and `152` named, `rollup_distinct` tp1-only with `65 189`,
  `distinct_functions` all five with `262`. Plan cells enabled in all three rows, which
  `plan_goldens.rs` requires now that the sections hold plans (`declared == "enabled"` for a body
  that does not open with `refused`). Result sections exist for all three hyphenated/new queries;
  the tp4 cost files carry `skipped: not enabled at this mode` for `rollup-distinct`.
- **`build-test.md` adds up.** 2362 = 1887 + 94 + 381; cpu 1215 = 619 + 567 + 26 + 3; the
  `--lib` delta +15 = end-to-end +5, refusals +2, translator +7, the new aggregate module +1;
  566 = 563 cells + 3 checks, and there are exactly four non-generated `#[test]`s in
  `test_cpu_corpus.rs`. Golden file counts (tpch.sf1 39, tpcds.sf1 116) are unchanged, correctly:
  the new queries are sections, not files, and only the 22/99 numbered queries carry a
  `duckdb_cost.txt`.
- **Ticket work.** #62 archived with a Done body; #65's line, #195's bullet, #261's pin all
  landed; #262's body is accurate, including why q28's and `rollup-distinct`'s registry rows do
  not name it ("carry it too once those close"). `every_refusal_names_a_ticket_that_exists` reads
  the archive too, so the move is safe. No live wiki page still cites #62 as open;
  `reports/corpus-fixes.md` and `reports/bugfix-proposals/*` do, and are snapshots ("read at
  `188c23ce`"), while the live `reports/hacks-audit.md` §10 was correctly cut.
- **Restriction honoured.** The only C++ change is the guard's deletion and the three gtest
  arguments; no `cpu_backend/`, no device fix for #65; `recipe-payloads.txt` absent from the diff.

#### Secondary notes, deliberately not raised as findings

- `#228` (DF 55's duplicate ordinal) gains a second consumer of `grouping_id_type` with a +1 key
  and a narrowing project. Its fix text says the width must come "from the plan's declared type
  rather than from `nkeys`", which now has to be true in two places.
- The lowering admits shapes no test reaches that cost nothing today: CUBE and GROUPING SETS
  (same `group.groups()` path as the tested ROLLUP), a DISTINCT with **no** companion under a
  grouping set (an inner stage with an empty `aggs` list — the `SELECT DISTINCT` shape, already
  supported), and the argument being *also* a masked key under a rollup, which is the case
  `__distinct_arg`'s own column exists for.
- `count(DISTINCT a+b)` beside an `avg` now makes an expression inner group key with no `GROUP BY`
  in the SQL, which `aggregate.cpp` refuses at run time ("only ColumnRef group exprs supported").
  Pre-existing for any `GROUP BY <expr>`, untracked by any ticket, and masked by #262 while the
  device cells are off.
- The `corpus_cases.inc` comments name #262 for q28 and `rollup-distinct` where their registry
  rows name only their own blockers. Consistent with #262's body; the comment is the looser of
  the two.
- No GPU run, every gpu cell off under #262, no `duckdb-result.txt` section owed, `rollup-distinct`
  tp4 cpu off on #189 — all as the coordinator's context states, and none of them findings.

### 2026-10-08 — completeness pass: 0 blocking, 7 important across the two readings

The two readings never saw each other's list and converged on one gap, which is the finding worth
reading twice: **the fifth trap's cast-back has no test.** The analyst found it as a missing
companion shape, the reviewer as unreached code at `translator/aggregate.rs:309-318`. Nothing on
the branch pairs a plain `sum`, `min` or `max` companion with a DISTINCT — q28, both new queries
and all seven plan tests use `avg` and `count`, whose own finalize casts — so the only live arm,
a decimal `sum` companion whose outer state widens to `Decimal128(35, 2)` against DataFusion's
`(25, 2)`, is never entered. The shape is reachable: `is_single_distinct_agg` declines a node as
soon as one companion is outside `{sum, min, max}`. The spec's Tests section never names it, so
the branch is letter-compliant and the gap is the contract's. Routed to the developer.

**Applied by me, the markdown and ticket half:**

- `architecture.md`, four sentences the branch falsified. The three sections the spec's Scope names
  were already true; these are the fourth-section cases. "Grouping sets": the gid asymmetry belongs
  to the init that *synthesizes* the id, not to inits in general — the outer stage is an init and
  lists it as a key. "The aggregate sequence", twice: the shuffle is skipped for a third case, a
  stage already co-located on its keys, and an init reads the columns its stage is handed rather
  than always raw rows. "What the Rust side puts in the flat buffers": a `Partial` over state
  columns is how the outer init merges a companion, so the merge rule rides in the aggregator and
  not in the mode.
- #189: its corpus list is six queries and 18 cpu cells, and its Fix proposed pointed at code this
  task moved — `tree = match shuffle` is in `sequence` now, `group` is out of scope there, and the
  id's ordinal is the stage's own, one higher in the inner stage. The fix now says to read the
  ordinal off the stage.
- #65: `tpch/rollup-distinct` added to its corpus list, and its claim that no cell is off on #65
  first corrected — `rollup-distinct`'s two tp1 gpu cells are. Its five-line enumeration of four
  pin names became the three files that hold them, since the names are the code's.
- #144's new paragraph was in the present tense and so contradicted itself: the Float32 arm *would*
  answer the shape, and does not. Mood fixed.

**Still over the ticket cap and left alone: #65 at 38 lines and #261 at 35, against 30 for a ticket
carrying a deferred fix.** Both were over before this task. Trimming either means deciding which
detail of a worked-out device fix to throw away, which is what the cap's own exception exists to
protect, so it is the human's call rather than mine.

### 2026-10-08 — completeness pass, developer's three

All three done, uncommitted. No golden moved (`git diff --name-only HEAD -- testdata/` empty),
`recipe-payloads.txt` still `b2f9cb2c…`. No `cpp/`, `flatbuffers/` or wire code in scope.

#### 1. The cast-back is tested — and it is NOT what makes the cpu answer right

Closed twice as asked, but **one of the two cannot go red, and the reason matters more than the
test does.** A plan test and the `sum` companion were both added; only the plan test pins the
cast-back.

- `a_decimal_sum_companion_casts_its_finalize_back_to_the_declared_type`
  (`translator/tests.rs`), over `SELECT c_nationkey, sum(c_acctbal), count(*),
  count(DISTINCT c_mktsegment) FROM customer GROUP BY c_nationkey` on the minimal dataset.
  `c_acctbal` is `Decimal128(15, 2)`, the inner state `(25, 2)`, the outer init `(35, 2)`,
  DataFusion declares `(25, 2)`. It asserts the outer init's state type, that the finalize is
  an `Expr::Cast` to `Decimal128(25, 2)`, and that the node DECLARES `(25, 2)` for that column
  — under `validate_all`.
- `sum(l_extendedprice)` added to
  `a_grouped_count_and_sum_distinct_beside_companions_answer_as_datafusion`, five modes.

**Red proof, and the finding inside it.** With the cast-back behind `if false &&`:

- the plan test fails — `Column(ColumnRef { index: 1, name: "sum(customer.c_acctbal)" })`, a
  bare column where a cast is wanted;
- the end-to-end case **passes**, at all five modes.

Not a plan-time schema error, and not a wrong answer either. I probed the executed output type
at every mode with the cast-back disabled: `sum(lineitem.l_extendedprice)` still comes back
`Decimal128(25, 2)`. The cause is `declared_as` (`executor/cpu_backend/mod.rs:309`), which
already casts a `widened_decimal` — same scale, wider precision — back to the declared type per
batch, with `safe: false` so a value that does not fit errors rather than becoming NULL. Its
own comment says why it exists: "a state merged twice would carry a wider type than one merged
once". So on the cpu the plan-level cast-back is **redundant by construction**, and no cpu run
of any shape can observe its absence.

What the cast-back is actually for, then: the PLAN declaring what it produces. Without it the
finalize project claims `(25, 2)` and its expression yields `(35, 2)` — a plan that lies about
itself. `declared_as` papers over that on the cpu; a device has no `declared_as` and the sink's
schema validator refuses the divergence, so the cast-back is what makes this shape device-runnable
at all. Untestable on this chain (no GPU, #262).

Plan validation does not catch the lie either, and that is a gap rather than a decision:
`validate_all` passed with the cast-back disabled, because `check_expr_types`
(`plan/common.rs:44`) only checks each type is holdable and `finalize_columns` checks the
finalize list's WIDTH and the key names — nothing compares an expression's result type against
the field it is declared as. See the ticket candidate at the end.

The e2e case stays: it is the only five-mode value check of a plain decimal `sum` companion
beside a DISTINCT, which nothing had. Its doc now says the type is the plan test's and why.

**DataFusion is a sound oracle for it**, checked rather than assumed. I ran the new SQL and its
hand-lowered equivalent — the DISTINCT aggregates over `SELECT DISTINCT l_returnflag,
l_suppkey`, joined to the companions over `lineitem` — through DataFusion at
`target_partitions = 1` and compared: identical, row for row, including `se`. The values check
independently too: `count(DISTINCT l_suppkey)` 10000 and `sum(DISTINCT l_suppkey)` 50005000 =
10000·10001/2, and `n`/`a` match `rollup-distinct`'s committed golden. `is_single_distinct_agg`
declines the node because `count(*)` is outside `{sum, min, max}`, so DataFusion runs its own
distinct accumulators — which it gets right for `count`/`sum(DISTINCT)`; the shapes it gets
wrong are `stddev`/`var(DISTINCT)`, keyless `avg(DISTINCT)` and grouped decimal `avg(DISTINCT)`,
none of which is here. The probe was a throwaway and is not in the tree.

#### 2. The register verifies every claim it makes

`ANSWER_HELD_ELSEWHERE` is now a `struct AnswerHeldElsewhere { dataset, query, file, test, why }`
rather than a tuple with prose in it — the `TEST_ONLY_ITEMS` shape, and for its reason. Three new
assertions beside the two set-equality ones: `why` is non-empty, `file` exists under
`CARGO_MANIFEST_DIR`, and `file`'s text still contains `test`. The `test` field's doc carries the
VERIFIED note, as `Exemption::GpuJob`'s does.

**Each proved red by hand, reverted after each; three distinct messages:**

- `why: ""` → `tpch/distinct-functions: names distinct_functions_answer_as_their_hand_lowered_form
  and says nothing about why DataFusion is no oracle — the reason is the one part of this row a
  reader cannot derive`
- `file` → `src/tests/end_to_end_renamed.rs` → `… names src/tests/end_to_end_renamed.rs as
  holding its answer and that file is gone`
- `test` → `…_v2` → `… names distinct_functions_answer_as_their_hand_lowered_form_v2 in
  src/tests/end_to_end.rs and that file no longer holds it, so five cpu cells check no answer and
  nothing else does either`

#### 3. The #144 message pin is live

`planner/tests/join_refusals.rs` pinned POSITIVELY on the differing-arguments arm's own words,
`"where another's is"`, instead of negating a phrase round 2 had already removed from the tree.
Proved by swapping the two arms' messages by hand: both `bug_two_distinct_arguments_are_refused`
and `bug_one_float32_distinct_argument_under_two_coercions_is_refused` then fail, each reporting
`a DISTINCT over more than one argument (#144)` for a shape that is not that. Restored, both green.

#### Counts, re-summed mechanically

The one new plan test: translator 36 → **37**, `--lib` 619 → **620**, cpu block 1215 → **1216**,
Rust 1887 → **1888**, grand 2362 → **2363**. `test_cpu_corpus` stays 567 — the register check is
a plain `fn` called from an existing test, deliberately, so it adds no case. Every header equals
its rows and the cpu breakdown sums to its block.

| command | result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2` | 618 passed, 0 failed, 2 ignored |
| `… --test test_cpu_corpus -- --test-threads=2` | 567 passed, 0 failed |
| `… --test test_ci_coverage -- --test-threads=2` | 9 passed, 0 failed |
| `… --test test_corpus_goldens -- --test-threads=2` | 26 passed, 0 failed |
| `… --test test_cost_model -- --test-threads=2` | 3 passed, 0 failed |
| `… --test test_module_layout -- --test-threads=2` | 17 passed, 0 failed |
| `… --test test_golden_format -- --test-threads=2` | 26 passed, 0 failed |
| `cargo build --features rust-only` after `touch lib.rs` | 0 warnings |

#### Second ticket candidate for the human, not filed

**Plan validation cannot see a project whose expression type is not the field it declares.**
`check_expr_types` checks holdability per type; `finalize_columns` checks the finalize list's
width and the key names. Neither compares a result type against its declared field, so a
finalize (or any) project may declare `Decimal128(25, 2)` and produce `(35, 2)` and
`validate_schemas_and_partitions` passes — demonstrated above by disabling the cast-back and
watching `validate_all` stay green while the plan lied. On the cpu `declared_as` hides it; on
the device the sink refuses it, so the symptom is a device-only failure for a plan that validated.
General and pre-existing, not introduced by this branch, and the fix is a type check in
`plan/`. Not filed because a permanent ID on a validation rule's scope is your call, and the
first branch to need it is the one that runs this shape on a device (#262).

### 2026-10-08 — completeness findings closed

All three applied. `--lib` 618 passed / 2 ignored, `test_cpu_corpus` 567 (the register check is a
plain `fn` called from an existing test, so it adds no case), `test_ci_coverage` 9,
`test_corpus_goldens` 26, `test_cost_model` 3, `test_module_layout` 17, `test_golden_format` 26,
zero warnings. No golden moved; `recipe-payloads.txt` still `b2f9cb2c…`. `build-test.md` re-summed:
translator 37, `--lib` 620, cpu block 1216, Rust 1888, grand 2363.

**The cast-back finding came back with a correction worth keeping.** Both readings predicted the
plan test and the end-to-end case would go red without the cast-back. Only the plan test does. With
the cast-back disabled the end-to-end case passes at all five modes and the executed output type is
still `Decimal128(25, 2)`, because `declared_as` (`executor/cpu_backend/mod.rs:309`) already casts a
widened decimal — same scale, wider precision — back to the declared type per batch, `safe: false`
so an overflow errors rather than becoming NULL. So on the cpu the plan-level cast-back is
redundant by construction and **no cpu run can observe its absence.** What it is for is the plan
declaring what it produces: without it the finalize project claims `(25, 2)` while its expression
yields `(35, 2)`. A device has no `declared_as` and the sink's schema validator refuses the
divergence, so the cast-back is what makes this shape device-runnable — untestable on this chain,
under #262. The plan test is therefore the only possible pin, and it asserts the cast, the outer
state type and the node's declared type. The end-to-end case stays as the only five-mode value
check of the shape, with its doc saying where the type is pinned instead.

DataFusion is a sound oracle for the added `sum` companion, checked rather than assumed: the new
SQL and its hand-lowered equivalent agree row for row at one lane, and the values check
independently (`sum(DISTINCT l_suppkey)` 50005000 = 10000·10001/2). `is_single_distinct_agg`
declines the node on `count(*)`, so DataFusion runs its own distinct accumulators — right for
`count` and `sum(DISTINCT)`, and none of the three shapes it gets wrong is in that query.

**The developer's second ticket candidate is not filed, because `architecture.md` already records
it.** "Plan validation cannot see a project whose expression type is not the field it declares" is
the state the page states under *Types are a plan fact*: "a project's expression is compared against
nothing at plan time — what the device produces for it is held to the declaration per batch by the
test harness and the corpus's validator — and the C++ half is #164." A ticket would re-file a
documented property, and nothing behaves wrongly for a user today. The `Float32` candidate stays
recorded on #144.

### 2026-10-08 — done

CI run `37859319653` on head `a141b055`: Changed paths, both cuDF matrix legs, the 25.02 GPU
build, the cost report and the S3 metadata check all green; Deploy pages skipped as a master-push
job. The cost-report job's pass is worth naming, since it is what exercises #62's move to the
archive — `TicketIndex` resolves a number to whichever file holds its anchor.
`GPU Tests (remote)` failed on `ssh: connect to host llm-gpu0h200.velkerr.ru port 22: Connection
timed out`, the same unreachable host as guard-checks saw, which chain K does not wait on.

The task is terminal for the ensemble. The human merges; PR #171 targets `ENS-guard-checks`, so
that one merges first.

## Rebase needed: the chain is moving to master 31c56bea (2026-10-09)

The human wrote `rebase` to `.claude/ensemble/K.control`. guard-checks has already been
rebased onto master 31c56bea; this branch's base is therefore about to move and is marked
before anything is fixed on the parent. `guard-checks-detail.md` carries what master
brought across.

This branch **will** conflict in code-adjacent files, unlike guard-checks: master's
`64ced62e` and this branch both edit the same ten tpcds goldens —
`tp{1,4}-{single,rowgroup,sized}.plans.txt` and `tp{1,4}-*-mini.cpu.txt`. The resolution is
a regeneration by a developer, not a hand-merge: master changed how a join's projection and
a null decimal render, this branch added q28's sections, and only a run can say what the
combined text is.

### 2026-10-09 — rebased onto guard-checks on master 31c56bea

`git rebase --onto ENS-guard-checks pre-rebase-K-guard-checks ENS-distinct-companions`. The
`--onto` form is the one that works here: a plain `git rebase ENS-guard-checks` tried to replay
guard-checks' own pre-rebase commits, because only three of the nine were recognised as already
applied.

**Every conflict was wiki, none was code.** All ten tpcds goldens master and this branch both
touch auto-merged — `tp{1,4}-{single,rowgroup,sized}.plans.txt` and the `tp*-mini.cpu.txt`
beside them. That is a textual merge no run has checked, and it is the one thing the re-proving
dispatch has to settle: master's `64ced62e` moved how a join's projection (q10, q35, q45, q58,
q83) and a null decimal (q90) print, while this branch inserted q28's sections into the same
files.

Four conflicts, resolved by ownership:

- `build-test.md`'s two count lines, four times over (once per commit of this branch that moved
  them). Resolved each time as `ours + (theirs − base)` per field, with the grand total asserted
  equal to Rust + C++ + Python and the cpu block equal to the sum of its binaries. The end state
  is **2366 — Rust 1891, C++ 94, Python 381**, cpu `1220: --lib 624, test_cpu_corpus 567,
  test_corpus_goldens 26, test_cost_model 3`. guard-checks' re-prove corrected the Cost-report
  renderer row from 37 to 36 on the way through, and that correction survived; so did master's
  new `Join projection names` row and its two moved plan-text rows.
- `archive/archived-tickets.md` — an add/add at the top of the file. Both sides kept: #62 first,
  then master's #237 and #236, which is the file's newest-archived-first order.
- `tickets.md`'s index — master's side for which tickets exist (#280, #281, #282, #283 are
  master's), this branch's side for #62 leaving corpus-coverage. 118 open, corpus-coverage 35.
- `tasks.md` — master's side for tasks 3 and 4 (`limits` and `empty-sorts`, `approved to build`)
  and for the chain note's new cost-gate bullet; this branch's side for states 1 and 2.

### 2026-10-09 — re-proved on the rebased base

Head `a1e36716`, base `ENS-guard-checks` at `203bd92a` (master `31c56bea`). Nothing was
edited: the whole bar ran green on the rebased tree as the rebase left it, and
`git status` is clean after every run.

#### The auto-merged goldens are what the tree produces

This was the one open question, and it is closed three ways.

- **Every plan golden tier green.** `planner::tests::plan_goldens` — all ten per-mode tiers
  plus the six meta tests — 19 passed, 0 failed. The ten tpcds files master and this branch
  both edited verify byte for byte against a fresh render.
- **The cpu tier green.** `test_cpu_corpus` 567 passed, which is what reads and verifies the
  `tp*-mini.cpu.txt` sections master rewrote (q10, q35, q45, q58, q83) and the ones this
  branch inserted (q28). `test_cost_model` re-derived all three `.cost.txt` files.
- **Every line master added survives verbatim.** Mechanical check, not a reading: for each of
  the ten files, every line `64ced62e` added was looked up with `grep -qxF` in the merged
  file. **117 master-added lines, 0 missing** — 12 per `plans.txt`, 12/12/11/11/11 in the
  `cpu.txt` files.

**No golden was red, so nothing was regenerated.** `UPDATE_CANONICAL=1` was never run and
no golden byte moved.

Why the merge was safe where it looked risky: q28's six `GpuCrossJoin` nodes carry **no**
`projection=` field at all, so `projection_field` returns before master's rule can apply, and
q28 has no null decimal literal. Master's two renderer changes and this branch's inserted
sections touch disjoint text. That is the reason the patch arithmetic happened to be right —
worth recording, because the next task that collides in these files will not necessarily be.

#### The deletion audit

`git diff 203bd92a HEAD -- <file> | grep -c '^-[^-]'` over every file in the diff, with the
text of each deletion read:

| file | deletions | what they are |
|---|--:|---|
| the five `tpcds.sf1/*.plans.txt` | 1 each | the `(#62)` refusal line, this branch's own |
| `cost-registry.csv` | 1 | q28's row, rewritten |
| every other file under `testdata/goldens/` | 0 | pure section insertion |
| `recipe-payloads.txt` | — | absent from the diff; sha still `b2f9cb2c…`, equal to the base's |

Six deletions, six accounted for. **Identical to the pre-rebase baseline** this file records
under *Goldens that moved, and every one audited* — so the textual merge lost no section. A
lost master section would have shown up here as a deletion, since reverting master's line is
a deletion of it; there is none.

#### The bar, all from the rebased tree

| command | result |
|---|---|
| `cargo test --features rust-only -p peacockdb-core --lib -- --test-threads=2` | 622 passed, 0 failed, 2 ignored (#182) |
| `… --test test_cpu_corpus -- --test-threads=2` | 567 passed, 0 failed |
| `… --test test_corpus_goldens -- --test-threads=2` | 26 passed, 0 failed |
| `… --test test_cost_model -- --test-threads=2` | 3 passed, 0 failed |
| `… --test test_ci_coverage -- --test-threads=2` | 9 passed, 0 failed |
| `… --test test_module_layout -- --test-threads=2` | 17 passed, 0 failed |
| `… --test test_golden_format -- --test-threads=2` | 26 passed, 0 failed |
| `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib -- ffi_tests::` | 4 passed, 0 failed |
| `CUDF_ROOT=… scripts/cargo-cudf.sh test -p peacockdb-core --lib -- the_payload_golden` | 2 passed, 0 failed |
| `scripts/build.sh --cudf_ROOT ~/data/miniforge3/envs/rapids-cuda-12.2 --gcc-version 12 --build` | exit 0, 0 warnings, `peacock_plan_tests` linked |
| `ctest --test-dir cpp/build -L cpu` | 1/1 passed |
| `cargo check --features rust-only -p peacockdb-core --all-targets` after `touch lib.rs` | exit 0, 0 warnings |
| `cargo test -p cost-report` (for the count check) | 36 passed, 0 failed |

The cudf rung again needed `LD_LIBRARY_PATH` — `$PWD/target-cudf-rapids-cuda-12.2/debug/build/peacockdb-ffi-c2ff8be8e892f44d/out/lib`
and `~/data/miniforge3/envs/rapids-cuda-12.2/lib` prepended. Without it the command exits 127
and reads like a red test.

**The warning check was itself proved, because 1.2 s looked too fast to be real.**
`--message-format=json` says `peacockdb_core (lib)` and all nine `--test` binaries came back
`fresh: false` — rechecked — with every one of the 252 units above them fresh. Then an unused
local was appended to `lib.rs`: the check reported `warning: unused variable`, the probe was
removed, and the clean run was repeated. So the zero is a measured zero, not a skipped build.
`cargo check` is metadata-only over 2.7 MB of source with warm deps; 1.2 s is its real cost.

#### Counts: the page is right, nothing to correct

`build-test.md`'s two count lines on this head are **2366 — Rust 1891, C++ 94, Python 381**
and cpu `1220: --lib 624, test_cpu_corpus 567, test_corpus_goldens 26, test_cost_model 3`.
Both reproduce from the tree. The human's four `ours + (theirs − base)` resolutions landed
exactly right.

From `scripts/case-inventory.sh rust-only`, i.e. off the compiled binaries:

    --lib 624 · test_cpu_corpus 567 · test_corpus_goldens 26 · test_cost_model 3
    test_ci_coverage 9 · test_module_layout 17 · test_golden_format 26
    test_gpu_corpus 0 · test_node_timing 0   (compiled out under rust-only)

624 + 567 + 26 + 3 = **1220**, the cpu block. Every figure also equals its test run's
`test result:` line. Rust = 1220 (cpu) + 7 (ffi) + 576 (gpu) + 26 + 9 + 17 + 36 = **1891**.
ffi 7 = 4 measured on the cudf rung + 3 in `peacockdb-ffi/tests/test_ffi.rs`. C++ 94 = 80 from
`--gtest_list_tests` on the five binaries `cpp/build` builds (plan 56, cpu 12, gpu 4, tpch 4,
tpchv 4) + 14 source-counted in the five manual/2gpu files it does not build. Python 381 =
41 (`test_duckdb_cost.py`) + 328 (`exec_model`, of which `test_tpch.py` 19 and
`test_tpcds.py` 71 + `test_tpch_corpus.py` 22 = 93 corpus) + 12 (calibration).

The row sums were also diffed against the base's page. **The only rows that moved are this
branch's own, and every delta is the spec's:**

| row | base → head | why |
|---|---|---|
| Corpus, cpu | 554 → 566 | q28's 5 cpu modes + `rollup-distinct`'s 2 tp1 modes + `distinct-functions`' 5 |
| End to end | 29 → 34 | the five DISTINCT-lowering cases |
| Translator, one rule at a time | 29 → 37 | the plan tests of the two stages |
| Planner join refusals | 10 → 12 | the two `bug_` tests for #261 and #144 |
| DISTINCT reaching decompose | — → 1 | new row |

ffi, gpu and the whole *Everything else* table are unchanged from the base, and neither this
branch nor master's `64ced62e` touches any `gpu_tests` module or gpu `--test` binary — so
gpu 576 carries over and is checked by row arithmetic alone, the `gpu` shape being a build
this chain may not do. Three rows were spot-checked against the inventory's case names
directly: End to end 34, Translator 37, Planner join refusals 12 — all exact. Note a bare
`#[test]` grep undercounts End to end (18), because its per-query cases are macro-generated;
the inventory is the authority.

guard-checks' `Cost-report renderer (Rust)` correction holds: `cargo test -p cost-report`
reports **36**, matching the row.

#### One piece of wiki drift, pre-existing, not fixed here

`build-test.md`'s grand-total sentence says "a target's own `--list` total is larger, because
its registry test is counted once in Registry ↔ CSV rather than again in each tier it belongs
to." Measured, no rust-only target behaves that way: for all seven the `--list` total **equals**
its row sum (`test_cpu_corpus` 567 = 566 + 1, `--lib` 624 = 624, and so on), and the gpu
block's `test_gpu_corpus` 28 = 27 + 1 likewise. The clause is on master `31c56bea` and on the
base `203bd92a` unchanged, so it is not the rebase's doing and the numbers it guards are all
correct. Left for the helper rather than edited here — it is the sentence that exists to stop
false drift reports, and rewriting it is master's call, not this branch's.

#### No GPU, as the chain requires

No device run, no GPU cycle, no `--features gpu`, no shad-gpu, verda never attempted. The C++
was built and `ctest -L cpu` run; `peacock_plan_tests` was linked and **not** executed. Every
command carried an explicit `timeout`. Nothing in git was mutated — head still `a1e36716`,
working tree clean, stash untouched.

### 2026-10-09 — done again on the rebased base

CI run [37881237558](https://github.com/asymptote-tech/peacockdb/actions/runs/37881237558) on
`a1e36716`, the rebased code head. Every job green — changes, both dataset-matrix legs,
cpp-build-2502, cost-report and s3-datasets; deploy-pages skipped as a master-push job. The
cost-report job going green is the one that matters most here: it is what re-reads q28's and
the two new queries' cost sections.

GPU Tests failed at rsync with `ssh: connect to host llm-gpu0h200.velkerr.ru port 22:
Connection timed out`, the same unreachable host as before the rebase. No pool was built, so
it is not [#178](../tickets/testinfra.md#t178) and not this branch; chain K does not wait on
that job.

## The GPU half — reopened 2026-10-09

The human reopened this task from `done` to `building`. The cpu lowering is closed and #62 is
archived; what is left is the half the chain could not run while it had no GPU, filed as
[#262](../tickets/corpus-coverage.md#t262): **no device has ever run a plan from this lowering.**
The two-stage shape reaches the wire — an outer init running merge aggregators over state, a
`__distinct_arg` key, a narrowed grouping id — and a device answer could differ from the cpu's
with nothing to say so, because the cells are off.

### Second rebase, onto master bc9b6e2f through ENS-guard-checks

Fourteen commits replayed, every conflict bookkeeping:

- `tasks.md`, once — this branch's own state progression, so the replayed side stands.
- `build-test.md`'s grand-total line, four times. The replayed side is an **absolute** computed
  off the old base, so taking it would have discarded the new base's own count; each was resolved
  by applying the replayed commit's delta to the base instead (+26, +1, +1, and one more). Then
  the result was checked the way the page says it should be — by summing the rows, not the
  deltas: first table 1803, the second table's Rust rows 90, so Rust 1893, C++ 94, Python 381,
  grand total **2368**, which is what the header reads.

No code, golden or test conflicted. The branch is 14 commits above `ENS-guard-checks`, which is
the task's own count, so the PR diff is still the task.

### What the GPU run must do

`tpch/distinct-functions`' five device cells, off on #262 alone, are the ones to try. Each cell
that passes against the cpu golden is enabled; each that fails gets the ticket it fails on, and
#262 narrows to what is left rather than closing.

Two queries are **not** this run's business, and their cells stay off: tpcds q28 is a cross join
behind [#152](../tickets/joins.md#t152), and `tpch/rollup-distinct` is behind
[#65](../tickets/corpus-coverage.md#t65) at every mode and [#189](../tickets/corpus-coverage.md#t189)
at tp4. Those are other tickets' to close, and #262 says so.

### The host

nebius-gpu, `dmitry@89.169.109.150`, an L40S with 46 GB — not shad-gpu, which is unreachable and
is why every CI GPU job in this chain has failed. Chain K's board note in `tasks.md` carries the
rules and overrides this spec's "No GPU" section and `build-test.md` where they disagree. The
load-bearing ones: work in `~/peacockdb-K` and never `~/peacockdb-J`; rsync the tree uncommitted;
`build-test-shadgpu.sh --build` and then run the staged binaries directly, never `--run`, which
ssh-es to shad-gpu; cuDF 25.02; sf40 and the benchmarks are out. Probed at 15:20: card idle,
37 GB free, sf1 data already in `~/peacockdb-K/testdata`, no source tree there yet.

### The GPU run, 2026-10-09 on nebius-gpu

All five of `tpch/distinct-functions`' device cells are **enabled and green**. The line and
registry row changed in three ways, each one earned by a run rather than guessed:

| round | the line said | result |
|---|---|---|
| 1 | `golden_exact`, `schema_validation_enabled` | 0/5 — three tp4 modes refused on schema, two tp1 modes red on `stddev` |
| 2 | `golden_exact`, `schema_validation_disabled` // #225 | 0/5 — every mode past the hook, all five answers byte-identical, red on `stddev` alone |
| 3 | `golden_approx_std`, `schema_validation_disabled` // #225 | **5/5** |

**Round 1, the three tp4 modes.** The driver's output hook refused the outer init's batch:

    GpuAggregate lane 0: the output hook refused a batch:
      5 stddev(DISTINCT lineitem.l_quantity)$count: Int64 vs stddev(DISTINCT lineitem.l_quantity) INT64;
      6 …$mean: Float64 vs stddev(DISTINCT lineitem.l_quantity) FLOAT64;
      7 …$m2:   Float64 vs stddev(DISTINCT lineitem.l_quantity) FLOAT64

That is [#225](../tickets/corpus-coverage.md#t225) and nothing else — the device holds a Welford
state's three columns under the alias. It is also the most informative line in the run, because
the divergence enumerates *every* mismatching column, and it named three of the node's eleven.
So the other eight held as declared, position 9 among them: the outer init's
`avg(lineitem.l_extendedprice)$sum`, declared `Decimal128(35, 2)`. The widened decimal state the
lowering introduces is device-correct.

The two tp1 modes reached the result instead, because at one lane the outer stage is a single
`GpuAggregate` that inits *and* finalizes, so no state batch is ever emitted. That asymmetry is
why `schema_validation_disabled` costs something here and is still right: `corpus_cases.inc`'s
own header says a cell red on schema alone takes that flag with its ticket on the line, and
`tpch/shuffle-stddev` is the precedent. It comes back on when #225 lands (chain L's
aggregate-arms).

**Round 2, the answer.** With the hook off, all five modes answered identically, and the one
cell that differs from the cpu golden is `stddev_distinct_qty`:

    device (all five modes):  A 14.577379737113251   N …251   R …251
    cpu golden (tp4-sized):   A 14.577379737113253   N …251   R …253

1.4e-16 relative — one ULP, in the two groups where the cpu's own value differs from its third.
Welford association order, `tpch/shuffle-stddev`'s cause and `tpch/shuffle-stddev`'s remedy: the
`golden_approx_std` oracle, whose 1e-11 is about five orders above the difference. Not a ticket — a
ULP is not a wrong answer. Everything else matched to the digit at every mode, `sum_distinct_qty`
`1275.00` and `avg_distinct_qty` `25.500000` and `avg_price` `38273.129734` included, and so did
the plan shape, the per-node `in_rows`, the batch lists and the bytes: `assert_section` runs
before the result compare, and passed at the two tp1 modes in round 1 and at all five from round
2 on — the three tp4 modes never reached it in round 1, since the hook refuses mid-run.

### The decimal `sum` companion's cast-back: no device can see it either

The spec's signoff says the cast-back is "pinned by a plan test alone" and that "the device that
can has not run". The second half is wrong, and this run is what shows it. Two independent
reasons:

- **`distinct-functions` does not reach the cast.** The cast fires for a `Sum`, `Min` or `Max`
  read off state the outer init widened (`translator/aggregate.rs`, `state[0].data_type() !=
  out_type`). This query's companions are `count(*)` and `avg`, and its own `sum(DISTINCT
  l_quantity)` reads `__distinct_arg` `Decimal128(15, 2)`, whose sum state is `Decimal128(25, 2)`
   — equal to what DataFusion declares, so no cast. The goldens show the finalize as a bare
  `` `sum(DISTINCT lineitem.l_quantity)`@2 ``. No corpus query has a plain decimal `sum`
  companion beside a DISTINCT; `a_decimal_sum_companion_casts_its_finalize_back_to_the_declared_type`
  reaches it with `sum(c_acctbal)` and is still the only thing that does.
- **A device could not show it if it did.** The cast changes a decimal's *precision* and nothing
  else — `(35, 2)` to `(25, 2)`, same scale, same value. `test_support/device_schema.rs` says it
  in its header: "Precision, timezone and nullability are labels the export carries and the
  device does not hold." Every device read in the harness is a `type_id` and a scale, so both
  shapes read identically. A device case for it would be a guard that cannot go red.

So the plan test is not a stand-in for a device reading; it is the only reading there is. Nothing
is owed here, and the signoff sentence is the thing to correct.

**Four clauses of the signoff the GPU half falsified**, all of them to be rewritten at the
completeness pass, since the signoff is the spec's one permitted later write:

1. "One shortcut, and it is the chain's: no GPU" — the chain has one now, nebius-gpu.
2. "The C++ change … is compiled and not run" — `peacock_gpu_tests` (4) and `peacock_plan_tests`
   (56) ran green on nebius-gpu.
3. "every new gpu cell is off under #262" — distinct-functions' five are on; q28's and
   rollup-distinct's are still off, on their own tickets.
4. "the device that can has not run" — a device has run, and the reason the plan test stands
   alone is better than the one the signoff gives: no device reading can see a decimal's
   precision at all.

The conclusion survives: the cast-back is pinned by a plan test alone.

### What ran

Device, in `~/peacockdb-K` after `. ~/peacock-env.sh`, with
`LD_LIBRARY_PATH=$PWD/cpp/install/lib:$HOME/data/miniforge3/envs/rapids-cuda-12.2/lib` and
`PEACOCK_TESTDATA_DIR=$PWD/testdata`, every rust binary `--test-threads=1`:

| binary | result |
|---|---|
| `cpp/install/rust-tests/test_gpu_corpus` | 33 passed, 0 failed (31 cells, the no-golden case, registry↔CSV) |
| `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::` | 536 passed, 628 filtered out |
| `cpp/install/rust-tests/test_node_timing` | 1 passed |
| `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_` | 8 passed, 3 filtered out |
| `cpp/install/bin/peacock_gpu_tests` | 4 passed |
| `cpp/install/bin/peacock_plan_tests` | 56 passed |

Cpu, local, because the rebase onto master `bc9b6e2f` carried `cost-report/src/main.rs` and
`pipeline.yml` and the branch had not been re-proved on them: `--lib` 622 passed / 2 ignored,
`test_cpu_corpus` 567, `test_corpus_goldens` 26, `test_cost_model` 3, `test_ci_coverage` 9,
`cargo test -p cost-report` 38; the ffi rung through `scripts/cargo-cudf.sh` — `--lib --
ffi_tests::` 4, `the_payload_golden` 2, `peacockdb-ffi --test test_ffi` 3; and the C++ build
against cuDF 25.02 with `ctest -L cpu` green and `peacock_cpu_tests` 12.

Warnings. The device build reports none from cargo and two from the vendored flatbuffers
(`_deps/flatbuffers-src/src/reflection.cpp`, gcc-12's `-Wstringop-overflow=` inside
`<bits/stl_algobase.h>`) — third-party and not this branch's. Locally, one dead-code warning,
`sha_links is never used` in `cost-report/src/main.rs`; it is byte-identical at `bc9b6e2f`, so it
is master's, not the rebase's.

Deferred under the board note, blocking nothing: the sf40 pair `peacock_tpch_tests` and
`peacock_tpchv_tests`, `--run-benchmarks` (the three `bench_` cases, filtered out above), and
Nsight.

### Host notes for the next round

- **Keep run logs out of the synced tree.** `rsync --delete-after` removed `build-K.log` on the
  next sync, because it is untracked and not gitignored. Later logs went to `~/K-logs/`.
- **The first sync deleted `~/peacockdb-K/testdata/pbench.sf1`** — chain J's dataset, hard-linked
  into chain K's `testdata/` and named in neither chain K's tree nor its `.gitignore`. Chain J's
  own copy is intact, because the files were hard links. Chain K needs no pbench data.
- **Disk.** 37 GB free before the first build and 22-26 GB across the four incremental rebuilds
  after it; chain K's `target-cudf-rapids-cuda-12.2` settled at 11 GB against chain J's 17 GB.
  `~/miniforge3/bin/conda clean -a` found nothing removable — the 13 GB `pkgs` tree is all
  hard-linked into the two envs — so chain J's cleanup rule has no slack left on this host.
  Nothing was removed.
- The card was idle throughout and no run met a memory failure; the whole device tier takes about
  a minute of GPU time. The first build was ~20 minutes (C++ ~4, ccache warm from chain J's
  identical sources); each corpus-line rebuild was under two. The table above is the last of the
  two full tier runs, taken after the final sync so it reads the committed tree.

### Drift this run found, for the human

Chain L's `grouping-id.md` is frozen and at state `new`, so this run did not touch it, but two of
its statements are now false: "**#262's rows:** `tpch/distinct-functions` (off at all five device
modes on `262` alone…)", and its task 4, "**#262, the proof.** A walk test of a two-stage DISTINCT
at `TWO_LANES`…". distinct-functions is on at all five device modes and #262 now covers q28 and
`tpch/rollup-distinct` only. `grouping-id-impl.md` carries the same assumption in several places.
