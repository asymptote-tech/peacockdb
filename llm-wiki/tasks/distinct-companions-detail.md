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
