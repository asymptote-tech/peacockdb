# join-session-cpp — run record

Spec: [`join-session-cpp.md`](join-session-cpp.md) (frozen). Plan:
[`join-session-cpp-impl.md`](join-session-cpp-impl.md), 12 tasks, written at chain approval
(`38d5f2de`) and not yet touched by a developer. Design:
[`join-rewrite-design.md`](join-rewrite-design.md) §1, §2, §3, §5.6.

## Branch

`ENS-join-session-cpp`, forked off `ENS-exit-copies` at `c4d55aa7` on 2026-10-09. Its PR targets
`ENS-exit-copies`, which is already on the remote at that SHA. Seventh of nine; `join-backend`
and `verify-26.02` fork above it and nothing has branched yet.

## Environment, measured 2026-10-09 at dispatch

- **nebius-gpu `dmitry@89.169.109.150` is up**: L40S, 45458 MiB free of 46 GB, 37 GB disk free of
  96 GB, and `~/peacockdb-J/testdata` holds the sf1 data generated 2026-10-08. The board's host
  override governs: device work there, every CPU build and run local.
- **verda is unlocatable** — `scripts/list_verda_instances.sh` exits on an unset `VERDA_CLIENT_ID`,
  so there is no address to hand a developer. Every CPU run this task is local.
- **shad-gpu is down** and has been all chain. `gpu-tests` ("GPU Tests (remote)") is the one CI
  job the override exempts from `done`.
- **`cpp/build` in this worktree is an empty root-owned directory**, and cmake dies at configure
  blaming itself rather than the permissions. Nobody in the run can `sudo`. Use
  `/tmp/dkb-cppbuild`, which is configured against this worktree
  (`CMAKE_HOME_DIRECTORY=/home/dmitry/workspace/peacockdb-alpha/cpp`) and survives rounds:
  `cmake --build /tmp/dkb-cppbuild --target peacock_gpu peacock_plan_tests peacock_cpu_tests -j 8`
  is a 30-second compile check before paying for a device cycle. The new
  `peacock_join_session_tests` target wants adding to that command once it exists.

## What this task inherits from the six below it

