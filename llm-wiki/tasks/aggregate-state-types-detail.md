# aggregate-state-types — run record

Chain B, task 3. Branch `ENS-aggregate-state-types` off `ENS-decimal-precision-at-export` at
`d5fa0d1a`; PR targets `ENS-decimal-precision-at-export`. Tasks 1 and 2 are `done` (PRs #158,
#159 green) and await the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down** (name resolution), so rust-only proofs run locally; **shad-gpu up**,
  0 MiB of 144 GiB held.
- Caches: `target-cudf-rapids-cuda-12.2` warm from task 2's cycles; `cpp/build` present.
- Pre-dispatch: the three `bug_` pins the spec names are all in
  `tests/gpu_tests/aggregate_cases.rs` (`:268`, `:639`, `:679`), not at the spec's line numbers;
  23 registry rows carry `163`. Master's #216 ("the device's global aggregate has no Welford
  arm", `tickets.md`) arrived with the rebase and is adjacent: its two pins in
  `aggregate_dimension_cases.rs` were converted to export refusals by task 2 and are not this
  task's — this task's Welford change is the cpu's count cast, the device changes nowhere.
- The join-batching ticket is **#220** (master took #215 during the chain's rebase); the
  registry rule for this rollout names #185, #220 and whatever the 23 rows show next.
- Routing: the developer works `aggregate-state-types-impl.md` task by task — `state_type` and
  its table tests red then green, `decompose` deriving, the cpu's Welford projection, the
  finalize cast, the goldens regen (three classes and no other), then one shad-gpu cycle for
  the harness and one for the rollout at `tp1_single` and the other four modes, then the record.
- The spec's restriction holds: output types stay DataFusion's; `Sum`'s decimal rule quoted,
  not reinvented; `aggregate.cpp` untouched; nothing casts a count to `UInt64`.

## Developer notes
