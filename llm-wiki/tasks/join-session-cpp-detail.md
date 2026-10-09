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