- **`TableResult` is final and may not change** (refcounted-scatter, PR #173). One shared owner
  per column; `owning`, `slice`, `select`, `with`, plus the three public vectors. §3.0's `emit`,
  which combines columns from two gathered tables, has no constructor of its shape and is served
  by hand assembly through the public fields — `project.cpp` is the worked example.
- **`register_handle` is the only path to a handle number**, and refuses a handle of no columns or
  whose names or owners do not number its columns. Hand assembly is fine; the handle must be
  complete before registration.
- **`join.cpp`'s ten exit copies are this task's.** exit-copies (PR #176) took the eleven outside
  `join.cpp` and reworded #154 to exactly these ten, each with its kind. The classification is
  load-bearing and was a finding against the wiki twice: "ordinal subset of a fresh table" (four
  sites, `select`) is not "an input column kept in the output" (one site, `project.cpp`'s shape).
- **The gtest idiom to copy** is `allocated_by` in `cpp/tests/gpu/test_plan_executor.cpp`, with the
  RMM statistics adaptor: every bound measured against the same cuDF work on the same input rather
  than a formula, and `net` as well as `total`. Bound the release too — seven allocation bounds
  could not see an operator retaining what it should drop, and with both operators made to leak,
  75 of 77 cases still passed. The spec's item 6 wants one allocation check on a session probe.
- **CI runs every installed `peacock_*_tests` by glob** with a ran-any assertion, and treats
  `PASSED 0 tests` as an error — so `peacock_join_session_tests` runs on `gpu-tests` as soon as it
  is in `install(TARGETS …)` and the `INSTALL_RPATH` list, with no workflow edit. `test_ci_coverage`
  guards Rust targets only; the C++ loop needs no list.

## Deferred under the override

Nothing in this task's verification bar needs the sf40 binaries, `--run-benchmarks` or an Nsight
capture, so the override's large-test exclusion costs it nothing. Recorded here so the
completeness pass need not re-derive it.

## Rounds

- **Round 0, 2026-10-09** — board to `building`, developer dispatched against the 12-task plan.
- **Developer green, 2026-10-09** — 47 gtests / 135 hand-counted cases on nebius-gpu's L40S at
  cuDF 25.02, the existing gpu tier unchanged (2 + 77 + 95 + 1 + 580 + 8). Committed `46a2ab84`,
  pushed, PR **#177** against `ENS-exit-copies`, 2 commits. Board to `reviewing`. The developer's
  own breakdown of §5.6 onto case names, the two defects it found, and the three things it flagged
  for a reviewer are above in this file.

## Developer round 1, 2026-10-09 — the device cycle that works

`/tmp/dkb-devcycle.sh` (scratch, not in the tree) is the cycle this run used, ~90 s each:
rsync the uncommitted tree per the override, then on nebius-gpu
`cmake --build cpp/build --target peacock_gpu peacock_join_session_tests`,
`cmake --install cpp/build --prefix cpp/install`, and run
`cpp/install/bin/peacock_join_session_tests` with the override's `LD_LIBRARY_PATH` and
`PEACOCK_TESTDATA_DIR`. `build-test-shadgpu.sh --build` also stages the cargo test binaries,
which this task's bar does not need per cycle; the whole-file cycle at the end uses it.

**There is no local GPU** (`nvidia-smi` fails on this workstation), so every red/green is a
device cycle. `/tmp/dkb-cppbuild` still catches every compile error for free, and a link error
against a missing symbol is the cheap red for a new ABI call.

### Plan deviations, each deliberate

- **`register_join_output` is a method on `NodeSession::Impl`, not on `NodeSession`.** The plan
  put it in `plan_executor.h` taking a `RegionSink*`, but `RegionSink` lives in an anonymous
  namespace in `node_session.cpp`; a declaration in the header would name a different type.
  `Impl` is defined in the .cpp, so it can take it.
- **`emit`/`emit_probe_only`/`emit_build_only` are two functions, not three**: `side_by_side`
  (the hand assembly of §3.0's `emit`, `project.cpp`'s shape) and `projected` (the
  `projection`). A one-side type calls `projected` alone.
- **A present empty `projection` is refused in the constructor**, not at the first emit: a type
  that answers only at finish would otherwise reach the refusal a call late. The message says
  "a projection of zero columns".
- **An empty build needs no `finish` arm.** `matched` is built before the `empty_build` return,
  so at zero build rows Left/Full pad nothing, LeftSemi/LeftAnti answer zero rows and LeftMark
  appends a zero-row mark through the ordinary path.

### The plan's order was changed in one place

**Task 8 (hoisting) was implemented after Tasks 9 and 10, not before them.** The plan's Task 8
cases are nested loops, which Task 9 builds; and hoisting is an optimization — it moves no row
and changes no answer — so every one of them passes with hoisting absent, through the chunked
cross. Writing them before Task 9 would have been three tests that cannot go red. Done in the
order 9, 10, 8, each arm's cases red first.

**What made hoisting testable.** Only what a call allocates can show it happened:
`HoistingKeepsANestedLoopOffTheCrossProduct` runs the same nested loop twice over 2000 × 2000
rows, once with `upper(b_s) = p_s` (hoistable) and once with a build column the test upper-cased
itself (AST-able as it stands), and holds the first to the second's peak plus the build side's
own measured bytes. Measured: without hoisting, peak 133,616,192 against a bound of 2,364,896 —
red by 56×. With it, 2,272,128 against the same bound.

### Two defects this task's own tests found

- **The keyed pairs path evaluated the wrong conjunct set.** `keep_and_derive` was factored out
  in Task 9 taking `State::R` — the conjuncts the AST had refused — which is right for the
  nested loop and wrong for the keyed pairs path, where the key match alone made the candidates
  and *every* cross conjunct must be applied. It was green only because every residual under
  test then happened to be non-AST-able (`p_w` Int64 beside `b_lim` Int32), so `R` equalled
  `cross`. Task 8 made `R` empty for a keyed pair type and the four outer-residual cases went
  red. `keep_and_derive` now takes the conjunct list as an argument, and
  `AnOuterResidualOfMatchingTypesIsStillApplied` pins the case that was silently broken: the
  same residual with both operands Int64, which the AST *can* take. Red before the fix,
  verified by reverting the one line.
- **Two zero-column guards could not go red.** The constructor's "a build side of zero columns"
  and `probe`'s probe-side twin were unreachable: every route into a handle passes
  `TableResult::owning`, which refuses a table of no columns first, and an absent build side is
  typed through `null_table`, which calls `owning` too. Both were dropped;
  `ASideOfNoColumnsIsRefused` pins the reachable form (an absent build whose `build_schema` has
  no fields) and the message that answers it.

### Why `join_session.cpp` became four files

At the end of Task 10 it was 1080 lines, over `coding-style.md`'s 1000. Split by
responsibility, none of the four over 600 lines:

| file | responsibility |
|---|---|
| `join_columns.{h,cpp}` | §3.0's column-level pieces: index maps, the matched bit, typed pads, the side-by-side output, the two `size_type` ceilings |
| `join_residual.{h,cpp}` | §3.4 and §3.6's residual: its conjuncts, which sides each reads, the per-side and per-pair evaluation, the AST assembly, the chunk budget |
| `join_session.{h,cpp}` | `State`, the lifecycle, and one arm per join type |

`join_session.cpp` opens `using namespace join;` — the other two exist for it alone, and
qualifying forty call sites would say nothing a reader of that file does not know.

### Deviations from the plan's letter, beyond the above

- **`fits_or_throw` and `cross_rows_or_throw` are declared in `plan_executor_internal.h`**, not
  in `join_session.h` as the plan said. exit-copies' round-1 finding made that header the
  declared route for what a test reaches into, and it was the only include of `cpp/src/peacock/`
  by any test in the tree; a new private-header reach would reopen it. Each entry there carries
  a "why a test reaches this" note and these two do too.
- **`Conjunct` is not a struct with `reads_build`/`reads_probe`/`ast_able` fields.** The sides a
  conjunct reads decide which of three vectors it lands in at construction and are never read
  again, so the vectors carry bare `const fb::Expr*`. `AstConjunct` carries what the AST needs.
- **A hoisted conjunct's AST-ability is asked of `cudf_ast_can_evaluate`**, over a two-column
  FlatBuffer expression built for the question, rather than restated as "equal types and not
  decimal". The AST nodes still come from `hoisted_ast` directly, as the plan requires; the
  FlatBuffer exists only so the one routing rule answers for a hoisted comparison too.
- **`probe_nested` is three functions**, not one: row-wise (`A` non-empty, `R` empty, not Full),
  candidates (`A` non-empty with something left over, and every Full), and the chunked cross of
  indices (`A` empty). The three share `derive`, so no two can answer one type differently.

### The case count, hand-counted

**47 gtest cases, 135 hand-counted cases.** The second number is what §5.6's matrix asks for: a
table-driven case is counted once per (type, condition, shape) it runs, which is what the loops
inside the gtests expand to. The two that account for most of the gap:
`ANestedLoopAnswersEveryTypeOnTheAstAndOffIt` is 9 types × 3 condition forms = 27, and
`AnEmptyOrAbsentBuildAnswersEveryType` is 9 types × {no batch, zero rows} = 18. The rest:
`NoProbeBatchStillFinishesFromTheBuildSide` 5, `TheSemiFamilyOverACrossResidualAnswersTheSame\
EitherPath` 10, `ASemiJoinWithTwoCrossConjunctsAndsThemAtTheAstLevel` 6,
`AKeyedSemiJoinAnswersTheSameWithAHoistedConjunctOrThePairsPath` 6,
`APredicateFreeNestedLoopOverAnEmptySide` 6, `AnOuterResidualDecidesUnmatchedAfterTheFilter` 4,
`LeftSemiAntiMarkAnswerOnlyAtFinishEachBuildRowOnce` 3, `ANullPreservedSideConditionIsNoMatch`
3, `ANestedLoopOverNoProbeBatchFinishesFromTheBuild` 3, and eight two-case loops
(`RightSemiAntiAnswerPerBatchOverDuplicateBuildKeys`,
`ARightSemiAntiResidualOnOneSideAtATimeNeedsNoPairs`,
`ANestedLoopWhoseHoistedComparisonIsStillRefusedTakesTheCross`,
`ANestedLoopSplitsItsConjunctsBetweenTheAstAndTheColumns`,
`ACrossJoinOverAZeroRowSideIsOneZeroRowTable`,
`AnAbsentProjectionKeepsEveryColumnAndAnEmptyOneIsRefused`,
`AResidualAntiAndMarkOverNoProbeBatchAnswerFromTheBuild`,
`ARightSemiOverAnAllNullBuildAnswersNothingAndRightAntiEveryRow`). The remaining 24 gtests are
one case each.

Three cases were added after a dimension-by-dimension read of §5.6 against the file found them
missing, and each passed on its first run — they are coverage of behaviour earlier arms already
had, not new features:

- `ANestedLoopSplitsItsConjunctsBetweenTheAstAndTheColumns` — the matrix's "no keys + AST and
  non-AST conjuncts". `probe_nested_candidates` was reached only by Full with nothing left over;
  the `A`-non-empty-and-`R`-non-empty path had no case.
- `ANestedLoopOverNoProbeBatchFinishesFromTheBuild` — §5.6's "a Left nested loop over no probe
  batch". The existing case probed with a zero-row batch, which is a different call.
- `ASideOfNoColumnsIsRefused` — §5.6's "a zero-column side refused", in the one form the ABI can
  reach (see the second defect above).

### What §5.6 asks for and where each dimension is answered

- **type**: all nine with rows (`AnInnerJoin…`, `LeftPads…`, `RightPads…`, `FullNeverReemits…`,
  `LeftSemiAntiMark…`, `RightSemiAnti…`), all nine over an empty build
  (`AnEmptyOrAbsentBuild…`), all nine keyless (`ANestedLoopAnswersEveryType…`).
- **condition**: keys only; keys + cross residual on the AST (`TheSemiFamily…` ints,
  `AnOuterResidualOfMatchingTypes…`); keys + cross residual off it (`TheSemiFamily…` decimals);
  keys + preserved-side-only residual (`ANullPreservedSideCondition…` build side,
  `ARightSemiAntiResidualOnOneSide…` probe side); no keys + AST; no keys + both; no keys +
  nothing AST-able; cross.
- **keys**: NULL on both sides under UNEQUAL and under EQUAL
  (`NullEqualsNullMatchesNullKeys…`); duplicates on both sides; a composite key with a NULL in
  its second column.
- **build**: no batch, zero rows, rows. **probe**: no batch, one, three with a zero-row batch
  between (`FullNeverReemits…`). **projection**: absent, crossing sides, empty and refused.
- A NULL probe key against the two-argument `hash_join(Bk, cmp)` does not throw: every outer and
  semi case probes with one, which is the check §5.6 asks for.

### The run, 2026-10-09, nebius-gpu `dmitry@89.169.109.150`, cuDF 25.02

Device, after `./scripts/build-test-shadgpu.sh --build` on the host (uncommitted tree rsynced
per the override), every binary run from `cpp/install/`:

| binary | result |
|---|---|
| `cpp/install/bin/peacock_join_session_tests` | **47 passed** |
| `cpp/install/bin/peacock_gpu_tests` | 2 passed |
| `cpp/install/bin/peacock_plan_tests` | 77 passed |
| `cpp/install/rust-tests/test_gpu_corpus` | 95 passed |
| `cpp/install/rust-tests/test_node_timing` | 1 passed |
| `cpp/install/rust-tests/peacockdb_core_gpu_lib gpu_tests::` | 580 passed |
| `cpp/install/rust-tests/peacock_gpu_benchmarks --skip bench_` | 8 passed |

The last four are the existing gpu tier, which this task must leave unchanged: they are what
prove `JoinFilterColMap` becoming a span did not move the semi, mark or nested-loop answers
`join.cpp` still serves. Local: `peacock_cpu_tests` 15 passed, `cargo check -p peacockdb-ffi`
clean, and the C++ build with no warnings. The sf40 pair and `--run-benchmarks` are the
override's deferred items and were not run.

Two measurements the cases print, for the next reader (both on the L40S, 1 GiB pool):

    [hoist] build bytes 27232 | plain peak 2272128 total 2280336 | hoisted peak 2272128 total 2280592
    [join exit] inputs 33554432 | probe peak 41943040 total 41943232 net 16777216

The exit figure is the whole of the no-copy argument: a 2^20-row 1:1 Inner join over four int64
columns allocates 32 MB of output plus 8 MB of INT32 index maps and nothing else, and leaves
16 MB behind (the output, the probe batch having been consumed inside the call). A deep copy at
the exit would put the peak at 72 MB against a bound of 56 MB.

### `test_join_session.cpp` is one file at 1595 lines, deliberately

`coding-style.md`'s 1000-line limit is not waived for tests, but the C++ gtest tier keeps one
file per binary — `test_plan_executor.cpp` is 2898 — because a binary's `main()`, its RMM pool
and its harness are one unit, and a second translation unit in the same target would need the
harness in a header of its own. The three production files are each under 600 lines, which is
where the limit does its work. If this file is ever split, the seam is the harness (`Session`,
`JoinSpec`, `make_plan`) against the cases.

## Review round 1, 2026-10-09 — 1 blocking, 2 important, 7 nits

The reviewer traced the keyed pairs path, the nested-loop trio, the empty-build arms, the chunk
loop and every cuDF join object's lifetime against §3 and the vendored 26.02 headers, and found no
wrong answer in the session itself. All three of the developer's self-reported claims held up under
independent check, and so did the `plan_executor_internal.h` placement. What it found instead:

- **Blocking — the new wire kind turns a Rust guard red.** `CudfJoin` in `union PlanNodeKind`
  regenerates `fb::PlanNodeKind::ENUM_VALUES`, and `wire/read.rs`'s `children` has no arm for it,
  so it falls to the catch-all. `wire::tests::the_child_walk_names_every_node_kind` iterates
  `ENUM_VALUES` asserting `walked.is_ok()`, and exists for exactly this. Rust-only tier, so every
  CI leg runs it. It was green in the record because the round ran `peacock_cpu_tests` and
  `cargo check -p peacockdb-ffi` — no Rust test suite. One arm, mirroring the C++ twin already
  added at `node_session.cpp:324-327`; `read.rs` is not the wire writer, so the spec's restriction
  holds.
- **Important — a literal `true` filter with no `filter_columns` is refused**, and the comment this
  branch added at `expr.h:44-46` says the opposite. §4.1 and §3.6 make every predicate-free
  non-Inner join arrive with the literal `true`, so `tiny LEFT JOIN empty ON true` throws at
  `join_build`. Nothing in the session needs the map for such a filter — the reviewer checked
  `sides_read`, `cond_table`, `cross_ast` and `filter_type_table` one at a time. The test helper
  hides it: `unconditional()` supplies an entry nothing reads.
- **Important — the one allocation check bounds the allocation but not the release**, and its peak
  bound is a hand formula. Measured `inputs 33554432 | peak 41943040 | net 16777216`: a session
  that retained the probe batch would move `net` to 32 MB and `peak` not at all, and both
  assertions stay green. That is exit-copies' round-1 mode exactly. A session is the one operator
  holding state across calls, so it is the highest-value place in the tree for a release bound.
  `ReleaseWithoutFinishAndEndPlanFreeTheSession`, which §5.6 names as "release frees everything",
  measures nothing.

Nits worth taking while the developer is in the code: a duplicate `<cudf/reshape.hpp>`; three
`chunk_bytes = 16` forms that cannot chunk because `chunks()` caps `k` at the probe's row count and
they probe one row; `NullEqualsNullMatchesNullKeysForEveryOuterType` running `Full` alone; the fbs
`projection` comment not saying what the ordinals index, which differs per join type and is what
task 8 writes against; a build-side ordinal overrun throwing unnamed where the probe side is named;
`join_columns.h` silent on the two `plan_executor_internal.h` helpers it makes visible; and three
`generated.rs` line counts the new union variant falsified.

## Developer round 2, 2026-10-09 — review round 1 addressed

The blocking finding and both important ones are fixed; all seven nits taken. **48 gtests,
140 hand-counted cases** (was 47/135: looping `NullEqualsNullMatchesNullKeysForEveryOuterType`
over its four types adds 3, and §4.1's own example adds a gtest of 2 shapes).

### Blocking — the new union variant turned a Rust guard red

`peacockdb-core/src/wire/read.rs` gained one arm, `fb::PlanNodeKind::CudfJoin => Vec::new()`,
beside the other joins and commented as the leaf it is. Watched red first, locally:

    thread 'wire::tests::the_child_walk_names_every_node_kind' panicked at
      peacockdb-core/src/wire/tests.rs:974: CudfJoin is missing from the child walk

**No sibling guard exists** — `wire/tests.rs:956` is the only `ENUM_VALUES` iterator in the
tree, which the coordinator confirmed by its own grep. One match over `PlanNodeKind` does have
the same *shape* without being a guard: `wire/fb_text.rs`'s `payload_text` ends in `_ => {}`,
so a `CudfJoin` renders no fields in `recipe-payloads.txt`. Nothing is red, because nothing
writes the node; **join-backend's writer is what makes that arm reachable**, and it will want
one when its payload golden comes out empty. Left alone here deliberately: with no writer and
no golden, which of the nine fields to print would be a guess.

The lesson for the round record: this round ran `peacock_cpu_tests` and
`cargo check -p peacockdb-ffi` and called the CPU side proved. A `cargo check` compiles; it runs
no test. **An fbs change is a Rust change**, because `peacockdb-core/build.rs` regenerates the
bindings on every build — so the rust-only tier is part of re-proving any `gpu_plan.fbs` edit,
whatever the task's restriction says about Rust *behaviour*.

### Important 1 — the `filter_columns` throw refused the shape the chain exists to fix

`split_residual` threw on an empty filter-column map. Design §1.1 declares the map as
"filter schema ordinal -> (side, column)", and the literal `true` that every predicate-free
non-Inner join carries (§3.6, §4.1) has no column references and therefore no ordinals — so an
empty map is the *correct* encoding, and the throw refused `tiny LEFT JOIN empty ON true`,
§4.1's own example of the defect this chain removes. The throw is gone; `expr.h`'s comment was
right as written and `col_map_of`'s null branch now has the live caller it claims.

Checked path by path before dropping it, and nothing needs the map for such a filter:
`sides_read` is reached only on the keyed-semi branch and returns at once for a literal;
`cond_table` returns before touching the map when nothing is hoisted; `build_expr` never
indexes it for a `LiteralExpr`; `filter_type_table` over an empty map is a zero-column table
`cudf_ast_can_evaluate` only reads types from. A filter that *does* name a column with no map
is still refused by name, from `build_expr`'s and `sides_read`'s own bounds checks.

The test helper `unconditional()` now passes **no** `filter_columns`, so the two predicate-free
cases prove the shape join-backend will emit rather than passing over an entry nothing reads;
and `APredicateFreeLeftNestedLoopOverAnEmptyBuildOwesItsPaddedRows` pins §4.1's example with its
own number — a build of 8 rows against an absent and a zero-row probe, owing 8 padded rows.
No design text changed: it already said the right thing.

### Important 2 — the allocation bounds now bound the release, and the peak is measured

Three changes, each proved red by a deliberate leak on the device:

| bound | leak that proves it | measured |
|---|---|---|
| `net` upper bound on a probe | the session pushes a copy of every batch into `State` | net 33,554,432 against 16,842,752 — **and the peak did not move**, which is why no peak could have caught it |
| peak against the same cuDF work | a deep copy of every gathered column at the exit | peak 58,720,256 against the bare work's 41,943,040 |
| release gives the build side back | `join_release` made a no-op | release net 0 against −3,932,160 |

The peak bound was `inputs + inputs/4 + inputs/2`, a hand formula. It is now `bare_probe_of(n)`:
the same four cuDF calls (`inner_join_size`, the sized `inner_join`, two `gather`s) over tables
of the same rows and types, with the hash table built outside the measured scope exactly as the
session builds it in `join_build`. Green, the two agree **to the byte**:

    [join exit] build 16777216 probe 16777216 | bare peak 41943040 total 41943232
                | probe peak 41943040 total 41943232 net 16777216
    [join release] build 4194304 | release net -8389616

`ReleaseWithoutFinishAndEndPlanFreeTheSession` now measures both halves §5.6 names: releasing a
session gives back its build side (and its hash table — 8.4 MB against a 4 MB build), and
`end_plan` under a live second session does the same.

### Nits, all seven

1. The duplicated `#include <cudf/reshape.hpp>` is gone.
2. Three `chunk_bytes = 16` forms probed one row, and `chunks` caps the range count at the
   probe's rows, so each ran identically to its unchunked sibling. All three now probe three
   rows, with the extra rows chosen to leave the expected answers unchanged where that was
   possible and recomputed by hand where it was not (the keyless decimal case now answers
   three rows and an empty finish).
3. `NullEqualsNullMatchesNullKeysForEveryOuterType` ran Full alone; it now loops the four and
   the name is accurate.
4. `gpu_plan.fbs` now says what `projection`'s ordinals index, which differs by type, and that
   an empty `filter_columns` is what a filter naming no column carries. The fbs is what task 8
   writes against, which is why both belong there rather than only in the C++ comment.
5. `filter_type_table` names a build-side overrun as it already named a probe-side one.
6. `join_columns.h` points at the two `size_type` ceilings it defines in its `.cpp`.
7. The three "7,336 lines" counts now read **7,570**, the regenerated truth. Worth noting: the
   number was already stale before this task — HEAD's fbs regenerates to 7,287, so the comment
   was 49 lines out when the task started, and this corrects both drifts at once.

### What round 2 ran

- **Rust, locally** — the tier CI runs: `cargo test --features rust-only -p peacockdb-core --lib`
  **694 passed**, 0 failed (CI's run 37960619791 on `46a2ab84` had 693 passed, 1 failed);
  `--test test_module_layout` 18 passed; `--test test_ci_coverage` 11 passed;
  `--test test_corpus_goldens` 26 passed, which is what proves an fbs comment changed no bytes.
- **Device, nebius-gpu** — `cpp/install/bin/peacock_join_session_tests` **48 passed**.
- **Local C++** — `peacock_cpu_tests` 15 passed; the build with 0 warnings; `clang-format`
  clean on every file touched, `rustfmt --check` clean on `read.rs`.

The existing gpu tier was **not** re-run, deliberately: nothing it can reach moved. This round
touched `read.rs` (rust-only), `join_session.cpp`'s refusal path, `join_residual.cpp`'s refusal
path, two headers' comments, the fbs's comments and the test file. `expr.cpp` and `join.cpp` —
the only files round 1 changed that today's plans execute — were not touched at all.

## Round 2 closed, 2026-10-09 — coordinator's record

Committed `34e6b3f7`, pushed. All three findings closed, each proved rather than asserted: the
child-walk arm was watched red first with CI's own signature, and every allocation bound was proved
red by a deliberate leak on the device. What I verified myself before committing, since a green
measured against a mutated tree is worth nothing: `NodeSession::join_release` erases from both maps
again (the leak that proved the release bound is gone), 48 `TEST` lines in the file, no untracked
scratch files, and `build-test.md`'s header derived from the sum of its two tables' N columns rather
than from a delta — 3080, C++ 164, both matching. The 7,570-line count is the real flatc output in
`OUT_DIR`, not the 11-line `generated.rs` stub that includes it.

**Two things task 8 inherits from this round.**

- **`wire/fb_text.rs`'s `payload_text` ends in `_ => {}`**, so a `CudfJoin` renders no fields into
  `recipe-payloads.txt`. Nothing is red because nothing writes the node — join-backend's writer is
  what makes that arm reachable, and it will want one when its payload golden comes out empty. Left
  alone deliberately: with no writer and no golden, which of the nine fields to print is a guess.
- **An fbs change is a Rust change.** `peacockdb-core/build.rs` regenerates the bindings every
  build, so the rust-only tier belongs in re-proving any `gpu_plan.fbs` edit whatever a spec's
  restriction says about Rust behaviour. Round 1 proved the CPU side with `peacock_cpu_tests` and
  `cargo check -p peacockdb-ffi`, and a `cargo check` compiles without running a test, which is how
  a totality guard stayed red through a round.

One process note, self-reported by the developer: it used `git checkout -- <file>` once, to restore
the deliberate leak. Subagents do not mutate git state; that is the coordinator's. It was benign —
that file's only uncommitted change was the leak — and I confirmed independently that every other
round-2 edit survived, finding by finding.
