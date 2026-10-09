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
