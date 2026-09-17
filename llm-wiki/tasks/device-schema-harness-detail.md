# device-schema-harness — run record

Chain B, task 5. Branch `ENS-device-schema-harness` off `ENS-date-part-return-type` at
`60f04207`; PR targets `ENS-date-part-return-type`. Tasks 1–4 are `done` (PRs #158–#161 green)
awaiting the human's merge; the chain sits on master `0a338ead`.

## Dispatch 1 — 2026-09-17

- Hosts: **verda down**; rust-only proofs local. **shad-gpu up**, 0 MiB held. Caches warm.
- Pre-dispatch: `peacock_handle_schema` is in the header and externed in `peacockdb-ffi`
  (task 2), with its two gtests; nothing in Rust calls it yet. Chain E's join-cases builders
  are in the tree. What the four fixes left: no view type anywhere (task 1); the export is
  told each decimal's precision and `Device::fetch` takes the declared schema (task 2); every
  count is `Int64` and a decimal `avg`'s sum is `(p + 10, s)` (task 3); `date_part` answers
  `Int32` (task 4). The join-batching ticket is #220; the next free ticket number is 225.
- The spec's one hard rule for this task: **no spot-check expectation comes from a device
  run** — the eleven rows are written from the plan goldens and DataFusion before the first
  cycle, and a row the device contradicts becomes a `bug_` with a ticket, never a rewritten
  expectation. The coordinator reads every spot-check with that question at review.
- Routing: the developer works `device-schema-harness-impl.md` — the projection and its unit
  tests (rust-only), `schema_of` over the handle, the family suites one at a time with a
  shad-gpu cycle each, the walk hook and the eleven spot-checks, then the record.

## Developer notes
