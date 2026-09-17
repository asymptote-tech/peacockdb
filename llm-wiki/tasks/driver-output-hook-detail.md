# driver-output-hook — run record

Chain B, task 6, the last. Branch `ENS-driver-output-hook` off `ENS-device-schema-harness` at
`9375a52d`; PR targets `ENS-device-schema-harness`. Tasks 1–5 are `done` (PRs #158–#162 green)
awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: task 5's `test_support::schema_of(&GpuBatch)` (`pub(crate)`, `not(rust-only)`)
  and `device_divergence` are what the validator composes; `GpuBatch::executor()` exists. The
  registry has 26 enabled device cells over 14 queries; every one gets
  `schema_validation_enabled`. The join-batching ticket is #220; the next free ticket number is
  226. Tasks 1–4 cleared the known type classes, so the spec expects no cell to need
  `schema_validation_disabled`; one that does is a ticket, never a rewritten expectation.
- Routing: the developer works `driver-output-hook-impl.md` — the hook with its four mock
  tests red then green (rust-only), the validator and its cpu end-to-end test, the corpus
  argument, one shad-gpu cycle over the enabled corpus with validation on, the ad hoc trigger
  run recorded here and not committed, then the record.
- Restriction: the hook is the only production change; with `None` the driver does exactly
  what it did; no validation code outside `test-support`; `git diff executor/driver/` is the
  hook and nothing else.

## Developer notes
