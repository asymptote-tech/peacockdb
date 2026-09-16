# join-cases — run record

Chain `ENS-join-cases`, base `master`. Task 1's branch is the chain branch, `ENS-join-cases`,
forked at `c0660fe6`. PR targets `master`.

## Dispatch 1 — 2026-09-15

- Hosts at dispatch: **verda down** for us (`Permission denied (publickey)` — host answers,
  key rejected; not diagnosed), so the rust-only check runs locally. **shad-gpu up**, a
  neighbour holding ~62 GiB of 144; the gpu lib binary's 1 GiB budget fits.
- Routing: the developer works the impl plan (`join-cases-impl.md`) task by task, one
  shad-gpu device cycle per family, foreground calls (`--build`, then `--push-binaries
  --patch`, then `--run`), never one backgrounded chain. Local rust-only proof:
  `cargo test --features rust-only -p peacockdb-core --lib` unchanged (the spec's bar).
- One target dir per worktree: cudf builds go to this worktree's own
  `target-cudf-<basename CUDF_ROOT>`, never another checkout's.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, harness gaps)
