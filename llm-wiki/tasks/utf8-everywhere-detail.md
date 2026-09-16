# utf8-everywhere — run record

Chain B, task 1. Branch `ENS-utf8-everywhere` off master at `76f17db2`; PR targets `master`.

## Dispatch 1 — 2026-09-16

- Hosts: **verda down** (`Could not resolve hostname`), so the rust-only proofs run locally;
  the sf1 symlinks in this worktree resolve. **shad-gpu up**, 0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from chain E's runs; `cpp/build26` absent, so
  `--build` re-runs cmake there (minutes, not the cold hour).
- Pre-dispatch grep: `grep -rn "utf8view\|Utf8View\|declaring_view" peacockdb-core/src/tests`
  finds only `harness_cases.rs` (`declaring_view_strings`, the #183 pin) — chain E's Task 8
  retirements are in master, no survivor outside the spec's list. The nine files
  `grep -rln "Utf8View\|BinaryView" peacockdb-core/src` names are exactly the spec's scope
  table; `peacockdb-core/tests/common/corpus_cases.inc:30,48` mention the type in comments
  that the rollout step rewrites.
- Routing: the developer works `utf8-everywhere-impl.md` task by task — the option and the
  rule (red then green, local rust-only), the deletions, the goldens regen, then one shad-gpu
  cycle over `_cases` and one over `gpu_` for the rollout (foreground `--build`,
  `--push-binaries --patch`, `--run`, each under a timeout), then the record.
- The spec's restriction is the one to hold: no cast anywhere; a view type that survives the
  option is a ticket and the rule's refusal, never a conversion. Every golden diff line is
  `Utf8View → Utf8` and nothing else.
- Progress is judged by what reaches this file and the working tree, not by elapsed time.

## Developer notes

(the developer appends here: what was tried, what a finding meant, the rollout table)
