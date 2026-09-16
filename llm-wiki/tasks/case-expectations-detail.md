# case-expectations — run record

Chain E, task 3. Branch `ENS-case-expectations` off `ENS-aggregate-cases` at `15a713b9` (tasks
1 and 2 done, PRs #155 and #156 green, both rebased onto master `302d91dc` today); PR targets
`ENS-aggregate-cases`.

## Dispatch 1 — 2026-09-16

- Hosts: **verda down** (`Could not resolve hostname`), so the rust-only proofs run locally;
  the sf1 symlinks in this worktree resolve. **shad-gpu up**, 0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from aggregate-cases' Task 8; `cpp/build26`
  absent, so `--build` re-runs cmake there (minutes, not the cold hour).
- Routing: the developer works `case-expectations-impl.md` task by task — the comparator, the
  composite-key pin, the fixture, the record — with the rust-only red-then-green for Task 1
  local, and one shad-gpu device cycle over `_cases` (foreground `--build`,
  `--push-binaries --patch`, `--run`, each under a timeout) proving Tasks 2 and 3 together;
  then the whole `gpu_tests::` rung once as the handoff run. Rust-only `--lib` and
  `test_module_layout` local.
- The spec's restriction is the one to hold: a case that goes red under a tightened
  expectation is a ticket and a `bug_`, never a fixture restored, and `WELFORD_RELATIVE` does
  not move.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, harness gaps)
